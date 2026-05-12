#!/usr/bin/env bash
#
# compare_pg_ch.sh — Tier 2-4 helper for task 0207: validate that the CH
# endpoint query at `endpoint-queries-clickhouse/NN_get_*.sql` returns data
# semantically equivalent to its PG counterpart at `endpoint-queries/NN_get_*.sql`.
#
# Tiers (per task 0207 README §Validation):
#   Tier 1 — schema parse only (run via `run_endpoint_ch.sh NN --syntax-only`).
#   Tier 2 — row count diff (this script, default).
#   Tier 3 — sample-row diff for 10 random keys (this script, --sample).
#   Tier 4 — aggregate equivalence for aggregating queries (this script, --aggregate).
#
# Pre-conditions:
#   • PG audit container `sbe-audit-postgres-1` is up (port 5432, db
#     `soroban_block_explorer`).
#   • CH canonical container `soroban-block-explorer-clickhouse-1` is up
#     (`docker compose up -d clickhouse db-clickhouse-init`).
#   • CH populated for the ledger range the query touches — run
#     `cargo run -p backfill-runner -- --target clickhouse --start S --end E`
#     against the same range PG indexed.

set -uo pipefail

QUERY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PG_DIR="$(cd "$QUERY_DIR/../endpoint-queries" && pwd)"
COMPOSE_FILE="${SBE_COMPOSE_FILE:-$(git -C "$QUERY_DIR" rev-parse --show-toplevel 2>/dev/null)/docker-compose.yml}"

PG_CONTAINER="${SBE_PG_CONTAINER:-sbe-audit-postgres-1}"
PG_DB="${SBE_PG_DB:-soroban_block_explorer}"
PG_USER="${SBE_PG_USER:-postgres}"

CH_SERVICE="${SBE_CH_SERVICE:-clickhouse}"
CH_USER="${SBE_CH_USER:-default}"
CH_PASS="${SBE_CH_PASS:-clickhouse}"
CH_DB="${SBE_CH_DB:-default}"

MODE="rowcount"  # rowcount | sample | aggregate

usage() {
    cat <<EOF
Usage: compare_pg_ch.sh <id> [--sample | --aggregate]

Run the same endpoint query against PG and CH and report the diff.

  --sample      Tier 3: project 10 random keys, column-by-column compare.
  --aggregate   Tier 4: aggregating queries — compare sums/counts.

Default mode: Tier 2 row count diff.

IDs: 01..23 or 'all'.
EOF
}

ID=""
for arg in "$@"; do
    case "$arg" in
        --sample) MODE="sample" ;;
        --aggregate) MODE="aggregate" ;;
        -h|--help) usage; exit 0 ;;
        *) if [[ -z "$ID" ]]; then ID="$arg"
           else echo "unexpected arg: $arg" >&2; usage; exit 1
           fi ;;
    esac
done
[[ -z "$ID" ]] && { usage; exit 1; }

# Resolve files.
resolve_pair() {
    local id="$1"
    local nn; nn=$(printf '%02d' "$((10#$id))" 2>/dev/null || echo "$id")
    PG_FILE=$(ls "$PG_DIR"/${nn}_*.sql 2>/dev/null | head -1)
    CH_FILE=$(ls "$QUERY_DIR"/${nn}_*.sql 2>/dev/null | head -1)
    if [[ -z "$PG_FILE" || -z "$CH_FILE" ]]; then
        echo "[FAIL] id=$id — missing PG ($PG_FILE) or CH ($CH_FILE) file" >&2
        return 1
    fi
}

pg_count() {
    docker exec -i "$PG_CONTAINER" psql -U "$PG_USER" -d "$PG_DB" \
        -tA -v ON_ERROR_STOP=1 -c "$1"
}

ch_count() {
    docker compose -f "$COMPOSE_FILE" exec -T "$CH_SERVICE" \
        clickhouse-client \
        --user="$CH_USER" --password="$CH_PASS" --database="$CH_DB" \
        --format=TabSeparatedRaw --query="$1"
}

# Wrap the reference query in `SELECT count() FROM (<query>)` for Tier 2.
wrap_count() {
    local query="$1"
    printf 'SELECT count(*) FROM (\n%s\n) sub' "$query"
}

run_pair_rowcount() {
    local id="$1"
    resolve_pair "$id" || return 1

    echo "=== id=$id ==="
    echo "  PG: $(basename "$PG_FILE")"
    echo "  CH: $(basename "$CH_FILE")"
    echo "  --- Tier 2: row count ---"

    # NOTE: Both PG and CH SQL files use the same `$1`/`$2` parameter
    # convention but with different default cursor types. This helper
    # short-circuits with a small per-endpoint shim: see the case
    # statement in `run_endpoint_ch.sh` for canonical parameter binding.
    # For Tier 2, we accept a small inline param set; if discovery fails,
    # the user is asked to populate the relevant table first.

    case "$id" in
    01|04|08|15|18)
        # List queries with no required input — strip trailing semicolon
        # and wrap in count.
        local pg_sql ch_sql
        pg_sql=$(<"$PG_FILE")
        ch_sql=$(<"$CH_FILE")
        # Trim semicolons that break the outer SELECT count() wrapper.
        pg_sql="${pg_sql%;}"
        ch_sql="${ch_sql%;}"
        echo "  TODO: per-endpoint shim — auto-binding for list queries"
        echo "  Use run_endpoint_ch.sh + manual count(*) wrap for now."
        ;;
    *)
        echo "  TODO: per-endpoint shim for id=$id — implement after Phase 4 starts"
        ;;
    esac
}

# =============================================================================
# Dispatch
# =============================================================================
if [[ "$ID" == "all" ]]; then
    for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20 21 22 23; do
        echo
        run_pair_rowcount "$i" || echo "  -> failed (id=$i)"
    done
else
    run_pair_rowcount "$ID"
fi

echo ""
echo "Compare helper is intentionally a scaffold. Tier 2-4 per-endpoint shims"
echo "are implemented during task 0207 Phase 4 once the CH mirror is populated"
echo "with the same ledger range as the PG audit DB. See task 0207 README"
echo "§Phase 4 — Validation E2E for the populate command."
