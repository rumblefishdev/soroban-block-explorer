-- ClickHouse production schema (task 0206 + 0208 + ADR 0044 amendments).
--
-- ## Design — hybrid: surrogate Int64 for high-cardinality join hubs,
-- natural keys everywhere else
--
-- Three tables get surrogate `id Int64` columns (deterministic
-- `cityhash64(natural_key)` derivation in
-- `crates/db-clickhouse/src/persist/ids.rs`):
--
--   - `accounts.id`            ← cityhash64(account_id StrKey)
--   - `soroban_contracts.id`   ← cityhash64(contract_id StrKey)
--   - `transactions.id`        ← cityhash64(hash bytes)
--
-- These three are the **central FK hubs** — referenced by 6–8
-- downstream tables each. Tens of millions of unique values at full
-- mainnet scale. Empirical measurement (10k-ledger smoke):
-- plain-String / LowCardinality FK columns added ~500 MB on disk vs
-- a hash-i64 baseline, projected ~550 GB on the full 11M-ledger
-- backfill. Int64 FK columns ~4× smaller pre-compression and
-- significantly cheaper on JOIN / GROUP BY because CH compares
-- integers in a single CPU op vs variable-length string memcmp.
--
-- Other tables (`assets`, `nfts`, `liquidity_pools`,
-- `liquidity_pool_snapshots`, `operations_appearances`,
-- `transaction_participants`, `nft_ownership`, `lp_positions`,
-- `account_balances_current`, `wasm_interface_metadata`,
-- `ledgers`, `transaction_hash_index`) keep their natural / composite
-- primary keys — no surrogate `id`. Composite (StrKey-or-hash, …)
-- ORDER BYs work cheaply for these without a hash layer.
--
-- ## Why deterministic `cityhash64(natural_key)` for the surrogate IDs
--
-- - **Replay-idempotency** — `ReplacingMergeTree` dedups by
--   `ORDER BY`; replays must produce the same `id` for the same
--   natural key so the merger collapses duplicates from a re-run.
-- - **Parallel-writer safety** — K runners on disjoint partition
--   ranges all compute the same `id` for the same StrKey without
--   coordination.
-- - **Cross-table FK consistency** — every `source_id` /
--   `account_id` / `deployer_id` / etc. across the schema is
--   `cityhash64(strkey)` of the referenced account; every
--   `contract_id` FK column is `cityhash64(strkey)` of the
--   referenced contract; every `transaction_id` FK column is
--   `cityhash64(tx_hash_bytes)`.
--
-- Hash algorithm: `cityhash-rs::cityhash_102_128` (CityHash v1.0.2
-- 128-bit) lower 64 bits. **Not bit-equivalent to CH SQL
-- `cityHash64()`** (CH builtin is the 64-bit variant of CityHash
-- 1.0.2 — a different algorithm from the lower-half of the
-- 128-bit). Future CH-side queries that want to recompute an `id`
-- must call the writer's helper or add a UDF.
--
-- ## Other production design choices
--
-- - **`ReplacingMergeTree` for every state-shaped table.** Including
--   `liquidity_pools` (was `MergeTree` in pilot, ~20× duplication
--   per task 0208 observation; now folded inline). Background merger
--   collapses by `ORDER BY` key.
-- - **`LowCardinality(String)`** on bounded-cardinality columns:
--   asset codes, event signatures, home_domain.
-- - **`ZSTD(3)` codecs** on JSON-ish columns: `soroban_events.topics_xdr`,
--   `soroban_events.data_xdr`, `wasm_interface_metadata.metadata`.
-- - **Empty-string sentinel** for composite-PK "no value" slots
--   (`assets.asset_code = ''` for native, etc.). CH `ORDER BY` on
--   plain `String` is significantly faster than `Nullable(String)`.
-- - **`account_balances_current` trustline removals → `balance = 0`
--   rows**; reads filter `WHERE balance > 0`. No tombstone engine.
--
-- ## Engine assignment
--
--   - append-only fact tables  → ReplacingMergeTree (dedup by ORDER BY)
--   - state tables             → ReplacingMergeTree(version_column) where
--                                a natural NOT NULL ledger column exists
--                                (otherwise plain ReplacingMergeTree)
--   - immutable lookup tables  → ReplacingMergeTree (collapse re-run /
--                                parallel-merge duplicates; lore-0293)
--
-- Partitioning: every fact table uses `intDiv(ledger_sequence, 500000)`
-- (~29 days at 5 s/ledger). State and immutable tables not partitioned.
--
-- All statements are idempotent (`CREATE TABLE IF NOT EXISTS`,
-- `CREATE DICTIONARY IF NOT EXISTS`); applying twice is a no-op.

----------------------------------------------------------------------
-- Immutable lookups (ReplacingMergeTree — collapse re-run / parallel-merge
-- duplicates; were plain MergeTree until lore-0293)
----------------------------------------------------------------------

