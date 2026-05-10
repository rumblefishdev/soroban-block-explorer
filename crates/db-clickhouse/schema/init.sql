-- ClickHouse pilot schema (task 0204, ADR 0044).
--
-- Mirrors the Postgres `public` schema snapshot taken on 2026-05-08, with
-- the five deliberate divergences listed in ADR 0044 §Decision §4:
--
--   1. `soroban_events_appearances` is replaced by full-content
--      `soroban_events` (per-event row, full XDR inlined).
--   2. `created_at` is dropped from every CH table except `ledgers`
--      (CH partitions by `ledger_sequence`; wall-clock recoverable via
--      JOIN to `ledgers.closed_at`).
--   3. `nfts.metadata` is dropped (CH only).
--   4. `_sqlx_migrations` is not mirrored — this file IS the migration.
--   5. `transaction_hash_index` is exposed as a `Dictionary` for fast
--      `hash → ledger_sequence` lookups.
--
-- All statements are idempotent (`CREATE TABLE IF NOT EXISTS`,
-- `CREATE DICTIONARY IF NOT EXISTS`); applying twice is a no-op.
--
-- Engine assignment per ADR 0044 §Decision §5:
--   - append-only fact tables  → ReplacingMergeTree (dedup by ORDER BY key)
--   - state tables             → ReplacingMergeTree(version_column) where
--                                a natural NOT NULL ledger column exists
--                                (otherwise plain ReplacingMergeTree)
--   - immutable lookup tables  → MergeTree
--
-- Partitioning: every fact table uses `intDiv(ledger_sequence, 500000)`
-- (~29 days at 5 s/ledger). State tables are not partitioned.

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

CREATE TABLE IF NOT EXISTS liquidity_pools (
    pool_id            FixedString(32),
    asset_a_type       Int16,
    asset_a_code       Nullable(String),
    asset_a_issuer_id  Nullable(Int64),
    asset_b_type       Int16,
    asset_b_code       Nullable(String),
    asset_b_issuer_id  Nullable(Int64),
    fee_bps            Int32,
    created_at_ledger  Int64
)
ENGINE = MergeTree
ORDER BY (pool_id);

CREATE TABLE IF NOT EXISTS wasm_interface_metadata (
    wasm_hash FixedString(32),
    metadata  String
)
ENGINE = MergeTree
ORDER BY (wasm_hash);

----------------------------------------------------------------------
-- State tables (ReplacingMergeTree, with version column where natural)
----------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS accounts (
    id                Int64,
    account_id        String,
    first_seen_ledger Int64,
    last_seen_ledger  Int64,
    sequence_number   Int64,
    home_domain       Nullable(String)
)
ENGINE = ReplacingMergeTree(last_seen_ledger)
ORDER BY (id);

CREATE TABLE IF NOT EXISTS assets (
    id           Int32,
    asset_type   Int16,
    asset_code   Nullable(String),
    issuer_id    Nullable(Int64),
    contract_id  Nullable(Int64),
    name         Nullable(String),
    total_supply Nullable(Decimal128(7)),
    holder_count Nullable(Int32),
    icon_url     Nullable(String)
)
ENGINE = ReplacingMergeTree
ORDER BY (id);

-- ORDER BY contains nullable columns (asset_code, issuer_id) — pilot
-- accepts the `allow_nullable_key` setting because the partial-unique
-- semantics from PG (native vs credit) don't translate to CH.
CREATE TABLE IF NOT EXISTS account_balances_current (
    account_id          Int64,
    asset_type          Int16,
    asset_code          Nullable(String),
    issuer_id           Nullable(Int64),
    balance             Decimal128(7),
    last_updated_ledger Int64
)
ENGINE = ReplacingMergeTree(last_updated_ledger)
ORDER BY (account_id, asset_type, asset_code, issuer_id)
SETTINGS allow_nullable_key = 1;

-- soroban_contracts: the natural version column (`wasm_uploaded_at_ledger`)
-- is Nullable in PG; CH coerces it to NOT NULL with default 0 on the CH
-- side so it can drive ReplacingMergeTree dedup. PG keeps NULL semantics
-- unchanged.
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
ORDER BY (id);

