#!/bin/bash
# CH mirror — complete setup matching real audit DB schema
# Usage: ./ch-mirror-setup.sh <pg_container_id_or_name>
#
# Find PG container:
#   docker ps --filter "ancestor=postgres" --format "{{.ID}}  {{.Names}}"

set -e

if [[ -z "$1" ]]; then
    echo "Error: PG container ID/name required."
    echo ""
    echo "Usage: $0 <pg_container_id_or_name>"
    echo ""
    echo "Available postgres containers:"
    docker ps --filter "ancestor=postgres" --format "  {{.ID}}  {{.Names}}  ({{.Image}})" 2>/dev/null
    docker ps --filter "name=postgres" --format "  {{.ID}}  {{.Names}}  ({{.Image}})" 2>/dev/null
    exit 1
fi

PG_CONTAINER="$1"

# Verify container exists
if ! docker ps --format "{{.ID}} {{.Names}}" | grep -qE "^${PG_CONTAINER}|${PG_CONTAINER}\$"; then
    echo "Error: container '$PG_CONTAINER' not found or not running."
    exit 1
fi
CH_CONTAINER="ch-mirror"
CH_USER="mirror"
CH_PASS="mirror"
PG_DB="soroban_block_explorer"
PG_USER="postgres"

run_pg() {
    docker exec "$PG_CONTAINER" psql -U "$PG_USER" -d "$PG_DB" "$@"
}

run_ch() {
    docker exec -i "$CH_CONTAINER" clickhouse-client --user "$CH_USER" --password "$CH_PASS" "$@"
}

echo "=== Step 1: Recreate database ==="
run_ch -q "DROP DATABASE IF EXISTS mirror"
run_ch -q "CREATE DATABASE mirror"

echo "=== Step 2: Create schemas ==="

run_ch <<'EOF'
-- ledgers
CREATE TABLE mirror.ledgers (
    sequence          UInt64 CODEC(Delta, LZ4),
    hash_hex          FixedString(64) CODEC(LZ4),
    closed_at         DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4),
    protocol_version  UInt32,
    transaction_count UInt32 CODEC(Delta, LZ4),
    base_fee          Int64 CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY sequence;