-- ReplacingMergeTree (was plain MergeTree): the commit marker. A normal
-- single-machine crash-resume never re-writes a marked ledger (resume keys on
-- the ABSENT marker), but parallel-backfill merges / range overlap / manual
-- re-index DO produce duplicate `sequence` rows that plain MergeTree never
-- collapses — they double `ledgers` JOINs and needed a manual
-- `OPTIMIZE … DEDUPLICATE BY sequence` (task 0228). RMT collapses them
-- automatically on merge. Content is immutable per `sequence`, so no version
-- column; cost is merge-time only and ~0 on a unique monotonic key; reads stay
-- FINAL-free. (lore-0293)
CREATE TABLE IF NOT EXISTS ledgers (
    sequence          Int64,
    hash              FixedString(32),
    closed_at         DateTime64(3, 'UTC'),
    protocol_version  Int32,
    transaction_count Int32,
    base_fee          Int64,
    -- `closed_at` is not the sort key (`sequence` is), so a time-bounded read —
    -- the LP chart resolving a window's ledger range — scanned the whole ~26M-row
    -- table. `closed_at` is monotonic with `sequence`, so a minmax skip index
    -- prunes to the matching granule range for a few KB (minmax stores 2 values
    -- per granule group). Measured on the 2026-07-17 50M req/month load test:
    -- lpchart 77.9M -> 26.3M read_rows/request (-27.5bn over a 12-min run), CH
    -- time 411 -> 106 ms. Applied to the live prod table via
    -- ALTER ... ADD INDEX + MATERIALIZE INDEX (online, 2026-07-17); GRANULARITY 4
    -- matches what is deployed — keep them in sync.
    INDEX closed_at_mm closed_at TYPE minmax GRANULARITY 4
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(sequence, 500000)
ORDER BY (sequence);

-- ReplacingMergeTree (was plain MergeTree): written in the entity phase
-- (before the `ledgers` commit marker), so a crash-resume / backfill re-run
-- re-emits the same `(wasm_hash, metadata)` row. Plain MergeTree never dedups
-- → permanent byte-identical duplicates that double `contracts/interface`
-- JOINs and needed a manual `OPTIMIZE … DEDUPLICATE BY wasm_hash` (task 0228).
-- Content is immutable per `wasm_hash`, so no version column — any duplicate is
-- byte-identical and RMT collapses it on merge; reads stay FINAL-free. (lore-0293)
CREATE TABLE IF NOT EXISTS wasm_interface_metadata (
    wasm_hash FixedString(32),
    metadata  String CODEC(ZSTD(3))
)
ENGINE = ReplacingMergeTree
ORDER BY (wasm_hash);

----------------------------------------------------------------------
-- State tables — surrogate-id hubs (accounts, soroban_contracts)
----------------------------------------------------------------------

-- accounts: surrogate `id Int64` for cheap FK joins from 8+ tables.
-- ORDER BY natural key (`account_id`) — direct `WHERE account_id = 'GDMOSA…'`
-- granule-prunes cheaply, FK joins use `id` via hash-join (sort order
-- doesn't matter for hash join). Best of both worlds.
CREATE TABLE IF NOT EXISTS accounts (
    id                Int64,
    account_id        String,
    first_seen_ledger Int64,
    last_seen_ledger  Int64,
    sequence_number   Int64,
    -- home_domain: MUTABLE (SET_OPTIONS rewrites it), but rare — 4 of the
    -- 1.01M prod accounts that carry one have more than one value (measured
    -- 2026-07-22, task 0397). Any read projecting it MUST take the latest
    -- version (`ORDER BY last_seen_ledger DESC LIMIT 1`); the bare
    -- `LIMIT 1 BY id` used where only `account_id` is projected is NOT safe
    -- here. Very low cardinality globally (mainnet has a handful of unique
    -- SEP-1 issuers' domains across tens of millions of accounts; the vast
    -- majority are NULL). LowCardinality dictionary-encodes the few unique
    -- values per block — strong compression on top of default LZ4.
    home_domain       LowCardinality(Nullable(String)),
    -- The table is ORDER BY account_id (StrKey -> id resolves on the PK), but
    -- tx-list endpoints need the REVERSE: id (surrogate) -> account_id, to
    -- project `source_account`. `id` is not the sort key, so that lookup
    -- full-scans accounts (~23M rows, task 0290 — the ~35M/poll the polled
    -- /transactions list read came from this join, NOT the partition scan).
    -- A bloom_filter on `id` lets `WHERE id IN (page source_ids)` prune to a
    -- handful of granules. FP 0.001 (tighter than the 0.025 default) because
    -- the lookup tests N keys at once and per-key false positives compound
    -- (1-(1-p)^N): at default 0.025 / 11 keys ~6M rows survived; at 0.001 ~1M.
    -- Index is ~tens of MB. Applied to the live prod table via
    -- ALTER ... ADD INDEX + MATERIALIZE INDEX (online, 2026-06-16).
    -- CONSUMERS (check before ever dropping this as "unused"): the tx-list /
    -- search / assets id->StrKey seeks in `crates/api`, AND the SEP-1
    -- enrichment worker's issuer resolve (`enrich_and_persist::sep1_assets`,
    -- task 0397) — that one is a single-key seek and reads 3 granules with the
    -- index vs the whole table without it, silently, with no error to notice.
    INDEX idx_acc_id id TYPE bloom_filter(0.001) GRANULARITY 1
)
ENGINE = ReplacingMergeTree(last_seen_ledger)
ORDER BY (account_id);

-- accounts_recent (task 0385): a `last_seen_ledger`-ordered copy of `accounts`
-- powering the acclist browse (`GET /v1/accounts`, sole sort = last_seen). The
-- base table is ORDER BY account_id (identity — REQUIRED for RMT dedup, since
-- last_seen_ledger MUTATES and cannot live in the sort key), so the list's
-- last_seen sort is a whole-dimension `accounts FINAL` scan+sort (~24M rows). A
-- projection on the RMT is refused by CH 26.3 (task 0353, Code 344), so the
-- alternate ordering lives in this separate plain-MergeTree table, filled by a
-- refreshable MV (full recompute + atomic EXCHANGE → reads need no FINAL; mirrors
-- `balance_aggregates_mv`). `accounts::fetch_list` then read-in-order SEEKs it.
CREATE TABLE IF NOT EXISTS accounts_recent (
    id                Int64,
    account_id        String,
    last_seen_ledger  Int64,
    first_seen_ledger Int64,
    home_domain       LowCardinality(Nullable(String))
)
ENGINE = MergeTree
ORDER BY (last_seen_ledger, id);

-- Refreshable MV that recomputes `accounts_recent` from `accounts` (defined above
-- — the source table MUST exist before this CREATE). Full recompute + atomic
-- EXCHANGE, so reads need no FINAL. Refresh interval = acclist freshness: a
-- ≤interval-stale "recently active accounts" browse is fine, and it is a shared
-- server-side origin (strictly better than the old per-client FE 60s cache).
CREATE MATERIALIZED VIEW IF NOT EXISTS accounts_recent_mv
REFRESH EVERY 2 MINUTE
TO accounts_recent AS
SELECT id, account_id, last_seen_ledger, first_seen_ledger, home_domain
FROM accounts FINAL;

-- soroban_contracts: same hybrid pattern as accounts.
-- `wasm_uploaded_at_ledger` is the version slot; `DEFAULT 0` is the
-- stub-row sentinel (Pass 2 stub-rowing for referenced-but-not-deployed
-- contracts in mid-stream backfill ranges).
-- NAMING TRAP (task 0398) — `contract_id` means two different things:
--   * HERE (and in `soroban_contract_metadata`) it is a `String`: the real
--     `C…` StrKey.
--   * EVERYWHERE ELSE (`assets`, `nfts`, `nft_ownership`, `soroban_events`,
--     `operations_appearances`, …) it is an `Int64`: the cityhash64 surrogate
--     OF that StrKey, i.e. the value stored in `soroban_contracts.id`.
-- So a foreign key named `contract_id` joins `soroban_contracts.id`, NEVER
-- `soroban_contracts.contract_id`. Same value, three column names, two types
-- (`sac_contract_id`, `caller_contract_id` are the same surrogate too).
-- Deliberate one shared surrogate space (`ids::{account,contract,address}_id`
-- are byte-identical) — not redundancy. Renaming was costed and deferred to
-- task 0418: the ALTER is metadata-only, but the call sites are 85 in
-- `stage.rs` + 21 in `crates/api`.
CREATE TABLE IF NOT EXISTS soroban_contracts (
    id                       Int64,
    contract_id              String,
    wasm_hash                Nullable(FixedString(32)),
    wasm_uploaded_at_ledger  Int64 DEFAULT 0,
    deployer_id              Nullable(Int64),
    deployed_at_ledger       Nullable(Int64),
    contract_type            Nullable(Int16),
    is_sac                   Bool,
    -- `name` DROPPED (task 0304): dead since 0297 (no writer, reader-less,
    -- 0/148663 populated in prod). Prod `ALTER … DROP COLUMN name` pending.
    -- 0344: tx-detail resolves surrogate `id` -> `contract_id`, but `id` is not
    -- the sort key; mirror accounts' `idx_acc_id` so `WHERE id IN (…)` is a
    -- granule seek instead of a full-table `JOIN soroban_contracts FINAL`.
    INDEX idx_sc_id id TYPE bloom_filter(0.001) GRANULARITY 1
)
ENGINE = ReplacingMergeTree(wasm_uploaded_at_ledger)
ORDER BY (contract_id);

-- On-chain Soroban token metadata (name/symbol/decimals) read from the
-- contract's instance-storage `Symbol("METADATA")` struct. Per-contract,
-- INDEXER-derived (NOT the off-chain enrichment family). A SEPARATE table, not
-- columns on `soroban_contracts`, because: (1) RMT whole-row replace +
-- soroban_contracts' many writers (deploy / contract_type_rebuild EXCHANGE /
-- stub INSERTs / db-merge) would clobber in-row metadata to NULL (the G5 bug
-- class); (2) deploy identity (wasm_hash/deployer, from the deploy tx, NOT in
-- the instance entry) and metadata live on DIFFERENT update clocks, which one
-- RMT version column cannot track. Written by the parser on contract-instance
-- `created` / `updated` / `restored` changes; SACs skipped (name=CODE:ISSUER /
-- symbol=code / decimals=7 already derivable from SAC identity). `version` =
-- observed ledger (deterministic/replay-safe; latest wins). `decimals` is
-- rendered as 7 at read for classic/SAC.
-- INVARIANT: every row is a WHOLE-struct snapshot at one ledger (name+symbol+
-- decimals all set from the same METADATA at that version) — never a partial
-- single-column write. Read with `FINAL` (latest whole row per contract_id) —
-- the direct, frankenstein-proof RMT collapse for a whole-row read; the table is
-- bounded (Soroban-native tokens only) so the read-time merge is cheap. Read
-- (assets list): `assets … LEFT JOIN (SELECT contract_id, name, symbol, decimals
-- FROM soroban_contract_metadata FINAL) m ON m.contract_id = sc.contract_id`,
-- `COALESCE(ae.name, m.name, sc.name, …)`. The contract detail endpoint does NOT
-- read this table. Full reasoning: task 0297.
CREATE TABLE IF NOT EXISTS soroban_contract_metadata (
    contract_id  String,
    name         Nullable(String),
    symbol       Nullable(String),
    decimals     Nullable(UInt32),
    version      Int64
)
ENGINE = ReplacingMergeTree(version)
ORDER BY (contract_id);

