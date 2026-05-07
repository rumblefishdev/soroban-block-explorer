#!/usr/bin/env bash
# Sequentially generate N disjoint snapshots ready for `db-merge ingest`.
# Each iteration: docker compose down -v --rmi local → up -d → db:migrate
# → backfill-bench [LO..HI] → pg_dump (zstd:19, custom format).
#
# DESTRUCTIVE — `down -v` wipes EVERY volume in this compose project,
# including pgdata-merge and pgdata-snapshot-source if they exist. Run
# this only during the snapshot-generation phase, before any merge work.
#
# Usage: bash scripts/gen-merge-snapshots.sh
# Env overrides:
#   START      first ledger (default 62016000)
#   COUNT      ledgers per iteration (default 250)
#   ITERATIONS number of iterations  (default 4)
#   OUT_DIR    snapshot output dir   (default .temp/merge-snapshots)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

START="${START:-62016000}"
COUNT="${COUNT:-250}"
ITERATIONS="${ITERATIONS:-4}"
OUT_DIR="${OUT_DIR:-$REPO_ROOT/.temp/merge-snapshots}"

DB_URL="postgresql://postgres:postgres@localhost:5432/soroban_block_explorer"

# backfill-bench needs STELLAR_NETWORK_PASSPHRASE (SAC contract_id
# derivation). npm scripts pick it up automatically; bash doesn't.
if [[ -f "$REPO_ROOT/.env" ]]; then
    set -a
    . "$REPO_ROOT/.env"
    set +a
fi

mkdir -p "$OUT_DIR"

for i in $(seq 1 "$ITERATIONS"); do
    LO=$((START + (i - 1) * COUNT))
    HI=$((LO + COUNT - 1))
    LABEL=$(printf "%02d" "$i")
    SNAPSHOT="$OUT_DIR/${LABEL}_ledgers-${LO}-${HI}.dump"

    echo
    echo "=========================================================="
    echo "Iter ${i}/${ITERATIONS}: ledgers ${LO}..${HI}"
    echo "Output: ${SNAPSHOT}"
    echo "=========================================================="

    echo ">>> docker compose down -v --rmi local"
    docker compose down -v --rmi local

    echo ">>> docker compose up -d"
    docker compose up -d

    echo ">>> waiting for postgres..."
    until docker compose exec -T postgres pg_isready -U postgres >/dev/null 2>&1; do
        sleep 1
    done

    echo ">>> npm run db:migrate"
    npm run db:migrate

    echo ">>> cargo run --release -p backfill-bench -- --start ${LO} --end ${HI}"
    cargo run --release -p backfill-bench -- --start "$LO" --end "$HI"

    echo ">>> pg_dump → ${SNAPSHOT}"
    pg_dump \
        --format=custom \
        --compress=zstd:19 \
        --no-owner --no-privileges \
        --file "$SNAPSHOT" \
        "$DB_URL"

    ls -lh "$SNAPSHOT"
done

echo
echo "=========================================================="
echo "DONE: ${ITERATIONS} snapshots in ${OUT_DIR}"
echo "=========================================================="
ls -lh "$OUT_DIR"
echo
echo "Next: bring up merge runtime + ingest in chronological order:"
echo
echo "  docker compose --profile db-merge up -d postgres-merge postgres-snapshot-source"
echo "  DATABASE_URL=postgresql://postgres:postgres@localhost:5436/soroban_block_explorer \\"
echo "      sqlx migrate run --source crates/db/migrations"
for i in $(seq 1 "$ITERATIONS"); do
    LO=$((START + (i - 1) * COUNT))
    HI=$((LO + COUNT - 1))
    LABEL=$(printf "%02d" "$i")
    echo "  cargo run --release -p db-merge -- ingest ${OUT_DIR}/${LABEL}_ledgers-${LO}-${HI}.dump \\"
    echo "      --target-url postgresql://postgres:postgres@localhost:5436/soroban_block_explorer \\"
    echo "      --snapshot-source-url postgresql://postgres:postgres@localhost:5437/soroban_block_explorer"
done
echo "  cargo run --release -p db-merge -- finalize \\"
echo "      --target-url postgresql://postgres:postgres@localhost:5436/soroban_block_explorer"
