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
--   - immutable lookup tables  → MergeTree
--
-- Partitioning: every fact table uses `intDiv(ledger_sequence, 500000)`
-- (~29 days at 5 s/ledger). State and immutable tables not partitioned.
--
-- All statements are idempotent (`CREATE TABLE IF NOT EXISTS`,
-- `CREATE DICTIONARY IF NOT EXISTS`); applying twice is a no-op.

----------------------------------------------------------------------
-- Immutable lookups (MergeTree)
----------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS ledgers (
    sequence          Int64,
    hash              FixedString(32),
    closed_at         DateTime64(3, 'UTC'),
    protocol_version  Int32,
    transaction_count Int32,
    base_fee          Int64
)
ENGINE = MergeTree
PARTITION BY intDiv(sequence, 500000)
ORDER BY (sequence);

CREATE TABLE IF NOT EXISTS wasm_interface_metadata (
    wasm_hash FixedString(32),
    metadata  String CODEC(ZSTD(3))
)
ENGINE = MergeTree
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
    -- home_domain: write-once per account, very low cardinality
    -- globally (mainnet has a handful of unique SEP-1 issuers'
    -- domains across tens of millions of accounts; the vast majority
    -- are NULL). LowCardinality dictionary-encodes the few unique
    -- values per block — strong compression on top of default LZ4.
    home_domain       LowCardinality(Nullable(String))
)
ENGINE = ReplacingMergeTree(last_seen_ledger)
ORDER BY (account_id);

-- soroban_contracts: same hybrid pattern as accounts.
-- `wasm_uploaded_at_ledger` is the version slot; `DEFAULT 0` is the
-- stub-row sentinel (Pass 2 stub-rowing for referenced-but-not-deployed
-- contracts in mid-stream backfill ranges).
CREATE TABLE IF NOT EXISTS soroban_contracts (
    id                       Int64,
    contract_id              String,
    wasm_hash                Nullable(FixedString(32)),
    wasm_uploaded_at_ledger  Int64 DEFAULT 0,
    deployer_id              Nullable(Int64),
    deployed_at_ledger       Nullable(Int64),
    contract_type            Nullable(Int16),
    is_sac                   Bool,
    name                     Nullable(String)
)
ENGINE = ReplacingMergeTree(wasm_uploaded_at_ledger)
ORDER BY (contract_id);

----------------------------------------------------------------------
-- State tables — composite natural keys (no surrogate id)
----------------------------------------------------------------------

-- assets identity is a 4-tuple. Native XLM: all-empty + asset_type=0.
-- Classic credit: code+issuer set, contract_id=0. SAC: contract_id
-- set, code/issuer optional. Soroban-native: contract_id set,
-- code/issuer=empty/0.
CREATE TABLE IF NOT EXISTS assets (
    asset_type      Int16,
    asset_code      LowCardinality(String),
    issuer_id       Int64,            -- 0 for native / soroban-native
    contract_id     Int64,            -- 0 for native / classic-credit
    name            Nullable(String),
    total_supply    Nullable(Decimal128(7)),
    holder_count    Nullable(Int32),
    icon_url        Nullable(String)
)
ENGINE = ReplacingMergeTree
ORDER BY (asset_type, asset_code, issuer_id, contract_id);

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

CREATE TABLE IF NOT EXISTS operations_appearances (
    transaction_id    Int64,
    application_order Int16,
    type              Int16,
    source_id         Nullable(Int64),
    destination_id    Nullable(Int64),
    contract_id       Nullable(Int64),
    asset_code        LowCardinality(String),
    asset_issuer_id   Nullable(Int64),
    pool_id           Nullable(FixedString(32)),
    amount            Int64,
    ledger_sequence   Int64
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

CREATE TABLE IF NOT EXISTS soroban_invocations_appearances (
    contract_id          Int64,
    transaction_id       Int64,
    ledger_sequence      Int64,
    caller_id            Nullable(Int64),
    caller_contract_id   Nullable(Int64),
    amount               Int32
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

CREATE TABLE IF NOT EXISTS liquidity_pool_snapshots (
    pool_id         FixedString(32),
    ledger_sequence Int64,
    reserve_a       Decimal128(7),
    reserve_b       Decimal128(7),
    total_shares    Decimal128(7),
    tvl             Nullable(Decimal128(7)),
    volume          Nullable(Decimal128(7)),
    fee_revenue     Nullable(Decimal128(7))
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (pool_id, ledger_sequence);

----------------------------------------------------------------------
-- Dictionary: hot path for `hash → ledger_sequence` lookups
----------------------------------------------------------------------

CREATE DICTIONARY IF NOT EXISTS transaction_hash_dict (
    hash            String,
    ledger_sequence Int64
)
PRIMARY KEY hash
SOURCE(CLICKHOUSE(
    TABLE 'transaction_hash_index'
    DB 'default'
    USER 'default'
    PASSWORD 'clickhouse'
))
LIFETIME(MIN 300 MAX 360)
LAYOUT(COMPLEX_KEY_CACHE(SIZE_IN_CELLS 1000000));