----------------------------------------------------------------------
-- State tables — composite natural keys (no surrogate id)
----------------------------------------------------------------------

-- assets identity is a 4-tuple. Native XLM: all-empty + asset_type=0.
-- Classic credit: code+issuer set, contract_id=0. SAC: contract_id
-- set, code/issuer optional. Soroban-native: contract_id set,
-- code/issuer=empty/0.
--
-- DEAD columns (`ALTER … DROP COLUMN` batched in the cleanup task 0310):
--  * `total_supply` / `holder_count` (lore-0293) — nothing reads them (the API
--    serves the aggregate from `balance_aggregates`); the indexer writes NULL.
--    A global rollup written into this per-ledger-rewritten row clobbered them
--    (no-version RMT, last-write-wins → ~25% of classic served NULL in prod).
--  * `name` / `icon_url` — the indexer writes them NULL (parser never sets an
--    asset name; verified 0/367321 rows populated in prod). Every read resolves
--    the display name/icon from `asset_enrichment` (curated) coalesced over
--    `soroban_contract_metadata` (on-chain) — never from `assets` — so these two
--    are vestigial too.
CREATE TABLE IF NOT EXISTS assets (
    asset_type      Int16,
    asset_code      LowCardinality(String),
    issuer_id       Int64,            -- 0 for native / soroban-native
    contract_id     Int64,            -- 0 for native / classic-credit
    -- `name` DROPPED (task 0304): dead since 0297 (reader-less, 0/336053 prod).
    -- Prod `ALTER … DROP COLUMN name` batches with 0310's assets deploy-drain.
    total_supply    Nullable(Decimal128(7)),  -- DEAD (lore-0293) → balance_aggregates
    holder_count    Nullable(Int32),          -- DEAD (lore-0293) → balance_aggregates
    icon_url        Nullable(String),         -- DEAD → asset_enrichment.icon_url
    -- lore-0331 (Option C): single surrogate = ids::asset_id (cityhash64 of the
    -- canonical identity; classic="CODE:ISSUER"; SAC + soroban keyed by their own
    -- contract, so each is a DISTINCT asset id). The first single-column asset key — `balances.asset_id`
    -- references it. NOT in ORDER BY (natural key unchanged; additive, non-breaking).
    -- PROD: existing table needs `ALTER TABLE assets ADD COLUMN id Int64` + a
    -- one-time backfill of `id` for existing rows (maintenance window) — CREATE IF
    -- NOT EXISTS won't add it. Default 0 until a row is rewritten/backfilled.
    id              Int64 DEFAULT 0
)
ENGINE = ReplacingMergeTree
ORDER BY (asset_type, asset_code, issuer_id, contract_id);

