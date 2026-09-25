-- Endpoint:     GET /search?q=&type=transaction,contract,asset,account,nft,pool
-- Purpose:      Unified search across all entity types. The API classifies
--               the query (hash-shape, StrKey-shape, CODE:ISSUER, plain text)
--               and may restrict via the `type` parameter; each bucket below
--               is one read the API fires only when the classification says
--               it can match, and returns a capped result set.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.15
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Impl:         crates/api/src/search/queries.rs (task 0318) and
--               search/queries/transactions.rs; the hash → ledger step is
--               transactions/queries/hash_lookup.rs (`lookup_hash_ledgers`).
-- Data sources: DB-only.
-- Inputs (positional; Rust binds user text with `?` and inlines trusted
-- integers with `format!` — noted per input):
--   $1  :q                String  the raw user query (UTF-8), bound
--   $2  :q_hex            String  64-char hex of the parsed 32-byte hash
--                                 (hash mode only), bound
--   $3  :strkey_prefix    String  StrKey or prefix (G…/C…), bound
--   $4  :per_group_limit  Int     cap per entity bucket, inlined
--   $5  :code             String  asset code (code_issuer mode), bound
--   $6  :issuer           String  issuer G-StrKey (code_issuer mode), bound
--   $7  :ledger           Int64   candidate ledger from the tx step 1, inlined
--   $8  :partition        Int64   intDiv($7, 500000), inlined
--   The contract-name step and the asset issuer step take the page's ids,
--   inlined by Rust as literal IN lists; the literals below are examples
--   (same convention as 02 / 08).
--
-- CH PATTERN (task 0318): NOT one UNION-of-six like the PG path. The API fires
-- only the buckets the classifier proves can match, CONCURRENTLY, and skips the
-- rest. This is the minimal-rows-scanned shape — the common hash lookup touches
-- two point-seeks and nothing else — and avoids CH evaluating WHERE-gated
-- branches that would scan state tables for a needle that cannot match.
--
--   classifier mode            buckets fired
--   ------------------------   --------------------------------------------
--   hash_bytes (hex / L-key)   transaction, pool (by id)
--   strkey_prefix (G… / C…)    account, contract(prefix), asset, nft,
--                              pool (by code — gated out: a StrKey is longer
--                              than any asset code)
--   code_issuer (CODE:ISSUER)  asset (exact), contract(name), nft, pool (code)
--   plain text                 contract(name), asset, nft, pool (by code)
--
-- Intentional divergence from PG: in hash_bytes mode the small-table substring
-- buckets are skipped — an asset_code (≤12 chars) can never contain a 64-hex /
-- 56-char needle (provably empty), and a contract/NFT *named* after a full tx
-- hash is not a real search intent.
--
-- CRITICAL CH-vs-canonical-SQL CORRECTIONS (verified against schema/init.sql
-- and the live CH read modules):
--   • NFT name lives in `nft_enrichment`, NOT `nfts.name` (vestigial NULL on CH
--     — the live indexer rewrites whole `nfts` rows with metadata NULL; 0231).
--   • Contract name lives in `soroban_contract_metadata`, NOT the dead
--     `soroban_contracts.name` (no writer since task 0297).
--   • Asset issuer StrKey is resolved by a bloom-pruned `accounts WHERE id IN
--     (page ids)` key-seek (`idx_acc_id`), NEVER `LEFT JOIN accounts` — the
--     full-table hash-side build OOMs (CH Code 241, the 0317 trap).
--   • Transaction lookup is a `transaction_hash_prefix_index` seek on the
--     hash's first 8 bytes; every candidate ledger is checked for the full hash.
--   • `nullIf(...)` maps a JOIN miss to NULL (api_reader runs readonly=1 → no
--     `SETTINGS join_use_nulls`).

-- ── transaction bucket (hash_bytes mode), step 1 ────────────────────────────
-- hash → candidate ledgers (immutable mapping, no FINAL; more than one only
-- when two hashes share the 8-byte prefix — task 0580). Step 2 runs per
-- candidate, newest first, until one carries the full hash.
SELECT DISTINCT ledger_sequence FROM transaction_hash_prefix_index
WHERE hash_prefix = reinterpretAsUInt64(substring(unhex($2), 1, 8))
ORDER BY ledger_sequence DESC;

-- @@ split @@
-- ── transaction bucket, step 2 ──────────────────────────────────────────────
-- successful + ledger closed_at (PG `tx_hits` enrichment). Single-row seek:
-- leading-PK `ledger_sequence` (one ledger is one granule) + partition prune.
-- closed_at via a BOUNDED `ledgers WHERE sequence = $7` sub-select (PK point
-- seek) — NOT a plain `INNER JOIN ledgers`, which builds its hash side from the
-- whole ledgers table.
SELECT t.successful AS successful, l.closed_at AS created_at_ms
FROM transactions t
INNER JOIN (SELECT sequence, closed_at FROM ledgers WHERE sequence = $7) l
        ON l.sequence = t.ledger_sequence
