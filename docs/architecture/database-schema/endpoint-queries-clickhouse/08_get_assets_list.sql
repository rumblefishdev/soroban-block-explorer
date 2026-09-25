-- Endpoint:     GET /assets
-- Purpose:      Paginated list of assets across native, classic credit and
--               soroban-native (a SAC is a FACET of its classic/native row,
--               ADR 0051 — no separate row). Ordered by active holders, most
--               held first. Optional filters: type, asset_code (substring
--               against the DISPLAYED code, plus on-chain name / symbol), SAC
--               deployed only.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.8
-- Impl:         crates/api/src/assets/queries.rs — `build_list_seek_sql`,
--               `hydrate_sql`, `resolve_soroban_contracts`,
--               `resolve_page_issuers` (task 0364 two-phase read).
-- Schema:       assets, balance_aggregates (task 0331), asset_enrichment,
--               asset_sac, soroban_contracts, soroban_contract_metadata,
--               accounts; ADR 0044.
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                 Int     page size (the handler's +1 peek row);
--                                      the seek over-fetches `$1 * 8`
--                                      (SEEK_OVERFETCH) raw versions
--   $2  :asset_type_filter     Int16   NULL = no filter
--   $3  :asset_code_filter     String  NULL = no filter (substring)
--   $4  :cursor_holder_rank    Int32   NULL on first page
--   $5  :cursor_id             Int64   NULL on first page
--   $6  :sac_only              UInt8   0 = no filter, 1 = deployed SAC only
--   Statements B–D take the page's keys, inlined by Rust as literal IN lists;
--   the literals below are examples (same convention as 02).
-- Indexes:      assets ORDER BY (asset_type, asset_code, issuer_id,
--                 contract_id) — the hydration seek is a PK IN-list.
--               balance_aggregates keyed by asset_id (the `assets.id`
--                 surrogate, cityhash64 of the 4-tuple).
--               soroban_contracts `idx_sc_id` bloom; accounts `idx_acc_id`
--                 bloom — both seeked by surrogate id, never hash-joined.
-- CH Engine:    assets — ReplacingMergeTree, read WITHOUT FINAL: the seek
--                 over-fetches and Rust consecutive-dedups by the 4-tuple;
--                 hydration collapses versions with `LIMIT 1 BY` (every
--                 projected `assets` column is immutable across versions).
--               balance_aggregates — refreshable-MV target (no FINAL).
--               asset_enrichment — Replacing(version), argMax per key.
--               asset_sac — max() per key (multi-row facet).
--               soroban_contract_metadata — FINAL (small).
--               accounts — latest version by `ORDER BY last_seen_ledger DESC
--                 LIMIT 1 BY id` (home_domain is mutable).
-- CH Pattern:   A (seek) → Rust dedup to `$1` keys → B (hydrate) ∥ D (issuers),
--               C (contract context) overlapping B. Keyset cursor
--               `(holder_rank, id)`, DESC.
-- ADR 0044 §:   §4.5 (Replacing state).
-- Notes:
--   • Browse order is ACTIVE HOLDERS, highest first (task 0547), with
--     `assets.id` as the tiebreak that makes it total. LEFT join + the `-1`
--     fold keep assets with no aggregate row in the list, below a measured
--     zero. Ordering on a joined column reads all of `assets` (measured 61 ms
--     / 1.02 M rows per page on production); if that ever binds, the cheaper
--     shape is a driver over `balance_aggregates` itself (measured 11 ms).
--   • The cursor carries the rank A ordered by, not the `holder_count` B
--     re-reads: `balance_aggregates` is rebuilt every few minutes and a
--     rebuild between the two reads would move the cursor off the boundary.
--   • `total_supply` / `holder_count` come from `balance_aggregates` (task
--     0331), keyed by `assets.id` — ONE aggregate row per asset, a SAC's
--     contract-held balances folded into its classic row. The retired
--     `asset_aggregates` (keyed `(asset_code, issuer_id)`) is gone.
--   • The asset_code filter matches the DISPLAYED code: native XLM stores an
--     EMPTY code and renders as `XLM` (task 0485). The same expression is in
--     22_get_search.sql and the pools predicate
--     (`common::asset_identity::shown_code_sql`).
--   • Rust joins `sc` / `m` into A ONLY when `$3` is set, and adds the SAC
--     semi-join ONLY when `$6` = 1; the unfiltered page is a bare `assets`
--     walk. Shown here always present, gated by the NULL / 0 tests.
--   • NO relevance ranking on this surface, by decision (task 0485): a tier
--     order would have to ride in the cursor. Relevance lives in 22.
--   • name / symbol / decimals / contract StrKey / deploy ledger are
--     assembled in Rust (`assemble_asset_row`): name = asset_enrichment.name
--     → soroban_contract_metadata.name → 'Stellar Lumens' for native;
--     decimals = metadata.decimals, else 7.

-- ============================================================================
-- A. Phase-1 seek — the page's identity keys + the rank they were ordered by.
-- ============================================================================
SELECT a.asset_type AS asset_type, a.asset_code AS asset_code,
       a.issuer_id AS issuer_id, a.contract_id AS contract_id, a.id AS id,
       coalesce(ba.holder_count, -1) AS holder_rank
FROM assets a
LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id           -- only when $3 is set
LEFT JOIN (SELECT contract_id, name, symbol FROM soroban_contract_metadata FINAL) m
       ON m.contract_id = sc.contract_id                          -- only when $3 is set
LEFT JOIN balance_aggregates ba ON ba.asset_id = a.id
WHERE 1
  AND ($2 IS NULL OR a.asset_type = $2)
  AND ($6 = 0 OR (a.asset_type, a.asset_code, a.issuer_id, a.contract_id) IN (
          SELECT asset_type, asset_code, issuer_id, contract_id FROM asset_sac
          GROUP BY asset_type, asset_code, issuer_id, contract_id
          HAVING toBool(max(sac_deployed))))
  AND ($3 IS NULL OR position(lower(if(a.asset_type = 0, 'XLM', toString(a.asset_code))), lower($3)) > 0
                  OR positionCaseInsensitive(coalesce(m.name, ''), $3) > 0
                  OR positionCaseInsensitive(coalesce(m.symbol, ''), $3) > 0)
  AND ($4 IS NULL OR (coalesce(ba.holder_count, -1), a.id) < ($4, $5))
ORDER BY holder_rank DESC, a.id DESC
LIMIT $1 * 8;

-- @@ split @@
-- ============================================================================
-- B. Phase-2 hydration (`hydrate_sql`) — the deduped page keys, every side
--    table bounded to them. Tuples and ids are inlined literals so CH uses the
--    PK index (a subquery IN would full-scan). Shared verbatim with 09.
-- ============================================================================
SELECT
    a.asset_type                AS asset_type,
    nullIf(a.asset_code, '')    AS asset_code,
    nullIf(ae.name, '')         AS name_enrichment,
    toString(bagg.total_supply) AS total_supply,
    bagg.holder_count           AS holder_count,
    nullIf(ae.icon_url, '')     AS icon_url,
    a.issuer_id                 AS issuer_id_key,
    a.contract_id               AS contract_id_key,
    sac.sac_contract_id         AS sac_contract_surrogate,
    sac.sac_deployed            AS sac_deployed,
    a.id                        AS id
FROM assets a
LEFT JOIN (
    SELECT asset_id, total_supply, holder_count
    FROM balance_aggregates WHERE asset_id IN (1234567890123, 2345678901234)
) bagg ON bagg.asset_id = a.id
LEFT JOIN (
    SELECT asset_type, asset_code, issuer_id, contract_id,
           argMax(icon_url, version) AS icon_url,
           argMax(name, version)     AS name
    FROM asset_enrichment
    WHERE (asset_type, asset_code, issuer_id, contract_id) IN ((0,'',0,0),(1,'USDC',987654321,0))
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) ae ON ae.asset_type  = a.asset_type  AND ae.asset_code  = a.asset_code
    AND ae.issuer_id   = a.issuer_id   AND ae.contract_id = a.contract_id
LEFT JOIN (
    SELECT asset_type, asset_code, issuer_id, contract_id,
           max(sac_contract_id)        AS sac_contract_id,
           toBool(max(sac_deployed))   AS sac_deployed
    FROM asset_sac
    WHERE (asset_type, asset_code, issuer_id, contract_id) IN ((0,'',0,0),(1,'USDC',987654321,0))
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) sac ON sac.asset_type  = a.asset_type  AND sac.asset_code  = a.asset_code
    AND sac.issuer_id   = a.issuer_id   AND sac.contract_id = a.contract_id
WHERE (a.asset_type, a.asset_code, a.issuer_id, a.contract_id) IN ((0,'',0,0),(1,'USDC',987654321,0))
LIMIT 1 BY a.asset_type, a.asset_code, a.issuer_id, a.contract_id;

-- @@ split @@
-- ============================================================================
-- C. `soroban_contracts` context (`resolve_soroban_contracts`) — one bloom
--    seek by surrogate id for the page's non-zero `contract_id`s: StrKey,
--    deploy ledger and on-chain metadata. Skipped when the page has none.
--    `argMax(_, wasm_uploaded_at_ledger)` skips the low-version NULL stubs.
-- ============================================================================
SELECT sc.id AS id,
       argMax(sc.contract_id, sc.wasm_uploaded_at_ledger)        AS contract_id,
       argMax(sc.deployed_at_ledger, sc.wasm_uploaded_at_ledger) AS deployed_at_ledger,
       any(m.name)     AS name,
       any(m.symbol)   AS symbol,
       any(m.decimals) AS decimals
FROM soroban_contracts sc
LEFT JOIN (
    SELECT contract_id, name, symbol, decimals
    FROM soroban_contract_metadata FINAL
) m ON m.contract_id = sc.contract_id
WHERE sc.id IN (3456789012345)
GROUP BY sc.id;

-- @@ split @@
-- ============================================================================
-- D. Page issuers (`resolve_page_issuers`) — surrogate → G-StrKey +
--    home_domain by the `idx_acc_id` bloom seek, latest version per id. Runs
--    off A's keys, concurrently with B. `id = 0` (native / no issuer) is
--    excluded by the caller; skipped when the page has none.
-- ============================================================================
SELECT id AS id, account_id AS account_id, home_domain AS home_domain
FROM accounts WHERE id IN (987654321)
ORDER BY last_seen_ledger DESC LIMIT 1 BY id;