-- SAC facet side table (ADR 0051 / task 0339). One logical row per SAC-having
-- classic_credit / native asset, keyed byte-for-byte like `assets` and joined at
-- read (`… GROUP BY key` with `max()`). Written by the INDEXER (not the enricher)
-- ONLY when a SAC is sighted — a deploy (`sac_deployed=1`) or an un-deployed
-- override event (`sac_deployed=0`) — NEVER on a plain trustline re-emit, so the
-- per-ledger whole-row `assets` rewrite can never zero it.
--   * `sac_contract_id` — cityhash64 surrogate of the SAC's `C…` StrKey (the same
--     hash used for `soroban_contracts.id`). The `C…` StrKey itself is NOT stored
--     — it re-derives on read from `code:issuer` (`derive_sac_strkey`).
--   * `sac_deployed` — deployed on-chain? MONOTONIC (false→true, never back), so
--     `SimpleAggregateFunction(max)` makes a deploy sighting stick over any later
--     un-deployed-override event; `max()` on the constant surrogate is a no-op.
-- AggregatingMergeTree (not RMT): `max` merges column-wise per key, so an override
-- event AFTER a deploy cannot downgrade `sac_deployed` (a versioned RMT keeps the
-- last-inserted whole row and WOULD downgrade). No `soroban_contracts` join needed
-- for deployed-ness — the flag is stored.
-- No skip-index on `sac_contract_id`: every read aggregates the whole (small)
-- table — `SELECT key, max(sac_contract_id) … GROUP BY key` for the join, then the
-- `/assets/{C…}` deep-link filters `sac.sac_contract_id = ?` on that join result —
-- so a per-column index would prune nothing. `asset_sac` is one row per
-- SAC-having asset (~31k at mainnet scale), so the full-table aggregate is cheap;
-- add a `sac_contract_id` skip-index + a direct point-lookup only if it ever grows.
CREATE TABLE IF NOT EXISTS asset_sac (
    asset_type      Int16,
    asset_code      LowCardinality(String),
    issuer_id       Int64,
    contract_id     Int64,
    sac_contract_id SimpleAggregateFunction(max, Int64),
    sac_deployed    SimpleAggregateFunction(max, UInt8)
)
ENGINE = AggregatingMergeTree
ORDER BY (asset_type, asset_code, issuer_id, contract_id);

