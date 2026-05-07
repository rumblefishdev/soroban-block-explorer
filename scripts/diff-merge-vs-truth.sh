#!/usr/bin/env bash
# Build a sequential ground-truth backfill on `postgres` (5432) covering
# the same range that was merged into `postgres-merge` (5436), run
# `db-merge finalize` on both sides for parity, then `db-merge diff` —
# 17 tables × natural-key projection × md5. Exit 0 if all match, 1 if
# any mismatch.
#
# DESTRUCTIVE on 5432 — wipes pgdata to ensure a clean single-laptop
# sequential backfill (the reference the merger is being compared
# against). If 5432 already has the right ground truth from a prior
# run, set SKIP_TRUTH=1 to skip the wipe + backfill phase.
#
# Usage: bash scripts/diff-merge-vs-truth.sh
# Env:
#   START         first ledger (default 62016000)
#   END           last ledger  (default 62016999)
#   SKIP_TRUTH=1  reuse whatever 5432 currently has (no wipe/backfill)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

START="${START:-62016000}"
END="${END:-62016999}"
SKIP_TRUTH="${SKIP_TRUTH:-0}"

URL_TRUTH="postgresql://postgres:postgres@localhost:5432/soroban_block_explorer"
URL_MERGE="postgresql://postgres:postgres@localhost:5436/soroban_block_explorer"

# backfill-bench needs STELLAR_NETWORK_PASSPHRASE.
if [[ -f "$REPO_ROOT/.env" ]]; then
    set -a
    . "$REPO_ROOT/.env"
    set +a
fi

if [[ "$SKIP_TRUTH" != "1" ]]; then
    echo "=========================================================="
    echo "Building ground truth on postgres (5432): ${START}..${END}"
    echo "=========================================================="

    echo ">>> docker compose down -v --rmi local"
    docker compose down -v --rmi local

    echo ">>> docker compose up -d"
    docker compose up -d postgres

    echo ">>> waiting for postgres..."
    until docker compose exec -T postgres pg_isready -U postgres >/dev/null 2>&1; do
        sleep 1
    done

    echo ">>> npm run db:migrate"
    npm run db:migrate

    echo ">>> cargo run --release -p backfill-bench -- --start ${START} --end ${END}"
    cargo run --release -p backfill-bench -- --start "$START" --end "$END"
else
    echo ">>> SKIP_TRUTH=1 — reusing whatever's on $URL_TRUTH"
fi

echo
echo ">>> docker compose --profile db-merge up -d postgres-merge"
docker compose --profile db-merge up -d postgres-merge

echo ">>> waiting for postgres-merge..."
until docker compose exec -T postgres-merge pg_isready -U postgres >/dev/null 2>&1; do
    sleep 1
done

echo
echo ">>> build db-merge"
cargo build --release -p db-merge
DB_MERGE_BIN="$REPO_ROOT/target/release/db-merge"

# Run finalize on both sides for parity. Truth's `nfts.current_owner_*`
# came from the indexer's live LWW writes; merge's came from the
# finalize rebuild over `nft_ownership`. Re-run finalize on truth so
# both sides go through the identical batch rebuild — eliminates
# false-positive mismatches caused by differing computation paths.
# `setval` on sequences is a no-op when MAX(id) equals current value.
echo
echo ">>> finalize truth (parity with merge's finalize)"
"$DB_MERGE_BIN" finalize --target-url "$URL_TRUTH"

echo
echo "=========================================================="
echo "DIFF: original truth (5432) vs merged (5436)"
echo "=========================================================="
# Convention: --left = ORIGINAL (truth), --right = MERGED — matches
# the ROWS_ORIGINAL / ROWS_MERGED column headers in the diff output.
"$DB_MERGE_BIN" diff --left "$URL_TRUTH" --right "$URL_MERGE"