WHERE t.ledger_sequence = $7
  AND intDiv(t.ledger_sequence, 500000) = $8
  AND (t.hash = unhex($2) OR t.inner_tx_hash = unhex($2))  -- a fee-bump by its inner hash too
ORDER BY t.application_order
LIMIT 1;
-- → identifier = q_hex, label = '' (PG parity), successful/last_activity_at set.

-- @@ split @@
-- ── pool bucket, by id (hash_bytes mode) ────────────────────────────────────
-- pool_id is the full ORDER BY key → point seek. Codes are version-stable, so
-- `ORDER BY last_updated_ledger DESC LIMIT 1` collapses versions (no FINAL).
-- The label is NOT composed here (task 0374): the row returns the raw legs and
-- the API names them with the resolver the pool endpoints use.
SELECT lower(hex(pool_id)) AS pool_hex,
       toInt16(pool_kind)  AS pool_kind,
       legs
FROM liquidity_pools
WHERE pool_id = unhex($2)
ORDER BY last_updated_ledger DESC
LIMIT 1;
-- → identifier = pool_id_hex_to_strkey(pool_hex, pool_kind): an `L…` SEP-23
--   strkey for a classic pool, a `C…` contract address for a Soroban one.

-- @@ split @@
-- ── pool bucket, by asset code (every non-hash mode) ────────────────────────
-- Same rule as the pools list (`common::pool_asset_codes`, task 0470): the
-- needle is trimmed, upper-cased and split on '/' into one or two codes; one
-- code → some leg DISPLAYS a code containing it (native as `XLM`). Shown here
-- for one code (`asset_codes_predicate`); two codes AND two such tests and
-- require two distinct legs. Skipped when any code is longer than 12 chars.
-- Versions collapse via argMax/GROUP BY, not FINAL (task 0420: FINAL here
-- merged the whole table for a search box).
SELECT pool_hex, pool_kind, legs
FROM (
    SELECT lower(hex(pool_id)) AS pool_hex,
           toInt16(argMax(pool_kind, last_updated_ledger)) AS pool_kind,
           argMax(legs, last_updated_ledger) AS legs,
           max(last_updated_ledger) AS newest
    FROM liquidity_pools
    GROUP BY pool_id
) AS lp
WHERE arrayExists(x -> x IN (SELECT id FROM assets
                             WHERE position(lower(if(asset_type = 0, 'XLM', toString(asset_code))),
                                            lower(trim($1))) > 0),
                  lp.legs)
ORDER BY newest DESC
LIMIT $4;

-- @@ split @@
-- ── account bucket (strkey_prefix mode) ─────────────────────────────────────
-- account_id IS the ORDER BY key → `startsWith() ORDER BY account_id LIMIT` is
-- an EARLY-TERMINATING PK range read (~constant few granules regardless of
-- prefix breadth). NO FINAL: a FINAL merge over the prefix range scales with
-- breadth. Re-ingest dupes of one account_id are contiguous → collapsed in
-- Rust. A C… prefix matches nothing (cheap empty range).
SELECT account_id AS account_id, ifNull(home_domain, '') AS label
FROM accounts
WHERE startsWith(account_id, $3)
ORDER BY account_id
LIMIT $4;

-- @@ split @@
-- ── contract bucket — prefix sub-mode (strkey_prefix mode), step 1 ──────────
-- Early-terminating PK range read for the matching ids (NO FINAL, same
-- reasoning as accounts; contract_id is the immutable identity). Deduped in
-- Rust.
SELECT contract_id AS contract_id
FROM soroban_contracts
WHERE startsWith(contract_id, $3)
ORDER BY contract_id
LIMIT $4;

-- @@ split @@
-- ── contract bucket — prefix sub-mode, step 2 ───────────────────────────────
-- Bounded name resolution for the ≤limit matched ids — contract_id is the
-- metadata PK, so IN(...) is a key-seek (NOT a whole-metadata aggregation).
-- `ifNull(argMax(...), '')` keeps the column non-nullable (name is
-- Nullable(String)) so it decodes into a non-Option Rust field.
SELECT contract_id AS contract_id, ifNull(argMax(name, version), '') AS name
FROM soroban_contract_metadata
WHERE contract_id IN ('CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA')
GROUP BY contract_id;

-- @@ split @@
-- ── contract bucket — name sub-mode (every non-hash, non-prefix mode) ────────
-- Substring over on-chain metadata name (Soroban-native tokens; SACs excluded
-- from that table by design). Subquery so argMax is computed once.
SELECT contract_id AS contract_id, ifNull(name, '') AS name FROM (
    SELECT contract_id, argMax(name, version) AS name
    FROM soroban_contract_metadata GROUP BY contract_id
)
WHERE positionCaseInsensitive(ifNull(name, ''), $1) > 0
LIMIT $4;

-- @@ split @@
-- ── asset bucket, code_issuer mode (CODE:ISSUER / CODE-ISSUER), step 1 ──────
-- A fully-qualified asset names exactly one row, so this arm is an equality
-- lookup with NO ranking and no holder join (task 0534). The classifier only
-- sets this mode when the issuer decodes as a CRC-valid G-StrKey, so a typo
-- falls back to the substring arm below. `accounts` is keyed ORDER BY
-- account_id, making the subquery a point seek, never the ~23M-row hash join
-- that OOMs (Code 241). `soroban_contracts` is deduped per id before the join
-- (it is version-full).
SELECT a.asset_type AS asset_type,
       nullIf(a.asset_code, '')   AS asset_code,
       nullIf(sc.contract_id, '') AS contract_strkey,
       a.issuer_id AS issuer_id
