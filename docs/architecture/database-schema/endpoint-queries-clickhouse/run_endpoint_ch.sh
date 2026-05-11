#!/usr/bin/env bash
#
# run_endpoint_ch.sh — run any of the 23 CH endpoint-queries SQL files
# against the local Docker ClickHouse (canonical ADR 0044 schema applied
# by the `db-clickhouse-init` sidecar).
#
# Assumes:
#   • the CH container is up (`docker compose up -d clickhouse db-clickhouse-init`)
#   • the schema is applied (the sidecar exits 0 after the init.sql)
#   • the tables are populated (otherwise discovery returns empty and
#     the corresponding endpoint is reported and skipped)
#
# Each `case` branch corresponds to exactly one .sql file in this directory.
# Discovery for required inputs (account StrKey, contract StrKey, tx hash,
# pool id, surrogate ids, etc.) is done up-front against the live CH so the
# script picks real values without you having to look them up by hand.
#
# Mirrors `endpoint-queries/run_endpoint.sh` (PG) in structure and CLI so
# reviewers can `./run_endpoint.sh 03 | tee pg.out && ./run_endpoint_ch.sh 03 | tee ch.out`
# and diff side-by-side.

set -uo pipefail
# `-e` is intentionally NOT set — `all` mode is allowed to soldier on past
# individual endpoint failures so you see the full picture in one run.

# =============================================================================
# Config (override via env)
# =============================================================================
CH_USER="${SBE_CH_USER:-default}"
CH_PASS="${SBE_CH_PASS:-clickhouse}"
CH_DB="${SBE_CH_DB:-default}"
QUERY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Use `docker compose exec` against the repo's compose file. The CH service
# is named `clickhouse` (see docker-compose.yml).
COMPOSE_FILE="${SBE_COMPOSE_FILE:-$(git -C "$QUERY_DIR" rev-parse --show-toplevel 2>/dev/null)/docker-compose.yml}"
COMPOSE_SERVICE="${SBE_CH_SERVICE:-clickhouse}"

EXPLAIN_PREFIX=""
SYNTAX_ONLY=0