-- Off-chain SEP-1 enrichment for `assets` (task 0231). Written by the
-- enrichment-worker Lambda (NOT the indexer), keyed byte-for-byte like
-- `assets`, joined at read (`… FINAL`/`argMax`). Lives in a separate table
-- because the live indexer re-writes whole `assets` rows (enrichment columns
-- NULL) and would clobber it; `ReplacingMergeTree(version)` is order-safe under
-- retries and lets the enricher CLEAR a value (re-insert NULL with a higher
-- `version`). `version` = enricher processing timestamp (ms; non-nullable as
-- RMT requires). NOTE: `assets.{icon_url,name}` stay — `icon_url` there is
-- vestigial (always NULL; dropping it on the live table is a heavy ALTER, low
-- value — deferred to a cleanup task), and `name` is still indexer-owned for
-- soroban-native assets (read path does `COALESCE(asset_enrichment.name,
-- assets.name)`). Full reasoning + measured evidence: lore task 0231,
-- `notes/R-clickhouse-enrichment-write-strategy.md`.
CREATE TABLE IF NOT EXISTS asset_enrichment (
    asset_type   Int16,
    asset_code   LowCardinality(String),
    issuer_id    Int64,
    contract_id  Int64,
    icon_url     Nullable(String),
    name         Nullable(String),
    version      DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(version)
ORDER BY (asset_type, asset_code, issuer_id, contract_id);

-- NOTE: the legacy `asset_aggregates` table + `asset_aggregates_mv` (classic
-- supply/holders over `account_balances_current`, keyed code+issuer) were DROPPED
-- (task 0331) — superseded by `balance_aggregates` (below) over the unified
-- `balances` table, keyed by `assets.id`. It was a refreshable MV (derived), so
-- the drop loses no source data; classic supply now flows through `balances`
-- (single-write + the one-time classic→`balances` migration).
--
-- Pre-computed per-asset aggregates over the unified `balances` table (task 0331,
-- Option C) — supply + active-holder count keyed by the `assets.id` surrogate.
-- `total_supply` is RAW `Int128` (the read scales by the asset's `decimals`); a
-- Soroban token's supply needs raw Int128 (token-specific decimals; PIKA=43224
-- overflows any Decimal), so this table is raw for ALL asset types once classic
-- migrates in (step 6). Refreshable-MV (full recompute, atomic EXCHANGE → no FINAL
-- on read). Columns `Nullable` so a read LEFT-JOIN miss is NULL (→ "—"), not 0.
-- (classic enters `balances` via single-write + the one-time classic→`balances`
-- migration; type-3 + SAC/native contract-held via the live parser + seed.)
CREATE TABLE IF NOT EXISTS balance_aggregates (
    asset_id     Int64,
    total_supply Nullable(Int128),
    holder_count Nullable(Int32)
)
ENGINE = MergeTree
ORDER BY (asset_id);

-- NOTE: `balance_aggregates_mv` (the refreshable MV that fills this table) is
-- defined AFTER `balances` below — a `CREATE MATERIALIZED VIEW … FROM balances`
-- needs its source table to already exist on a fresh `init.sql` run.

-- (tombstone) `soroban_token_supply` was DROPPED — task 0331 Option-A decision.
-- A per-token authoritative `TotalSupply` key read (76.6% of type-3 tokens expose
-- one; 27% do not) added a second supply source + a seed-only staleness bug for
-- no measurable gain: a mint always credits a holder balance (often a contract
-- treasury, summed under Path A G+C holders), so `balance_aggregates.total_supply`
-- (Σ amount, MV-refreshed) equals the real supply. ONE universal method; the
-- narrow residue (TTL-archived tail + true rebasing) is the accepted non-100% cost.

CREATE TABLE IF NOT EXISTS account_balances_current (
    account_id          Int64,
    asset_type          Int16,
    asset_code          LowCardinality(String),
    issuer_id           Int64,        -- 0 for native
    balance             Decimal128(7),
    last_updated_ledger Int64
)
ENGINE = ReplacingMergeTree(last_updated_ledger)
ORDER BY (account_id, asset_type, asset_code, issuer_id);

-- ── Option C unified balance model (task 0331) ──────────────────────────────
-- The two tables below are the unified replacement for `account_balances_current`
-- (classic) + the interim `soroban_token_balances` (type-3). The persist path now
-- writes `balances` ONLY (single-write, task 0331 Option A); `account_balances_current`
-- is retained (no longer written) pending the one-time classic→`balances` data
-- migration + drop (OPS steps 6b/6d). See the task README.

-- Unified per-holder balances — the single balance model for ALL asset types.
-- `amount` is RAW `Int128` (scale by the asset's `decimals` at read — universal
-- fixed-point, handles classic 7-dec AND arbitrary Soroban decimals). `holder_id`
-- = `cityhash64(holder StrKey)` (the same surrogate space as `accounts.id` /
-- `soroban_contracts.id`; resolve back to a StrKey via `accounts` (G) or
-- `soroban_contracts` (C) — there is no dedicated address dimension). `asset_id`
-- → the `assets.id` surrogate (`ids::asset_id`). RMT version = `last_updated_ledger`;
-- a removed/zeroed balance writes 0 so a fully-spent holder collapses.
CREATE TABLE IF NOT EXISTS balances (
    holder_id           Int64,
    asset_id            Int64,
    amount              Int128,
    last_updated_ledger Int64
)
ENGINE = ReplacingMergeTree(last_updated_ledger)
-- holder_id FIRST: the account-detail read is a per-holder PK-prefix seek (the
-- hot, latency-critical path — mirrors `account_balances_current`'s account_id-first
-- key, avoids the 0198 Seq Scan). `balance_aggregates_mv` GROUP BY asset_id is a
-- periodic full-recompute scan either way, so it doesn't need asset_id-first.
ORDER BY (holder_id, asset_id);

-- Refreshable MV that recomputes `balance_aggregates` from `balances` (defined
-- above — the source table MUST exist before this CREATE). Full recompute + atomic
-- EXCHANGE, so reads need no FINAL.
CREATE MATERIALIZED VIEW IF NOT EXISTS balance_aggregates_mv
REFRESH EVERY 2 MINUTE
TO balance_aggregates AS
SELECT
    asset_id,
    sum(amount)                  AS total_supply,
    toInt32(countIf(amount > 0)) AS holder_count
FROM balances FINAL
GROUP BY asset_id;

CREATE TABLE IF NOT EXISTS nfts (
    contract_id           Int64,
    token_id              String,
    collection_name       Nullable(String),
    name                  Nullable(String),
    media_url             Nullable(String),
    minted_at_ledger      Nullable(Int64),
    current_owner_id      Nullable(Int64),
    current_owner_ledger  Int64 DEFAULT 0
)
ENGINE = ReplacingMergeTree(current_owner_ledger)
ORDER BY (contract_id, token_id);

-- Task 0217 — quarantine for NFT-candidate rows whose contract is still
-- `Other`/NULL-classified (WASM not observed in the current backfill
-- window). Same row shape as `nfts` so promotion is a column-projection
-- INSERT. API endpoints never read this table — production sees only
-- definitive `Nft`-classified rows in `nfts`. Promoted to `nfts` on
-- `Other → Nft` reclassification, dropped on `Other → Fungible`
-- reclassification. `Token` (SAC) is classified at deploy time and not
-- reachable via WASM reclassification — `Token`-classified contracts are
-- dropped at persist-filter time and never enter pending.
--
-- Note: CH-side writer-driven routing into `*_pending` is not implemented
-- in PR #180; tables ship as schema-only on CH. PG writer drives the
-- persist-time routing today. CH writer parity is a follow-up task.
CREATE TABLE IF NOT EXISTS nfts_pending (
    contract_id           Int64,
    token_id              String,
    collection_name       Nullable(String),
    name                  Nullable(String),
    media_url             Nullable(String),
    minted_at_ledger      Nullable(Int64),
    current_owner_id      Nullable(Int64),
    current_owner_ledger  Int64 DEFAULT 0
)
ENGINE = ReplacingMergeTree(current_owner_ledger)
ORDER BY (contract_id, token_id);

-- Off-chain `token_uri` enrichment for `nfts` (task 0231). Written by the
-- enrichment-worker Lambda (NOT the indexer), keyed like `nfts`, joined at
-- read. Separate table for the same reason as `asset_enrichment`: the live
-- indexer re-writes whole `nfts` rows on every ownership change (metadata
-- NULL) and the ownership clock (`current_owner_ledger`) is its RMT version —
-- so enrichment in `nfts` would be clobbered AND has no safe version to claim.
-- Here `version` is the enricher's own clock (ms), independent of ownership.
-- `nfts.{name,media_url,collection_name}` stay vestigial (NULL; DROP deferred
-- to a cleanup task). See lore task 0231,
-- `notes/R-clickhouse-enrichment-write-strategy.md`.
CREATE TABLE IF NOT EXISTS nft_enrichment (
    contract_id      Int64,
    token_id         String,
    name             Nullable(String),
    media_url        Nullable(String),
    collection_name  Nullable(String),
    version          DateTime64(3, 'UTC')
)
ENGINE = ReplacingMergeTree(version)
ORDER BY (contract_id, token_id);

-- liquidity_pools (task 0208 Path 2 folded inline): RMT(last_updated_ledger),
-- `created_at_ledger` dropped (derive read-time from
-- `MIN(ledger_sequence) FROM liquidity_pool_snapshots GROUP BY pool_id`).
CREATE TABLE IF NOT EXISTS liquidity_pools (
    pool_id              FixedString(32),
    asset_a_type         Int16,
    asset_a_code         LowCardinality(String),
    asset_a_issuer_id    Int64,        -- 0 for native
    asset_b_type         Int16,
    asset_b_code         LowCardinality(String),
    asset_b_issuer_id    Int64,
    fee_bps              Int32,
    last_updated_ledger  Int64
)
ENGINE = ReplacingMergeTree(last_updated_ledger)
ORDER BY (pool_id);

CREATE TABLE IF NOT EXISTS lp_positions (
    pool_id              FixedString(32),
    account_id           Int64,
    shares               Decimal128(7),
    first_deposit_ledger Int64,
    last_updated_ledger  Int64
)
ENGINE = ReplacingMergeTree(last_updated_ledger)
ORDER BY (pool_id, account_id);

----------------------------------------------------------------------
-- Append-only fact tables (ReplacingMergeTree, partitioned)
----------------------------------------------------------------------

-- transactions: surrogate `id Int64` for cheap FK joins from
-- operations_appearances, transaction_participants, soroban_events,
-- soroban_invocations_appearances, nft_ownership. ORDER BY
-- (ledger_sequence, application_order) for time-series scans.
CREATE TABLE IF NOT EXISTS transactions (
    id                Int64,
    hash              FixedString(32),
    ledger_sequence   Int64,
    application_order Int16,
    source_id         Int64,           -- FK to accounts.id
    fee_charged       Int64,
    inner_tx_hash     Nullable(FixedString(32)),
    successful        Bool,
    operation_count   Int16,
    has_soroban       Bool,
    parse_error       Bool,
    INDEX idx_tx_hash_bloom hash TYPE bloom_filter(0.01) GRANULARITY 1
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (ledger_sequence, application_order);

CREATE TABLE IF NOT EXISTS transaction_hash_index (
    hash            FixedString(32),
    ledger_sequence Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (hash);

-- `amount` is a **fold count** of identity-tuple duplicates (task 0163 /
-- ADR 0033 PG-side convention; CH inherits same semantic): the number of
-- on-chain operation envelope ops that collapsed into this single
-- appearance row by identity. NOT a stroop value or per-op amount. Real
-- per-op stroop values live in `result_meta_xdr` (Archive XDR overlay
-- on PG; read-time decode on CH via E03 statement C). API callers MUST
-- NOT interpret this column as a token amount.
CREATE TABLE IF NOT EXISTS operations_appearances (
    transaction_id    Int64,
    application_order Int16,
    type              Int16,
    source_id         Nullable(Int64),
    destination_id    Nullable(Int64),
    contract_id       Nullable(Int64),
    asset_code        LowCardinality(String),
    asset_issuer_id   Nullable(Int64),
    -- Crossed liquidity pools (task 0261/0268): single-element for LP
    -- deposit/withdraw, full crossed-pool list (result claim atoms) for
    -- path payments / offers, [] for no pool involvement (Array cannot be
    -- Nullable; has([], x) = 0 so empty arrays miss pool filters). Sorted +
    -- deduped by the stage fold. Filter with
    -- has(pool_ids, toFixedString(unhex(...), 32)).
    pool_ids          Array(FixedString(32)),
    amount            Int64,   -- fold count, see header comment
    ledger_sequence   Int64,
    -- Skip index for the `has(pool_ids, …)` pool filter (E20 /
    -- liquidity-pools/:id/transactions; task 0281 C). The read driver
    -- (fetch_pool_transactions) seeks via read-in-order `ORDER BY ledger DESC
    -- LIMIT`, so a POPULAR pool early-terminates near the tip; this bloom bounds
    -- the OTHER regime — a sparse pool whose last activity is far below the tip,
    -- where the driver must scan back to reach it. `bloom_filter(0.001)` (not the
    -- 0.025 default) keeps that scan's false-positive floor at ~0.1 % of the table
    -- (~6 M rows) instead of ~2.5 % (~155 M, box-measured 2026-06-17); same
    -- tight-FP rationale as the 0290 `idx_acc_id`.
    INDEX idx_oa_pool_ids pool_ids TYPE bloom_filter(0.001) GRANULARITY 1,
    -- Skip index for the contract-filtered transaction-list path (E03
    -- Statement B; task 0333). `contract_id` is NOT the ORDER BY prefix
    -- (unlike the `soroban_events` / `soroban_invocations_appearances` arms of
    -- the same UNION, which seek on `contract_id`), so this arm full-scanned.
    -- The read driver seeks via read-in-order `ORDER BY ledger DESC LIMIT`: a
    -- VERY active contract early-terminates near the tip (cheap), but a SPARSE
    -- contract — few/old appearances — forces a scan of the entire table to
    -- fill the page (box-measured: 42-appearance contract read 13.18 M / the
    -- whole table; this is the ~6.2 B-rows/query full scan that blew the prod
    -- `api_throttle.read_rows` quota on 2026-06-29, CH Code 201). This bloom
    -- bounds that sparse regime to the granules that actually hold the contract.
    -- `bloom_filter(0.001)` (not the 0.025 default) keeps the false-positive
    -- floor tight, same rationale as `idx_oa_pool_ids` / the 0290 `idx_acc_id`.
    -- contract_id is Nullable; `= <id>` never matches NULL rows, and granules
    -- holding only NULLs carry no value → skipped.
    INDEX idx_oa_contract_id contract_id TYPE bloom_filter(0.001) GRANULARITY 1
    -- idx_oa_asset_issuer_id (bloom on asset_issuer_id, was here for the E10
    -- asset-tx CLASSIC arm, task 0334) DROPPED 2026-07-13: task 0359 moved the
    -- asset-tx driver to the `operation_asset_appearances` fan-out seek, so no
    -- query filters `operations_appearances` by `asset_issuer_id` anymore — the
    -- bloom's sole consumer is gone (verified across api / audit-harness /
    -- backfill). Prod is an existing DB (this file is fresh-only): reclaim the
    -- ~97 MiB with `ALTER TABLE operations_appearances DROP INDEX idx_oa_asset_issuer_id`.
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (ledger_sequence, transaction_id, application_order);

CREATE TABLE IF NOT EXISTS transaction_participants (
    account_id      Int64,
    ledger_sequence Int64,
    transaction_id  Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (account_id, ledger_sequence, transaction_id);

-- operation_asset_appearances: per-(asset, transaction) presence index (task
-- 0359). The EXACT transaction_participants shape, with asset_id for account_id.
-- Fixes the single-asset-slot loss on operations_appearances -- offers store
-- ZERO assets there, path payments keep only destAsset, native XLM is an
-- empty-string sentinel. Here every asset DECLARED in any op body of a tx is one
-- row, keyed asset-first so a per-asset activity page is a PK-prefix seek.
-- asset_id = cityhash64 surrogate (ids::asset_id); native = hash64('native'), a
-- first-class key, never an empty sentinel. Pure presence: duplicate (asset, tx)
-- rows within a tx collapse in the RMT.
-- NOT FK-guaranteed against `assets`: a fact row can hold an asset_id with no
-- `assets` row (an asset touched only by failed txs, a rejected allow_trust with
-- source != issuer, or a trustline predating the ingest window). Harmless -- the
-- read path reaches a fact row only via an existing `assets` row, so such orphans
-- are unreachable dead weight, never wrong output.
-- `net_settled` (task 0393): net-settled "value moved" per (transaction, asset)
-- for the tx-list "Net settled" column. RAW `Nullable(Int128)` (scale by the
-- asset's `decimals` at read, like balances/total_supply) — the figure
-- `max(Σ positive account deltas, Σ negative account deltas)` over the tx's
-- transfers, computed one-shot per (tx, asset) in Rust from the AUTHORITATIVE
-- LEDGER balance changes (`persist::stage::ledger_deltas_net_settled` +
-- `xdr_parser::ledger_balance_deltas` + `xdr_parser::net_settled`) — account,
-- trustline, and ContractData balances; NEVER from token events (logs). It is the
-- network-flow FLOW VALUE: by the flow decomposition theorem a flow splits into
-- source→sink paths plus cycles, and a cycle contributes exactly zero — so a
-- wash / round-trip nets to `0` BY DEFINITION, not by accident (that zero-balance
-- cycle is also how the wash-trading literature identifies a wash). Gross would be
-- `Σ path + Σ cycle`; if ever wanted, `cycle volume = gross − net`.
-- NULLABLE ON PURPOSE: `NULL` = not computable (the reducer could not represent
-- the result, or a recognised event's amount was unreadable), `0` = genuinely
-- nothing settled net. Without the distinction a value that could not be computed
-- would masquerade as a real zero. The read filters `IS NOT NULL AND != 0`.
-- NON-KEY data column, version-less RMT: `net_settled` has a SINGLE writer —
-- `stage.rs`, run by both live ingest and the full S3 re-ingest — so live and
-- historical rows for a key are computed identically and the duplicate collapses
-- cleanly. The read dedups with `max(net_settled)` (`max` ignores NULL, so a
-- computed value wins over a not-computed one for the same key). There is
-- deliberately no version column: a downward "correction" of a deterministic
-- figure only happens when OUR reducer changes, which is a deploy event handled
-- by re-running the re-ingest + `OPTIMIZE FINAL` over the range —
-- not a runtime concern worth a per-row version + a full-table engine rebuild.
-- The tx-list "+ N other assets" affordance is a read-time COUNT of asset rows
-- per tx, not a stored column.
-- tx-list "value" read note (task 0393): the PK is `asset_id`-leading (for the
-- per-asset activity page), so the tx-list read filtering
-- `(ledger_sequence, transaction_id) IN (page keys)` is NOT a prefix seek — it
-- SCANS the pruned partition. Measured ~26M rows/page against a full partition
-- vs ~16k for the seek-based op-types query beside it. This endpoint family is
-- polled and previously exhausted the read quota in exactly this shape (tasks
-- 0243/0386), so a `(ledger, tx)`-ordered companion (the `accounts_recent`
-- pattern: plain MergeTree + refreshable MV + atomic EXCHANGE — a projection is
-- refused on an RMT, CH Code 344) is REQUIRED before this ships at scale. Tracked
-- as the read-path work in task 0393's Operations / follow-up section; the head
-- partition being young hides the cost today.
CREATE TABLE IF NOT EXISTS operation_asset_appearances (
    asset_id        Int64,
    ledger_sequence Int64,
    transaction_id  Int64,
    net_settled     Nullable(Int128),
    -- Bloom skip index on transaction_id (task 0393 read-path). The "Net settled"
    -- value read filters by (ledger_sequence, transaction_id), but the table is
    -- asset_id-leading, so that filter is a partition SCAN (~26M rows/mature
    -- partition), not a primary-key seek. A tx-list page's ~hundreds of tx_ids are
    -- scattered across the partition's granules; this bloom lets ClickHouse skip
    -- the granules holding none of them (~10x fewer rows read, measured shape).
    -- Same pattern as idx_oa_contract_id / idx_acc_id; RMT-safe (skip indexes are
    -- allowed — only projections are refused on RMT, task 0353). A full
    -- (ledger, tx)-leading companion table is the heavier fallback if this proves
    -- insufficient at scale.
    INDEX idx_oaa_transaction_id transaction_id TYPE bloom_filter(0.001) GRANULARITY 1
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (asset_id, ledger_sequence, transaction_id);

-- operation_pools: per-(pool, transaction) presence index (task 0365). The
-- pool-dimension twin of operation_asset_appearances / transaction_participants,
-- keyed pool-first so GET /liquidity-pools/:id/transactions is a PK-prefix seek
-- instead of the density-dependent has(pool_ids, X) scan over
-- operations_appearances (0281-C read-in-order driver, superseded). pool_id = the
-- raw 32-byte pool hash (already how operations_appearances.pool_ids stores each
-- crossing -- no surrogate). Populated by arrayJoin(pool_ids) in staging; pure
-- presence, so duplicate (pool, tx) rows within a tx collapse in the RMT.
-- Plain Int64 columns, matching transaction_participants / operation_asset_appearances.
CREATE TABLE IF NOT EXISTS operation_pools (
    pool_id         FixedString(32),
    ledger_sequence Int64,
    transaction_id  Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (pool_id, ledger_sequence, transaction_id);

-- soroban_events: full-content per-event row (ADR 0044 §4a unfold).
-- ZSTD codecs on the ScVal-decoded JSON columns. `signature` is the
-- first-topic Symbol, lifted for cheap `WHERE signature = 'transfer'`.
CREATE TABLE IF NOT EXISTS soroban_events (
    contract_id     Int64,
    transaction_id  Int64,
    ledger_sequence Int64,
    event_index     Int16,
    event_type      Int16,
    signature       LowCardinality(Nullable(String)),
    topics_xdr      String CODEC(ZSTD(3)),
    data_xdr        String CODEC(ZSTD(3))
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, transaction_id, event_index);

-- `amount` is a **fold count of invocation-tree nodes** aggregated into
-- this (contract, transaction, ledger) trio (per ADR 0034 PG-side
-- convention; CH inherits same semantic). Multiple invocations of the
-- same contract within the same tx's call graph collapse into one row
-- with `amount` = how many call-graph nodes were folded. NOT a token
-- amount. Real per-invocation `function_name` / `args` / `return_value`
-- live in the Archive XDR (ADR 0029/0034).
CREATE TABLE IF NOT EXISTS soroban_invocations_appearances (
    contract_id          Int64,
    transaction_id       Int64,
    ledger_sequence      Int64,
    caller_id            Nullable(Int64),
    caller_contract_id   Nullable(Int64),
    amount               Int32   -- fold count, see header comment
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, transaction_id);

CREATE TABLE IF NOT EXISTS nft_ownership (
    contract_id      Int64,
    token_id         String,
    ledger_sequence  Int64,
    event_order      Int16,
    transaction_id   Int64,
    owner_id         Nullable(Int64),
    event_type       Int16
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, token_id, ledger_sequence, event_order);

-- Task 0217 — quarantine companion to `nft_ownership`. Same row shape +
-- partitioning so promotion (`INSERT … SELECT FROM nft_ownership_pending`)
-- copies parts cleanly. API endpoints never read this table.
CREATE TABLE IF NOT EXISTS nft_ownership_pending (
    contract_id      Int64,
    token_id         String,
    ledger_sequence  Int64,
    event_order      Int16,
    transaction_id   Int64,
    owner_id         Nullable(Int64),
    event_type       Int16
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, token_id, ledger_sequence, event_order);

CREATE TABLE IF NOT EXISTS liquidity_pool_snapshots (
    pool_id         FixedString(32),
    ledger_sequence Int64,
    reserve_a       Decimal128(7),
    reserve_b       Decimal128(7),
    total_shares    Decimal128(7),
    tvl             Nullable(Decimal128(7)),
    volume          Nullable(Decimal128(7)),
    fee_revenue     Nullable(Decimal128(7)),
    -- Gross trade volume in asset-A units per (pool, ledger), computed from
    -- path-payment claim atoms (task 0261 extractor; written by the 0266
    -- backfill / 0247 wiring). USD volume/fee stay NULL until the Prices
    -- API lands (ADR 0053 read-time join).
    gross_volume_a  Nullable(Decimal128(7))
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (pool_id, ledger_sequence);

----------------------------------------------------------------------
-- Dictionary: hot path for `hash → ledger_sequence` lookups
----------------------------------------------------------------------

-- The dictionary SOURCE clause reads via an inner CH→CH client
-- connection. We use the dedicated `dict_reader` user defined in
-- `users.d/dict.xml` instead of `default`: that user has an empty
-- password (safe — restricted to the loopback interface by its
-- `<networks>` ACL) so this DDL stays free of any committed
-- credential, and a future password rotation on `default` does
-- not require rewriting the schema.
--
-- CONNECT_TIMEOUT / SEND_TIMEOUT / RECEIVE_TIMEOUT in the SOURCE
-- clause govern the dict-load connection's socket timeouts. CH
-- user-profile timeouts apply to QUERY execution but not to the
-- internal client connection an external dictionary opens — so
-- the safe place to bound a stuck dict load is here, in the
-- DDL. 60 s is generous for a loopback intra-container fetch of
-- a single-column index.
CREATE DICTIONARY IF NOT EXISTS transaction_hash_dict (
    hash            String,
    ledger_sequence Int64
)
PRIMARY KEY hash
SOURCE(CLICKHOUSE(
    HOST '127.0.0.1'
    PORT 9000
    TABLE 'transaction_hash_index'
    DB 'default'
    USER 'dict_reader'
    CONNECT_TIMEOUT 5
    SEND_TIMEOUT 60
    RECEIVE_TIMEOUT 60
))
LIFETIME(MIN 300 MAX 360)
LAYOUT(COMPLEX_KEY_CACHE(SIZE_IN_CELLS 1000000));