FROM assets a FINAL
LEFT JOIN (
    SELECT id, any(contract_id) AS contract_id
    FROM soroban_contracts GROUP BY id
) sc ON sc.id = a.contract_id
WHERE lower(toString(a.asset_code)) = lower($5)
  AND a.issuer_id IN (SELECT id FROM accounts WHERE account_id = $6)
LIMIT $4;

-- @@ split @@
-- ── asset bucket, substring arm (strkey_prefix OR plain-text mode), step 1 ──
-- RANKED (task 0485): exact code > prefix > anywhere — the order follows from
-- what MATCHED — with holder count breaking ties inside a tier (441 assets
-- carry the code `USDC`; the real one has ~691k holders vs ~3k for the
-- runner-up), and the trailing key columns making the order total.
--
-- Both the match and the tier compare the DISPLAYED code: native XLM stores an
-- empty code and renders as `XLM`. The same expression appears in
-- 08_get_assets_list.sql and in the pools predicate — three literal copies,
-- each with a test that fails if it drifts.
--
-- No synonyms: `native` matches the assets whose code contains NATIVE, and
-- nothing else. Native XLM answers to `xlm`, which is what it displays as.
SELECT a.asset_type AS asset_type,
       nullIf(a.asset_code, '')   AS asset_code,
       nullIf(sc.contract_id, '') AS contract_strkey,
       a.issuer_id AS issuer_id
FROM assets a FINAL
LEFT JOIN (
    SELECT id, any(contract_id) AS contract_id
    FROM soroban_contracts GROUP BY id
) sc ON sc.id = a.contract_id
LEFT JOIN balance_aggregates bagg ON bagg.asset_id = a.id
WHERE position(lower(if(a.asset_type = 0, 'XLM', toString(a.asset_code))), lower($1)) > 0
ORDER BY multiIf(lower(if(a.asset_type = 0, 'XLM', toString(a.asset_code))) = lower($1), 0,
                 startsWith(lower(if(a.asset_type = 0, 'XLM', toString(a.asset_code))),
                            lower($1)), 1,
                 2) ASC,
         bagg.holder_count DESC NULLS LAST,
         a.asset_type ASC, a.asset_code ASC, a.issuer_id ASC
LIMIT $4;

-- @@ split @@
-- ── asset bucket, step 2 (both arms) ────────────────────────────────────────
-- Resolve the page's issuer surrogates → G-StrKey (bloom-pruned seek). The
-- account_id is identical across versions, so no FINAL; `LIMIT 1 BY id`
-- collapses re-ingest parts.
SELECT id AS id, account_id AS account_id
FROM accounts WHERE id IN (987654321) LIMIT 1 BY id;
-- → identifier = COALESCE(asset_code, 'XLM'); label = asset_family_name;
--   route_token = contract StrKey | CODE-ISSUER | native (composed in Rust).

-- @@ split @@
-- ── nft bucket (every non-hash mode) ────────────────────────────────────────
-- Name + collection come from nft_enrichment (argMax); contract surrogate →
-- C-StrKey via a page-scoped soroban_contracts lookup; the label prefers the
-- contract's on-chain metadata name over the enrichment collection name.
-- Routes on the (contract_id, token_id) composite (ADR 0030).
WITH
enr AS (
    SELECT contract_id, token_id,
           argMax(name, version)            AS name,
           argMax(collection_name, version) AS collection_name
    FROM nft_enrichment GROUP BY contract_id, token_id
),
page AS (
    SELECT n.contract_id     AS contract_surrogate,
           n.token_id        AS token_id,
           e.name            AS e_name,
           e.collection_name AS e_collection_name
    FROM nfts n FINAL
    LEFT JOIN enr e ON e.contract_id = n.contract_id AND e.token_id = n.token_id
    WHERE positionCaseInsensitive(ifNull(e.name, ''), $1) > 0
    LIMIT $4
),
sc AS (
    SELECT id, any(contract_id) AS contract_id
    FROM soroban_contracts
    WHERE id IN (SELECT contract_surrogate FROM page) GROUP BY id
),
scm AS (
    SELECT contract_id, argMax(name, version) AS name
    FROM soroban_contract_metadata
    WHERE contract_id IN (SELECT contract_id FROM sc)
    GROUP BY contract_id
)
SELECT ifNull(p.e_name, '')                                                         AS identifier,
       ifNull(coalesce(nullIf(scm.name, ''), nullIf(p.e_collection_name, '')), '') AS label,
       sc.contract_id                                                               AS contract_strkey,
       p.token_id                                                                   AS token_id
FROM page p
INNER JOIN sc ON sc.id = p.contract_surrogate
LEFT JOIN scm ON scm.contract_id = sc.contract_id;