-- nfts: same Nullable→NOT NULL coercion on the version column
-- `current_owner_ledger` (CH side only). `metadata` column dropped per
-- ADR 0044 §Decision §4c.
CREATE TABLE IF NOT EXISTS nfts (
    id                   Int32,
    contract_id          Int64,
    token_id             String,
    collection_name      Nullable(String),
    name                 Nullable(String),
    media_url            Nullable(String),
    minted_at_ledger     Nullable(Int64),
    current_owner_id     Nullable(Int64),
    current_owner_ledger Int64 DEFAULT 0
)
ENGINE = ReplacingMergeTree(current_owner_ledger)
ORDER BY (id);

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

CREATE TABLE IF NOT EXISTS transactions (
    id                Int64,
    hash              FixedString(32),
    ledger_sequence   Int64,
    application_order Int16,
    source_id         Int64,
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
ORDER BY (ledger_sequence, application_order, id);

CREATE TABLE IF NOT EXISTS transaction_hash_index (
    hash            FixedString(32),
    ledger_sequence Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (hash);

CREATE TABLE IF NOT EXISTS operations_appearances (
    id                Int64,
    transaction_id    Int64,
    type              Int16,
    source_id         Nullable(Int64),
    destination_id    Nullable(Int64),
    contract_id       Nullable(Int64),
    asset_code        Nullable(String),
    asset_issuer_id   Nullable(Int64),
    pool_id           Nullable(FixedString(32)),
    amount            Int64,
    ledger_sequence   Int64,
    application_order Nullable(Int16)
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (ledger_sequence, transaction_id, id);

CREATE TABLE IF NOT EXISTS transaction_participants (
    account_id      Int64,
    ledger_sequence Int64,
    transaction_id  Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (account_id, ledger_sequence, transaction_id);

-- soroban_events: full-content per-event row (ADR 0044 §Decision §4a).
-- This is the v3-spike design that ADR 0033 deliberately folded in PG;
-- CH unfolds it because columnar compression makes per-row event detail
-- cheap. PG remains on the folded `soroban_events_appearances` table.
CREATE TABLE IF NOT EXISTS soroban_events (
    contract_id     Int64,
    transaction_id  Int64,
    ledger_sequence Int64,
    event_index     Int16,
    event_type      Int16,
    signature       Nullable(String),
    topics_xdr      String,
    data_xdr        String
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, transaction_id, event_index);

CREATE TABLE IF NOT EXISTS soroban_invocations_appearances (
    contract_id        Int64,
    transaction_id     Int64,
    ledger_sequence    Int64,
    caller_id          Nullable(Int64),
    caller_contract_id Nullable(Int64),
    amount             Int32
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (contract_id, ledger_sequence, transaction_id);

CREATE TABLE IF NOT EXISTS nft_ownership (
    nft_id          Int32,
    ledger_sequence Int64,
    event_order     Int16,
    transaction_id  Int64,
    owner_id        Nullable(Int64),
    event_type      Int16
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (nft_id, ledger_sequence, event_order);

CREATE TABLE IF NOT EXISTS liquidity_pool_snapshots (
    id              Int64,
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
ORDER BY (pool_id, ledger_sequence, id);

----------------------------------------------------------------------
-- Dictionary: hot path for `hash → ledger_sequence` lookups
-- (ADR 0044 §Decision §4e). Backed by the `transaction_hash_index`
-- table; complex_key_cache layout is RAM-bounded and refreshes every
-- 5 minutes. Misses fall through to scanning the source table.
----------------------------------------------------------------------

-- Dictionary attribute types: ClickHouse 26.x requires `String` here
-- for the byte-string key (FixedString is unsupported in dict
-- attribute slots). The source table keeps `FixedString(32)`; the
-- dictionary loader coerces transparently. Callers pass the hash
-- through `toString(...)` or `unhex(...)` already, so this is
-- transparent on the read path.
--
-- USER/PASSWORD literals match the local-dev posture in
-- `docker-compose.yml` (default user with non-secret password). For
-- production deployment, replace with a named collection so the
-- credential is not version-controlled.
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