-- accounts
CREATE TABLE mirror.accounts (
    id                UInt64 CODEC(Delta, LZ4),
    account_id        FixedString(56) CODEC(LZ4),
    first_seen_ledger UInt64 CODEC(Delta, LZ4),
    last_seen_ledger  UInt64 CODEC(Delta, LZ4),
    sequence_number   Int64 CODEC(LZ4),
    home_domain       Nullable(String) CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY id;

-- soroban_contracts
CREATE TABLE mirror.soroban_contracts (
    id                      UInt64 CODEC(Delta, LZ4),
    contract_id             FixedString(56) CODEC(LZ4),
    wasm_hash_hex           Nullable(FixedString(64)) CODEC(LZ4),
    wasm_uploaded_at_ledger Nullable(UInt64) CODEC(LZ4),
    deployer_id             Nullable(UInt64) CODEC(LZ4),
    deployed_at_ledger      Nullable(UInt64) CODEC(LZ4),
    contract_type           Nullable(Int8),
    is_sac                  Bool,
    name                    Nullable(String) CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY id;

-- liquidity_pools
CREATE TABLE mirror.liquidity_pools (
    pool_id_hex       FixedString(64) CODEC(LZ4),
    asset_a_type      Int8,
    asset_a_code      Nullable(String) CODEC(LZ4),
    asset_a_issuer_id Nullable(UInt64) CODEC(LZ4),
    asset_b_type      Int8,
    asset_b_code      Nullable(String) CODEC(LZ4),
    asset_b_issuer_id Nullable(UInt64) CODEC(LZ4),
    fee_bps           UInt32,
    created_at_ledger UInt64 CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY pool_id_hex;

-- assets
CREATE TABLE mirror.assets (
    id            UInt32 CODEC(Delta, LZ4),
    asset_type    Int8,
    asset_code    Nullable(String) CODEC(LZ4),
    issuer_id     Nullable(UInt64) CODEC(LZ4),
    contract_id   Nullable(UInt64) CODEC(LZ4),
    name          Nullable(String) CODEC(LZ4),
    total_supply  Nullable(Decimal(28, 7)) CODEC(LZ4),
    holder_count  Nullable(UInt32) CODEC(LZ4),
    icon_url      Nullable(String) CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY id;

-- nfts
CREATE TABLE mirror.nfts (
    id                   UInt32 CODEC(Delta, LZ4),
    contract_id          UInt64 CODEC(LZ4),
    token_id             String CODEC(LZ4),
    collection_name      Nullable(String) CODEC(LZ4),
    name                 Nullable(String) CODEC(LZ4),
    media_url            Nullable(String) CODEC(LZ4),
    metadata             Nullable(String) CODEC(LZ4),
    minted_at_ledger     Nullable(UInt64) CODEC(LZ4),
    current_owner_id     Nullable(UInt64) CODEC(LZ4),
    current_owner_ledger Nullable(UInt64) CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY id;

-- account_balances_current
CREATE TABLE mirror.account_balances_current (
    account_id          UInt64 CODEC(Delta, LZ4),
    asset_type          Int8,
    asset_code          Nullable(String) CODEC(LZ4),
    issuer_id           Nullable(UInt64) CODEC(LZ4),
    balance             Decimal(28, 7) CODEC(LZ4),
    last_updated_ledger UInt64 CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY (account_id, asset_type, asset_code, issuer_id)
SETTINGS allow_nullable_key = 1;

-- lp_positions
CREATE TABLE mirror.lp_positions (
    pool_id_hex          FixedString(64) CODEC(LZ4),
    account_id           UInt64 CODEC(LZ4),
    shares               Decimal(28, 7) CODEC(LZ4),
    first_deposit_ledger UInt64 CODEC(LZ4),
    last_updated_ledger  UInt64 CODEC(LZ4)
)
ENGINE = MergeTree()
ORDER BY (pool_id_hex, account_id);

-- transaction_hash_index
CREATE TABLE mirror.transaction_hash_index (
    hash_hex        FixedString(64) CODEC(LZ4),
    ledger_sequence UInt64 CODEC(Delta, LZ4),
    created_at      DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4)
)
ENGINE = MergeTree()
ORDER BY hash_hex;

-- transactions (parent — copy from all partitions)
CREATE TABLE mirror.transactions (
    id                UInt64 CODEC(Delta, LZ4),
    hash_hex          FixedString(64) CODEC(LZ4),
    ledger_sequence   UInt64 CODEC(Delta, LZ4),
    application_order Int16,
    source_id         UInt64 CODEC(LZ4),
    fee_charged       Int64 CODEC(LZ4),
    inner_tx_hash_hex Nullable(FixedString(64)) CODEC(LZ4),
    successful        Bool,
    operation_count   Int16,
    has_soroban       Bool,
    parse_error       Bool,
    created_at        DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4),
    INDEX idx_hash hash_hex TYPE bloom_filter GRANULARITY 4
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (id, created_at);

-- transaction_participants
CREATE TABLE mirror.transaction_participants (
    transaction_id UInt64 CODEC(Delta, LZ4),
    account_id     UInt64 CODEC(LZ4),
    created_at     DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4),
    INDEX idx_account account_id TYPE bloom_filter GRANULARITY 4
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (transaction_id, account_id);

-- soroban_events_appearances
CREATE TABLE mirror.soroban_events_appearances (
    contract_id     UInt64 CODEC(LZ4),
    transaction_id  UInt64 CODEC(Delta, LZ4),
    ledger_sequence UInt64 CODEC(Delta, LZ4),
    amount          Int64 CODEC(LZ4),
    created_at      DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (contract_id, ledger_sequence, transaction_id);

-- soroban_invocations_appearances (note: has caller_contract_id)
CREATE TABLE mirror.soroban_invocations_appearances (
    contract_id        UInt64 CODEC(LZ4),
    transaction_id     UInt64 CODEC(Delta, LZ4),
    ledger_sequence    UInt64 CODEC(Delta, LZ4),
    caller_id          Nullable(UInt64) CODEC(LZ4),
    caller_contract_id Nullable(UInt64) CODEC(LZ4),
    amount             Int32 CODEC(LZ4),
    created_at         DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (contract_id, ledger_sequence, transaction_id);

-- operations_appearances
CREATE TABLE mirror.operations_appearances (
    id              UInt64 CODEC(Delta, LZ4),
    transaction_id  UInt64 CODEC(Delta, LZ4),
    type            Int8,
    source_id       Nullable(UInt64) CODEC(LZ4),
    destination_id  Nullable(UInt64) CODEC(LZ4),
    contract_id     Nullable(UInt64) CODEC(LZ4),
    asset_code      LowCardinality(Nullable(String)),
    asset_issuer_id Nullable(UInt64) CODEC(LZ4),
    pool_id_hex     Nullable(FixedString(64)) CODEC(LZ4),
    amount          Int64 CODEC(LZ4),
    ledger_sequence UInt64 CODEC(Delta, LZ4),
    created_at      DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4),
    INDEX idx_pool_id pool_id_hex TYPE bloom_filter GRANULARITY 4,
    INDEX idx_contract_id contract_id TYPE bloom_filter GRANULARITY 4,
    INDEX idx_source_id source_id TYPE bloom_filter GRANULARITY 4
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (id, created_at);

-- wasm_interface_metadata
CREATE TABLE mirror.wasm_interface_metadata (
    wasm_hash_hex FixedString(64) CODEC(LZ4),
    metadata      String CODEC(LZ4)  -- JSONB → String
)
ENGINE = MergeTree()
ORDER BY wasm_hash_hex;

-- nft_ownership (partitioned, may be empty in audit DB)
CREATE TABLE mirror.nft_ownership (
    nft_id          UInt32 CODEC(Delta, LZ4),
    transaction_id  UInt64 CODEC(Delta, LZ4),
    owner_id        Nullable(UInt64) CODEC(LZ4),
    event_type      Int8,
    ledger_sequence UInt64 CODEC(Delta, LZ4),
    event_order     Int16,
    created_at      DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (nft_id, created_at, ledger_sequence, event_order);

-- liquidity_pool_snapshots
CREATE TABLE mirror.liquidity_pool_snapshots (
    id              UInt64 CODEC(Delta, LZ4),
    pool_id_hex     FixedString(64) CODEC(LZ4),
    ledger_sequence UInt64 CODEC(Delta, LZ4),
    reserve_a       Decimal(28, 7) CODEC(LZ4),
    reserve_b       Decimal(28, 7) CODEC(LZ4),
    total_shares    Decimal(28, 7) CODEC(LZ4),
    tvl             Nullable(Decimal(28, 7)) CODEC(LZ4),
    volume          Nullable(Decimal(28, 7)) CODEC(LZ4),
    fee_revenue     Nullable(Decimal(28, 7)) CODEC(LZ4),
    created_at      DateTime64(6, 'UTC') CODEC(DoubleDelta, LZ4)
)
ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY (pool_id_hex, ledger_sequence);
EOF

echo "Schemas created."

echo "=== Step 3: Copy data — non-partitioned tables ==="

copy_table() {
    local tbl="$1"
    local select_clause="$2"

    local row_count=$(run_pg -tA -c "SELECT count(*) FROM $tbl;" 2>/dev/null | tr -d '[:space:]')

    if [[ -z "$row_count" ]] || [[ ! "$row_count" =~ ^[0-9]+$ ]]; then
        echo "  $tbl: cannot read row count, skipping"
        return
    fi

    if [[ "$row_count" -eq 0 ]]; then
        echo "  $tbl: empty, skipping"
        return
    fi

    echo "  $tbl: $row_count rows..."

    run_pg -c "COPY (SELECT $select_clause FROM $tbl) TO STDOUT WITH (FORMAT CSV, HEADER false, NULL '\\N')" \
    | run_ch --date_time_input_format=best_effort --query="INSERT INTO mirror.$tbl FORMAT CSV"

    echo "  $tbl: done"
}

copy_table_renamed() {
    # For tables where CH table name differs from PG (e.g. transaction_hash_index)
    local pg_tbl="$1"
    local ch_tbl="$2"
    local select_clause="$3"

    local row_count=$(run_pg -tA -c "SELECT count(*) FROM $pg_tbl;" 2>/dev/null | tr -d '[:space:]')

    if [[ -z "$row_count" ]] || [[ ! "$row_count" =~ ^[0-9]+$ ]]; then
        echo "  $pg_tbl → $ch_tbl: cannot read, skipping"
        return
    fi

    if [[ "$row_count" -eq 0 ]]; then
        echo "  $pg_tbl → $ch_tbl: empty, skipping"
        return
    fi

    echo "  $pg_tbl → $ch_tbl: $row_count rows..."

    run_pg -c "COPY (SELECT $select_clause FROM $pg_tbl) TO STDOUT WITH (FORMAT CSV, HEADER false, NULL '\\N')" \
    | run_ch --date_time_input_format=best_effort --query="INSERT INTO mirror.$ch_tbl FORMAT CSV"

    echo "  $pg_tbl → $ch_tbl: done"
}

copy_table "ledgers" \
    "sequence, encode(hash, 'hex') AS hash_hex, closed_at, protocol_version, transaction_count, base_fee"

copy_table "accounts" \
    "id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain"

copy_table "soroban_contracts" \
    "id, contract_id, encode(wasm_hash, 'hex') AS wasm_hash_hex, wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, contract_type, is_sac, name"

copy_table "liquidity_pools" \
    "encode(pool_id, 'hex') AS pool_id_hex, asset_a_type, asset_a_code, asset_a_issuer_id, asset_b_type, asset_b_code, asset_b_issuer_id, fee_bps, created_at_ledger"

copy_table "assets" \
    "id, asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url"

copy_table "nfts" \
    "id, contract_id, token_id, collection_name, name, media_url, metadata::TEXT AS metadata, minted_at_ledger, current_owner_id, current_owner_ledger"

copy_table "account_balances_current" \
    "account_id, asset_type, asset_code, issuer_id, balance, last_updated_ledger"

copy_table "lp_positions" \
    "encode(pool_id, 'hex') AS pool_id_hex, account_id, shares, first_deposit_ledger, last_updated_ledger"

copy_table_renamed "transaction_hash_index" "transaction_hash_index" \
    "encode(hash, 'hex') AS hash_hex, ledger_sequence, created_at"

copy_table "wasm_interface_metadata" \
    "encode(wasm_hash, 'hex') AS wasm_hash_hex, metadata::TEXT AS metadata"

echo "=== Step 4: Copy data — partitioned tables (per partition) ==="

PARTITIONS=$(run_pg -tA -c "SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename ~ '_y[0-9]+m[0-9]+\$' ORDER BY tablename")

for partition in $PARTITIONS; do
    case "$partition" in
        operations_appearances_y*)
            ch_tbl="operations_appearances"
            select_clause="id, transaction_id, type, source_id, destination_id, contract_id, asset_code, asset_issuer_id, encode(pool_id, 'hex') AS pool_id_hex, amount, ledger_sequence, created_at"
            ;;
        soroban_events_appearances_y*)
            ch_tbl="soroban_events_appearances"
            select_clause="contract_id, transaction_id, ledger_sequence, amount, created_at"
            ;;
        soroban_invocations_appearances_y*)
            ch_tbl="soroban_invocations_appearances"
            select_clause="contract_id, transaction_id, ledger_sequence, caller_id, caller_contract_id, amount, created_at"
            ;;
        transactions_y*)
            ch_tbl="transactions"
            select_clause="id, encode(hash, 'hex') AS hash_hex, ledger_sequence, application_order, source_id, fee_charged, encode(inner_tx_hash, 'hex') AS inner_tx_hash_hex, successful, operation_count, has_soroban, parse_error, created_at"
            ;;
        transaction_participants_y*)
            ch_tbl="transaction_participants"
            select_clause="transaction_id, account_id, created_at"
            ;;
        liquidity_pool_snapshots_y*)
            ch_tbl="liquidity_pool_snapshots"
            select_clause="id, encode(pool_id, 'hex') AS pool_id_hex, ledger_sequence, reserve_a, reserve_b, total_shares, tvl, volume, fee_revenue, created_at"
            ;;
        nft_ownership_y*)
            ch_tbl="nft_ownership"
            select_clause="nft_id, transaction_id, owner_id, event_type, ledger_sequence, event_order, created_at"
            ;;
        *)
            echo "  $partition: skipping (no parent mapping)"
            continue
            ;;
    esac

    row_count=$(run_pg -tA -c "SELECT count(*) FROM $partition;" 2>/dev/null | tr -d '[:space:]')

    if [[ -z "$row_count" ]] || [[ "$row_count" -eq 0 ]]; then
        echo "  $partition: empty, skipping"
        continue
    fi

    echo "  $partition → mirror.$ch_tbl: $row_count rows..."

    run_pg -c "COPY (SELECT $select_clause FROM $partition) TO STDOUT WITH (FORMAT CSV, HEADER false, NULL '\\N')" \
    | run_ch --date_time_input_format=best_effort --query="INSERT INTO mirror.$ch_tbl FORMAT CSV"

    echo "  $partition: done"
done

echo "=== Step 5: Optimize merges ==="
for tbl in operations_appearances soroban_events_appearances soroban_invocations_appearances transactions transaction_participants liquidity_pool_snapshots nft_ownership ledgers accounts soroban_contracts liquidity_pools assets nfts account_balances_current lp_positions transaction_hash_index wasm_interface_metadata; do
    echo "  Optimizing mirror.$tbl..."
    run_ch -q "OPTIMIZE TABLE mirror.$tbl FINAL" 2>/dev/null || echo "    (skip — table empty or doesn't exist)"
done

echo "=== Step 6: Verification ==="
run_ch <<'EOF'
SELECT
    table,
    formatReadableSize(sum(bytes_on_disk)) AS compressed,
    formatReadableSize(sum(data_uncompressed_bytes)) AS raw,
    round(sum(data_uncompressed_bytes) / sum(bytes_on_disk), 2) AS ratio,
    sum(rows) AS rows
FROM system.parts
WHERE active AND database = 'mirror'
GROUP BY table
ORDER BY sum(bytes_on_disk) DESC
FORMAT PrettyCompact;
EOF

echo "=== Done ==="