# =============================================================================
# Help
# =============================================================================
usage() {
    local me; me=$(basename "$0")
    cat <<EOF
Usage: $me <id> [--explain] [--syntax-only]

Run one of the 23 CH endpoint-queries SQL files against the local Docker
ClickHouse (canonical ADR 0044 schema).

IDs:
  01..23     run a single endpoint
  all        run every endpoint in sequence

Flags:
  --explain        wrap the reference query in EXPLAIN PLAN actions=1
  --syntax-only    skip discovery + execution; parse-check the SQL via
                   \`clickhouse-client --query=... --format=Null\`. Tier 1
                   validation per task 0207.
  -h, --help

Env:
  SBE_COMPOSE_FILE  default: <repo-root>/docker-compose.yml
  SBE_CH_SERVICE    default: clickhouse
  SBE_CH_USER       default: default
  SBE_CH_PASS       default: clickhouse
  SBE_CH_DB         default: default

Examples:
  ./$me 01                      # network stats
  ./$me 04                      # ledgers list (50 newest)
  ./$me 03 --explain            # /transactions/:hash with EXPLAIN
  ./$me all --syntax-only       # Tier 1 parse-check every endpoint
  ./$me all                     # smoke-run every endpoint
EOF
}

# =============================================================================
# Argument parsing
# =============================================================================
ID=""
for arg in "$@"; do
    case "$arg" in
        --explain) EXPLAIN_PREFIX="EXPLAIN PLAN actions=1 " ;;
        --syntax-only) SYNTAX_ONLY=1 ;;
        -h|--help) usage; exit 0 ;;
        *)
            if [[ -z "$ID" ]]; then ID="$arg"
            else echo "unexpected arg: $arg" >&2; usage; exit 1
            fi
            ;;
    esac
done
[[ -z "$ID" ]] && { usage; exit 1; }

# =============================================================================
# clickhouse-client wrappers
# =============================================================================
ch_query() {
    # Execute a SQL string against CH; output goes to caller's stdout.
    # Under SYNTAX_ONLY=1 we route through ch_parse_only so the per-ID
    # case statement's parameter bindings drive Tier 1 directly — no
    # separate ad-hoc substitution.
    local sql="$1"
    if [[ "$SYNTAX_ONLY" == "1" ]]; then
        if ch_parse_only "$sql" 2>/dev/null; then
            echo "  [OK]   parses"
            return 0
        else
            echo "  [FAIL] parse error:"
            ch_parse_only "$sql" 2>&1 | head -5 | sed 's/^/         /'
            return 1
        fi
    fi
    docker compose -f "$COMPOSE_FILE" exec -T "$COMPOSE_SERVICE" \
        clickhouse-client \
        --user="$CH_USER" --password="$CH_PASS" --database="$CH_DB" \
        --query="$sql"
}

ch_oneshot() {
    # One-shot query, value-only output (tab-separated, single value).
    docker compose -f "$COMPOSE_FILE" exec -T "$COMPOSE_SERVICE" \
        clickhouse-client \
        --user="$CH_USER" --password="$CH_PASS" --database="$CH_DB" \
        --format=TabSeparatedRaw --query="$1"
}

ch_parse_only() {
    # Tier 1 — parse the SQL against the live schema; format=Null discards
    # results, only the planner runs. Exit 0 = parses.
    local sql="$1"
    docker compose -f "$COMPOSE_FILE" exec -T "$COMPOSE_SERVICE" \
        clickhouse-client \
        --user="$CH_USER" --password="$CH_PASS" --database="$CH_DB" \
        --format=Null --query="$sql"
}

# =============================================================================
# SQL file helpers
# =============================================================================
# get_statement <file> <n> — print the n-th statement (1-indexed) from a
# multi-statement file split on `-- @@ split @@`. For single-statement files
# n=1 returns the whole file.
get_statement() {
    local file="$1" n="$2"
    awk -v n="$n" '
        BEGIN { stmt = 1 }
        /^-- @@ split @@/ { stmt++; next }
        { if (stmt == n) print }
    ' "$file"
}

# substitute_params <sql> <p1> <p2> ... — replace $1, $2, ... in the SQL with
# the listed values. Iterates from highest index down so $10 is replaced
# before $1.
substitute_params() {
    local sql="$1"; shift
    local args=("$@")
    local i
    for ((i=${#args[@]}; i>=1; i--)); do
        local val="${args[$((i-1))]}"
        sql="${sql//\$$i/$val}"
    done
    printf '%s' "$sql"
}

# wrap a SQL statement in EXPLAIN iff --explain was passed
explain_wrap() {
    local sql="$1"
    if [[ -n "$EXPLAIN_PREFIX" ]]; then
        printf '%s%s' "$EXPLAIN_PREFIX" "$sql"
    else
        printf '%s' "$sql"
    fi
}

require_value() {
    local v="$1" label="$2"
    if [[ -z "$v" ]]; then
        echo "  SKIP: $label is empty in CH; populate the table first." >&2
        return 1
    fi
    return 0
}

# =============================================================================
# Tier 1 parse-check note
# =============================================================================
# When SYNTAX_ONLY=1, ch_query above routes through ch_parse_only
# (`--format=Null`), so the per-ID case statement's parameter bindings drive
# the parser without materialising results. This depends on discovery
# oneshots returning real values; on an empty CH the per-ID case logs
# "SKIP" and Tier 1 cannot run for that endpoint. Populate CH via
# `cargo run -p backfill-runner -- --target clickhouse --start S --end E`
# before relying on `--syntax-only` as a CI gate.

# =============================================================================
# Per-endpoint runners
# =============================================================================
run_one() {
    local id="$1"
    local FILE STMT SUB

    # Resolve file path from id (zero-padded).
    local nn; nn=$(printf '%02d' "$((10#$id))" 2>/dev/null || echo "$id")
    FILE=$(ls "$QUERY_DIR"/${nn}_*.sql 2>/dev/null | head -1)
    if [[ -z "$FILE" ]]; then
        echo "unknown id: $id (no ${nn}_*.sql in $QUERY_DIR)" >&2
        return 1
    fi

    if [[ "$SYNTAX_ONLY" == "1" ]]; then
        echo "=== Tier 1 parse: $(basename "$FILE") ==="
        # Falls through to the case statement; ch_query is rewired
        # to route through ch_parse_only (--format=Null).
    fi

    case "$id" in
    01) echo "=== E01: GET /network/stats ==="
        explain_wrap "$(<"$FILE")" | ch_query "$(cat)" ;;

    02) echo "=== E02: GET /transactions (statement A — no filter) ==="
        # Cursor params: $1 = limit, $2 = cursor_ledger, $3 = cursor_app_order, $4 = cursor_id
        STMT=$(get_statement "$FILE" 1)
        SUB=$(substitute_params "$STMT" "50" "NULL" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    03) echo "=== E03: GET /transactions/:hash ==="
        local hex
        hex=$(ch_oneshot "SELECT lower(hex(hash)) FROM transaction_hash_index FINAL ORDER BY ledger_sequence DESC LIMIT 1")
        require_value "$hex" "transaction_hash_index" || return 1
        echo "  hash = $hex"
        # 6 statements: A (dictGet), B (header), C (ops), D (participants), E (events), F (invocations)
        # All threaded via the resolved ledger_sequence + transaction_id.
        for stmt_idx in 1 2 3 4 5 6; do
            STMT=$(get_statement "$FILE" "$stmt_idx")
            SUB=$(substitute_params "$STMT" "unhex('$hex')")
            echo "--- statement $stmt_idx ---"
            explain_wrap "$SUB" | ch_query "$(cat)" || true
        done ;;

    04) echo "=== E04: GET /ledgers ==="
        SUB=$(substitute_params "$(<"$FILE")" "50" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    05) echo "=== E05: GET /ledgers/:sequence ==="
        local seq
        seq=$(ch_oneshot "SELECT max(sequence) FROM ledgers")
        require_value "$seq" "ledgers" || return 1
        echo "  sequence = $seq"
        for stmt_idx in 1 2; do
            STMT=$(get_statement "$FILE" "$stmt_idx")
            SUB=$(substitute_params "$STMT" "$seq")
            echo "--- statement $stmt_idx ---"
            explain_wrap "$SUB" | ch_query "$(cat)" || true
        done ;;

    06) echo "=== E06: GET /accounts/:account_id ==="
        local strkey
        strkey=$(ch_oneshot "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$strkey" "accounts" || return 1
        echo "  account = $strkey"
        for stmt_idx in 1 2; do
            STMT=$(get_statement "$FILE" "$stmt_idx")
            SUB=$(substitute_params "$STMT" "'$strkey'")
            echo "--- statement $stmt_idx ---"
            explain_wrap "$SUB" | ch_query "$(cat)" || true
        done ;;

    07) echo "=== E07: GET /accounts/:account_id/transactions ==="
        local strkey
        strkey=$(ch_oneshot "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$strkey" "accounts" || return 1
        echo "  account = $strkey"
        SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    08) echo "=== E08: GET /assets ==="
        SUB=$(substitute_params "$(<"$FILE")" "50" "NULL" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    09) echo "=== E09: GET /assets/:id ==="
        local aid
        aid=$(ch_oneshot "SELECT id FROM assets FINAL ORDER BY id DESC LIMIT 1")
        require_value "$aid" "assets" || return 1
        echo "  asset id = $aid"
        SUB=$(substitute_params "$(<"$FILE")" "$aid")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    10) echo "=== E10: GET /assets/:id/transactions ==="
        local aid
        aid=$(ch_oneshot "SELECT id FROM assets FINAL ORDER BY id DESC LIMIT 1")
        require_value "$aid" "assets" || return 1
        echo "  asset id = $aid"
        SUB=$(substitute_params "$(<"$FILE")" "$aid" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    11) echo "=== E11: GET /contracts/:contract_id ==="
        local strkey
        strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$strkey" "soroban_contracts" || return 1
        echo "  contract = $strkey"
        for stmt_idx in 1 2; do
            STMT=$(get_statement "$FILE" "$stmt_idx")
            SUB=$(substitute_params "$STMT" "'$strkey'" "7")
            echo "--- statement $stmt_idx ---"
            explain_wrap "$SUB" | ch_query "$(cat)" || true
        done ;;

    12) echo "=== E12: GET /contracts/:contract_id/interface ==="
        local strkey
        strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$strkey" "soroban_contracts" || return 1
        echo "  contract = $strkey"
        SUB=$(substitute_params "$(<"$FILE")" "'$strkey'")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    13) echo "=== E13: GET /contracts/:contract_id/invocations ==="
        local strkey
        strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$strkey" "soroban_contracts" || return 1
        echo "  contract = $strkey"
        SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    14) echo "=== E14: GET /contracts/:contract_id/events ==="
        local strkey
        strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$strkey" "soroban_contracts" || return 1
        echo "  contract = $strkey"
        SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    15) echo "=== E15: GET /nfts ==="
        SUB=$(substitute_params "$(<"$FILE")" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    16) echo "=== E16: GET /nfts/:id ==="
        local nid
        nid=$(ch_oneshot "SELECT id FROM nfts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$nid" "nfts" || return 1
        echo "  nft id = $nid"
        SUB=$(substitute_params "$(<"$FILE")" "$nid")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    17) echo "=== E17: GET /nfts/:id/transfers ==="
        local nid
        nid=$(ch_oneshot "SELECT id FROM nfts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$nid" "nfts" || return 1
        echo "  nft id = $nid"
        SUB=$(substitute_params "$(<"$FILE")" "$nid" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    18) echo "=== E18: GET /liquidity-pools ==="
        SUB=$(substitute_params "$(<"$FILE")" "50" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    19) echo "=== E19: GET /liquidity-pools/:id ==="
        local pool_hex
        pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY created_at_ledger DESC LIMIT 1")
        require_value "$pool_hex" "liquidity_pools" || return 1
        echo "  pool = $pool_hex"
        SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    20) echo "=== E20: GET /liquidity-pools/:id/transactions ==="
        local pool_hex
        pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY created_at_ledger DESC LIMIT 1")
        require_value "$pool_hex" "liquidity_pools" || return 1
        echo "  pool = $pool_hex"
        SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')" "50" "NULL" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    21) echo "=== E21: GET /liquidity-pools/:id/chart ==="
        local pool_hex
        pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY created_at_ledger DESC LIMIT 1")
        require_value "$pool_hex" "liquidity_pools" || return 1
        echo "  pool = $pool_hex (range: last 30 days)"
        SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')" "30")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    22) echo "=== E22: GET /search ==="
        local prefix
        prefix=$(ch_oneshot "SELECT substring(account_id, 1, 4) FROM accounts FINAL ORDER BY id DESC LIMIT 1")
        require_value "$prefix" "accounts" || return 1
        echo "  query prefix = $prefix"
        SUB=$(substitute_params "$(<"$FILE")" "'$prefix'" "10")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    23) echo "=== E23: GET /liquidity-pools/:id/participants ==="
        local pool_hex
        pool_hex=$(ch_oneshot "SELECT lower(hex(lp.pool_id)) FROM liquidity_pools lp JOIN lp_positions FINAL p ON p.pool_id = lp.pool_id WHERE p.shares > 0 ORDER BY lp.created_at_ledger DESC LIMIT 1")
        require_value "$pool_hex" "lp_positions (with shares > 0)" || return 1
        echo "  pool = $pool_hex"
        SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')" "50" "NULL")
        explain_wrap "$SUB" | ch_query "$(cat)" ;;

    *)
        echo "unknown id: $id" >&2
        return 1 ;;
    esac
}

# =============================================================================
# Dispatch
# =============================================================================
if [[ "$ID" == "all" ]]; then
    for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20 21 22 23; do
        echo
        run_one "$i" || echo "  -> failed (id=$i)"
    done
else
    run_one "$ID"
fi
