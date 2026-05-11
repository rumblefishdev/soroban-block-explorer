#!/usr/bin/env bash
# Merge every *.dump in $SNAPSHOTS_DIR (sorted by filename — `01_`, `02_`,
# … from gen-merge-snapshots.sh sort lexically into chronological order)
# into postgres-merge, then run `db-merge finalize`.
#
# Usage: bash scripts/run-merge-snapshots.sh
# Env:
#   SNAPSHOTS_DIR  default .temp/merge-snapshots
#   RESET=1        wipe pgdata-merge + pgdata-snapshot-source first
#                  (required if postgres-merge has stale data; without
#                  it the chronological-only preflight will reject the
#                  first ingest)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SNAPSHOTS_DIR="${SNAPSHOTS_DIR:-$REPO_ROOT/.temp/merge-snapshots}"
RESET="${RESET:-0}"

URL_MERGE="postgresql://postgres:postgres@localhost:5436/soroban_block_explorer"
URL_SNAPSHOT_SOURCE="postgresql://postgres:postgres@localhost:5437/soroban_block_explorer"

# Discover snapshots — lexical sort matches the `0N_` numeric prefix.
mapfile -t SNAPSHOTS < <(find "$SNAPSHOTS_DIR" -maxdepth 1 -name '*.dump' -type f | sort)
if (( ${#SNAPSHOTS[@]} == 0 )); then
    echo "ERROR: no *.dump files in $SNAPSHOTS_DIR" >&2
    exit 1
fi
echo "Found ${#SNAPSHOTS[@]} snapshots (ingest order):"
for s in "${SNAPSHOTS[@]}"; do echo "  $s"; done

if [[ "$RESET" == "1" ]]; then
    echo
    echo ">>> RESET=1 — wiping pgdata-merge + pgdata-snapshot-source"
    docker compose --profile db-merge stop postgres-merge postgres-snapshot-source 2>/dev/null || true
    docker compose --profile db-merge rm -f postgres-merge postgres-snapshot-source 2>/dev/null || true
    docker volume rm \
        sorban-block-explorer_pgdata-merge \
        sorban-block-explorer_pgdata-snapshot-source 2>/dev/null || true
fi

echo
echo ">>> docker compose --profile db-merge up -d postgres-merge postgres-snapshot-source"
docker compose --profile db-merge up -d postgres-merge postgres-snapshot-source

echo ">>> waiting for postgres-merge..."
until docker compose exec -T postgres-merge pg_isready -U postgres >/dev/null 2>&1; do
    sleep 1
done

echo
echo ">>> sqlx migrate run → postgres-merge"
DATABASE_URL="$URL_MERGE" sqlx migrate run --source crates/db/migrations

echo
echo ">>> create *_default partitions on postgres-merge"
# Match the snapshot layout: backfill-bench calls
# `db_partition_mgmt::ensure_default_partition` per table (default only,
# no monthly children). The full CLI (`ensure_all_partitions`) would
# create monthly children too — preflight then rejects on partition-
# layout mismatch (ADR 0040 §Partition handling).
docker compose exec -T postgres-merge \
    psql -U postgres -d soroban_block_explorer -v ON_ERROR_STOP=1 <<'SQL'
CREATE TABLE IF NOT EXISTS transactions_default                    PARTITION OF transactions                    DEFAULT;
CREATE TABLE IF NOT EXISTS operations_appearances_default          PARTITION OF operations_appearances          DEFAULT;
CREATE TABLE IF NOT EXISTS transaction_participants_default        PARTITION OF transaction_participants        DEFAULT;
CREATE TABLE IF NOT EXISTS soroban_invocations_appearances_default PARTITION OF soroban_invocations_appearances DEFAULT;
CREATE TABLE IF NOT EXISTS soroban_events_appearances_default      PARTITION OF soroban_events_appearances      DEFAULT;
CREATE TABLE IF NOT EXISTS nft_ownership_default                   PARTITION OF nft_ownership                   DEFAULT;
CREATE TABLE IF NOT EXISTS liquidity_pool_snapshots_default        PARTITION OF liquidity_pool_snapshots        DEFAULT;
SQL

echo
echo ">>> build db-merge"
cargo build --release -p db-merge
DB_MERGE_BIN="$REPO_ROOT/target/release/db-merge"

for snap in "${SNAPSHOTS[@]}"; do
    echo
    echo "=========================================================="
    echo "ingest $(basename "$snap")"
    echo "=========================================================="
    "$DB_MERGE_BIN" ingest "$snap" \
        --target-url "$URL_MERGE" \
        --snapshot-source-url "$URL_SNAPSHOT_SOURCE"
done

echo
echo "=========================================================="
echo "finalize"
echo "=========================================================="
"$DB_MERGE_BIN" finalize --target-url "$URL_MERGE"

echo
echo "=========================================================="
echo "DONE"
echo "=========================================================="
echo "  merge target: $URL_MERGE"
echo "  inspect:      psql '$URL_MERGE'"
echo "  pre-merge backups: $REPO_ROOT/.temp/db-merge-backups/ (remove when satisfied)"
