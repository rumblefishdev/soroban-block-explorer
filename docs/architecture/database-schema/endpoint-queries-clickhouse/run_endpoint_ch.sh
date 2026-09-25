#!/usr/bin/env bash
#
# run_endpoint_ch.sh — run the CH endpoint-queries SQL files (one per public
# REST endpoint, `NN_get_*.sql` in this directory) against the local Docker
# ClickHouse (canonical ADR 0044 schema applied by the `db-clickhouse-init`
# sidecar).
#
# Assumes:
#   • the CH container is up (`docker compose up -d clickhouse db-clickhouse-init`)
#   • the schema is applied (the sidecar exits 0 after the init.sql)
#   • for execution mode: tables are populated (otherwise discovery
#     returns empty and the endpoint is reported as SKIP)
#
# Two modes:
#   • Default (execution): discover real input values from CH, thread
#     them between multi-statement endpoints, execute, print results.
#   • `--syntax-only`: substitute type-correct dummy literals, run each
#     statement through `--format=Null`. Does NOT need live data — works
#     against an empty `db-clickhouse-init`-applied schema, and writes
#     nothing. This is the Tier 1 CI gate (.github/workflows/ci.yml).
#
# Tier 1 gate guarantees (syntax-only):
#   • every statement of every file is checked — an arm that skips one, or a
#     file whose `;`-terminated statements are not split with `-- @@ split @@`,
#     FAILS;
#   • an unsubstituted placeholder (`$N`, `:name`, `{name}`) FAILS with its
#     name, before CH is asked;
#   • `all` walks every `NN_*.sql` file present (a file without a runner arm
#     FAILS), prints a summary and exits non-zero on any failure.
#
# Mirrors `endpoint-queries/run_endpoint.sh` (PG) in structure and CLI
# so reviewers can `./run_endpoint.sh 03 | tee pg.out
# && ./run_endpoint_ch.sh 03 | tee ch.out` and diff side-by-side.

set -uo pipefail
# `-e` is intentionally NOT set — the runner soldiers on past individual
# statement failures so you see the full picture in one run; the exit code
# is computed from the failure counter instead.

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

# Counters (all arms, all files).
CHECKS=0          # statements sent (or refused by the placeholder guard)
FAILS=0           # of which failed
SEEN=" "          # statement indices exercised for the current file

# =============================================================================
# Help
# =============================================================================
usage() {
    local me; me=$(basename "$0")
    cat <<EOF
Usage: $me <id> [--explain] [--syntax-only]

Run one of the CH endpoint-queries SQL files (NN_get_*.sql) against the local
Docker ClickHouse (canonical ADR 0044 schema).

IDs:
  01..24     run a single endpoint (20 was retired, superseded by 24)
  all        run every NN_*.sql file present, in order

Flags:
  --explain        wrap each statement in EXPLAIN PLAN actions=1
  --syntax-only    Tier 1 CI gate. Substitute type-correct dummy literals
                   per endpoint and route through \`--format=Null\`. No
                   discovery against live data; works on an empty schema.
                   Exits non-zero if any statement fails.
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
  ./$me all --syntax-only       # Tier 1 parse-check every endpoint (CI)
  ./$me all                     # smoke-run every endpoint w/ live data
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
ch_exec() {
    # Execute a SQL string against CH. In syntax-only mode, route through
    # ch_parse_only (--format=Null) and emit a one-line OK/FAIL marker.
    # In execution mode, stream results to caller's stdout.
    local sql="$1"
    if [[ "$SYNTAX_ONLY" == "1" ]]; then
        local out
        if out=$(ch_parse_only "$sql" 2>&1 >/dev/null); then
            echo "  [OK]   parses"
            return 0
        else
            echo "  [FAIL] parse error:"
            printf '%s\n' "$out" | head -5 | sed 's/^/         /'
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
    # Always runs against live data — DO NOT call from syntax-only paths.
    docker compose -f "$COMPOSE_FILE" exec -T "$COMPOSE_SERVICE" \
        clickhouse-client \
        --user="$CH_USER" --password="$CH_PASS" --database="$CH_DB" \
        --format=TabSeparatedRaw --query="$1"
}

ch_parse_only() {
    # Tier 1 — run the SQL against the live schema; format=Null discards
    # results. Against the empty schema this resolves every table, column,
    # function and type without reading data. Exit 0 = parses.
    local sql="$1"
    docker compose -f "$COMPOSE_FILE" exec -T "$COMPOSE_SERVICE" \
        clickhouse-client \
        --user="$CH_USER" --password="$CH_PASS" --database="$CH_DB" \
        --format=Null --query="$sql"
}

# =============================================================================
# SQL helpers
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

# statement_count <file> — number of `-- @@ split @@`-separated statements.
statement_count() {
    echo $(( $(grep -c '^-- @@ split @@' "$1") + 1 ))
}

# terminated_count <file> — number of `;` outside `--` comments. Each
# statement ends in exactly one, so this must equal statement_count; a
# mismatch means two statements share one split section (the runner would
# send them to CH as one multi-query and only half-check them).
terminated_count() {
    sed 's/--.*$//' "$1" | tr -cd ';' | wc -c | tr -d ' '
}

# substitute_params <sql> <p1> <p2> ... — replace $1, $2, ... in the SQL
# with the listed values. Iterates from highest index down so `$10` is
# replaced before `$1` (avoids partial matches).
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

# substitute_fragments <sql> <name>=<value> ... — replace `{name}` with the
# value. For runtime `format!` fragments (21) that Rust interpolates rather
# than binds.
substitute_fragments() {
    local sql="$1"; shift
    local kv
    for kv in "$@"; do
        sql="${sql//\{${kv%%=*}\}/${kv#*=}}"
    done
    printf '%s' "$sql"
}

# leftover_placeholders <sql> — print any placeholder the arm failed to
# substitute (`$N`, `$name`, `:name`, `{name}`), comments stripped.
leftover_placeholders() {
    printf '%s\n' "$1" | sed 's/--.*$//' \
        | grep -oE '\$[A-Za-z0-9_]+|(^|[^:[:alnum:]_])\:[a-z_]+|\{[a-z_]+\}' \
        | sed 's/^[^$:{]//' | sort -u | tr '\n' ' '
}

# explain_wrap <sql> — prepend `EXPLAIN PLAN actions=1` iff --explain set.
explain_wrap() {
    local sql="$1"
    if [[ -n "$EXPLAIN_PREFIX" ]]; then
        printf '%s%s' "$EXPLAIN_PREFIX" "$sql"
    else
        printf '%s' "$sql"
    fi
}

# check <n> <label> <sql> — run statement n (already substituted), count it,
# record it as exercised. Returns the statement's status.
check() {
    local n="$1" label="$2" sql="$3"
    SEEN="$SEEN$n "
    CHECKS=$((CHECKS + 1))
    if [[ -n "$label" ]]; then
        echo "--- statement $n ($label) ---"
    else
        echo "--- statement $n ---"
    fi
    local left; left=$(leftover_placeholders "$sql")
    if [[ -n "$left" ]]; then
        echo "  [FAIL] unsubstituted placeholder(s): $left— the runner arm does not supply them"
        FAILS=$((FAILS + 1))
        return 1
    fi
    if ! ch_exec "$(explain_wrap "$sql")"; then
        FAILS=$((FAILS + 1))
        return 1
    fi
    return 0
}

# stmt <file> <n> [params...] — statement n of file with $1.. substituted.
stmt() {
    local file="$1" n="$2"; shift 2
    substitute_params "$(get_statement "$file" "$n")" "$@"
}

# run_all_stmts <file> [params...] — check every statement of the file with
# one shared positional parameter list (the files number their inputs
# file-wide, so one list fits every statement).
run_all_stmts() {
    local file="$1"; shift
    local n total; total=$(statement_count "$file")
    for ((n=1; n<=total; n++)); do
        check "$n" "" "$(stmt "$file" "$n" "$@")"
    done
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
# Dummy literals for `--syntax-only` mode
# =============================================================================
# Hardcoded type-correct dummy values per discovery slot. These make
# Tier 1 parse-check independent of CH being populated — the planner
# only needs concrete types, not real rows.
DUMMY_HASH_HEX="0000000000000000000000000000000000000000000000000000000000000000"
DUMMY_POOL_HEX="$DUMMY_HASH_HEX"
DUMMY_STRKEY_G="GAAA"
DUMMY_STRKEY_C="CAAA"
DUMMY_ID_I64="1234567890123"
DUMMY_LEDGER_SEQ="12345"
DUMMY_FROM_MS="1767225600000"   # 2026-01-01T00:00:00Z
DUMMY_TO_MS="1767830400000"     # 2026-01-08T00:00:00Z

# 21 reads the prices tenant's `prices.price_usd_series*` views, which live
# in the same production cluster but are NOT part of this repo's schema
# (crates/db-clickhouse/schema/init.sql) — a CH bootstrapped from here has
# no `prices` database (the api decode_smoke test skips on the same probe).
# The gate therefore substitutes `{series_view}` with an inline, empty,
# read-only stand-in carrying the columns 21 reads, typed per the prices
# interop contract (asset_kind/asset_code/issuer_address String, bucket a
# grain-floored DateTime, close_usd Decimal). This checks everything in 21
# except the view's name and column types, which the prices repo owns; the
# real view name is printed next to each check so it stays visible.
PRICES_SERIES_STUB="(SELECT '' AS asset_kind, '' AS asset_code, '' AS issuer_address, toDateTime(0, 'UTC') AS bucket, toDecimal128(0, 18) AS close_usd WHERE 0)"

# =============================================================================
# Per-endpoint runners
# =============================================================================
run_one() {
    local id="$1"
    local FILE
    FILE=$(ls "$QUERY_DIR"/${id}_*.sql 2>/dev/null | head -1)
    if [[ -z "$FILE" ]]; then
        echo "unknown id: $id (no ${id}_*.sql in $QUERY_DIR)" >&2
        return 1
    fi
    SEEN=" "
    local fails_before=$FAILS
    echo "=== $(basename "$FILE") ==="

    case "$id" in
    01)
        # Params: $1=head (chain head; the Rust query inlines it via format!).
        local head
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            head="$DUMMY_LEDGER_SEQ"
        else
            head=$(ch_oneshot "SELECT max(sequence) FROM ledgers")
            require_value "$head" "ledgers" || return 2
        fi
        run_all_stmts "$FILE" "$head"
        ;;

    02)
        # 5 statements, file-wide params: $1=limit, $2=cursor_ledger,
        # $3=cursor_app_order, $4=source_id, $5=contract_id, $6=op_type,
        # $7=latest_partition (PR #175: intDiv(max_ledger, 500000) bounds the
        # unfiltered scan to one partition). Filters get typed dummies so the
        # filtered statements (B contract, C/D op-type) plan their real shape.
        local latest_part
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            latest_part="124"  # intDiv(62016000, 500000)
        else
            latest_part=$(ch_oneshot "SELECT intDiv(max(ledger_sequence), 500000) FROM transactions")
            require_value "$latest_part" "transactions" || return 2
            echo "  latest_partition = $latest_part"
        fi
        run_all_stmts "$FILE" "50" "NULL" "NULL" "NULL" "$DUMMY_ID_I64" "1" "$latest_part"
        ;;

    03)
        # 6 statements; A=hash index seek (hash→ledger_sequence), B=header,
        # C=ops, D=participants, E=events (full payload §5.1), F=invocations.
        # All take $1=hash (FixedString(32) — pass as unhex(hex)).
        local hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            hex="$DUMMY_HASH_HEX"
        else
            hex=$(ch_oneshot "SELECT lower(hex(hash)) FROM transactions WHERE ledger_sequence = (SELECT max(ledger_sequence) FROM transactions) LIMIT 1")
            require_value "$hex" "transactions" || return 2
            echo "  hash = $hex"
        fi
        run_all_stmts "$FILE" "unhex('$hex')"
        ;;

    04)
        # Stmt A params: $1=limit, $2=cursor_closed_at, $3=cursor_sequence.
        # Stmt B params: $1=sequence list, $2=partition list (task 0445).
        check 1 "page" "$(stmt "$FILE" 1 "50" "NULL" "NULL")"
        check 2 "aggregates" "$(stmt "$FILE" 2 "$DUMMY_LEDGER_SEQ" "$((DUMMY_LEDGER_SEQ / 500000))")"
        ;;

    05)
        # Stmt A: $1=sequence. Stmt B: $1=sequence, $2=cursor_lseq,
        # $3=cursor_id, $4=limit. Stmt C (task 0445): $1=sequence list,
        # $2=partition list.
        local seq
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            seq="$DUMMY_LEDGER_SEQ"
        else
            seq=$(ch_oneshot "SELECT max(sequence) FROM ledgers")
            require_value "$seq" "ledgers" || return 2
            echo "  sequence = $seq"
        fi
        check 1 "header" "$(stmt "$FILE" 1 "$seq")"
        check 2 "transactions" "$(stmt "$FILE" 2 "$seq" "NULL" "NULL" "50")"
        check 3 "aggregates" "$(stmt "$FILE" 3 "$seq" "$((seq / 500000))")"
        ;;

    06)
        # 3 statements. A: $1 = StrKey. B: $1 = Int64 account id from A.
        # C: $1 = account id, $2 = native asset surrogate (bound from Rust).
        local strkey acc_id native_id
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_G"
            acc_id="$DUMMY_ID_I64"
            native_id="$DUMMY_ID_I64"
        else
            strkey=$(ch_oneshot "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "accounts" || return 2
            echo "  account = $strkey"
            acc_id=$(ch_oneshot "SELECT id FROM accounts FINAL WHERE account_id = '$strkey' LIMIT 1")
            require_value "$acc_id" "accounts.id" || return 2
            echo "  id = $acc_id"
            native_id=$(ch_oneshot "SELECT id FROM assets WHERE asset_type = 0 LIMIT 1")
            require_value "$native_id" "assets (native row)" || return 2
        fi
        check 1 "header" "$(stmt "$FILE" 1 "'$strkey'")"
        check 2 "balances" "$(stmt "$FILE" 2 "$acc_id")"
        check 3 "deleted flag" "$(stmt "$FILE" 3 "$acc_id" "$native_id")"
        ;;

    07)
        # Params: $1=account_strkey, $2=limit, $3=cursor_ledger,
        # $4=cursor_app_order. B/C take example literal IN lists.
        local strkey
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_G"
        else
            strkey=$(ch_oneshot "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "accounts" || return 2
            echo "  account = $strkey"
        fi
        run_all_stmts "$FILE" "'$strkey'" "50" "NULL" "NULL"
        ;;

    08)
        # Two-phase list (task 0364). A: $1=limit, $2=asset_type_filter,
        # $3=asset_code_filter, $4/$5=cursor (holder_rank, id), $6=sac_only.
        # B–D take example literal IN lists. A is checked twice: first page
        # unfiltered, then every filter + the cursor bound, so both branches
        # of each `$N IS NULL OR …` gate are type-checked.
        check 1 "seek, first page" "$(stmt "$FILE" 1 "50" "NULL" "NULL" "NULL" "NULL" "0")"
        check 1 "seek, filtered + cursor" "$(stmt "$FILE" 1 "50" "1" "'usdc'" "100" "$DUMMY_ID_I64" "1")"
        check 2 "hydrate" "$(stmt "$FILE" 2)"
        check 3 "contract context" "$(stmt "$FILE" 3)"
        check 4 "page issuers" "$(stmt "$FILE" 4)"
        ;;

    09)
        # File-wide params: $1=issuer StrKey, $2=asset_code, $3=issuer_id
        # (from A), $4=issuer_id_key (from D). C–E take example literals.
        local iss_strkey acode iss_id
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            iss_strkey="$DUMMY_STRKEY_G"; acode="USDC"; iss_id="$DUMMY_ID_I64"
        else
            local row
            row=$(ch_oneshot "SELECT a.asset_code || '|' || toString(a.issuer_id) FROM assets a WHERE a.asset_type = 1 LIMIT 1")
            require_value "$row" "assets (classic credit)" || return 2
            acode="${row%%|*}"; iss_id="${row#*|}"
            iss_strkey=$(ch_oneshot "SELECT account_id FROM accounts WHERE id = $iss_id LIMIT 1")
            require_value "$iss_strkey" "accounts (issuer $iss_id)" || return 2
            echo "  asset = $acode-$iss_strkey"
        fi
        run_all_stmts "$FILE" "'$iss_strkey'" "'$acode'" "$iss_id" "$iss_id"
        ;;

    10)
        # Params: $1=asset_id (ids::asset_id surrogate), $2=limit,
        # $3=cursor_ledger, $4=cursor_app_order. B/C take example literals.
        local aid
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            aid="$DUMMY_ID_I64"
        else
            aid=$(ch_oneshot "SELECT id FROM assets WHERE id != 0 LIMIT 1")
            require_value "$aid" "assets" || return 2
            echo "  asset_id = $aid"
        fi
        run_all_stmts "$FILE" "$aid" "50" "NULL" "NULL"
        ;;

    11)
        # 3 statements. A: $1 = StrKey. B: $1 = Int64 contract id from A,
        # $2 = window_days. C (task 0441): $3 = contract id list.
        local strkey ctr_id
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"
            ctr_id="$DUMMY_ID_I64"
        else
            strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "soroban_contracts" || return 2
            echo "  contract = $strkey"
            ctr_id=$(ch_oneshot "SELECT id FROM soroban_contracts FINAL WHERE contract_id = '$strkey' LIMIT 1")
            require_value "$ctr_id" "soroban_contracts.id" || return 2
            echo "  id = $ctr_id"
        fi
        check 1 "header" "$(stmt "$FILE" 1 "'$strkey'")"
        check 2 "stats, window=7d" "$(stmt "$FILE" 2 "$ctr_id" "7")"
        check 3 "mirrored asset" "$(stmt "$FILE" 3 "" "" "$ctr_id")"
        ;;

    12|13|14)
        # 12: $1=contract_strkey.
        # 13: $1=contract_strkey, $2=limit, $3=cursor_ledger, $4=cursor_tx_id.
        # 14: $1=contract_strkey, $2=limit, $3..$6=cursor (ledger,
        #     transaction_index, operation_index, event_index).
        local strkey
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"
        else
            strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "soroban_contracts" || return 2
            echo "  contract = $strkey"
        fi
        run_all_stmts "$FILE" "'$strkey'" "50" "NULL" "NULL" "NULL" "NULL"
        ;;

    15)
        # PR #175: nfts dropped surrogate id; cursor is (contract_id, token_id) tuple.
        # Params: $1=limit, $2=cursor_contract_id, $3=cursor_token_id,
        #         $4=collection_name, $5=contract_strkey_filter, $6=name_filter.
        run_all_stmts "$FILE" "50" "NULL" "NULL" "NULL" "NULL" "NULL"
        ;;

    16|17)
        # 16: $1=contract_strkey, $2=token_id.
        # 17: + $3=limit, $4=cursor_ledger, $5=cursor_event_order.
        local strkey tokid
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"; tokid="'1'"
        else
            local row
            row=$(ch_oneshot "SELECT sc.contract_id || '|' || n.token_id FROM nfts n FINAL JOIN soroban_contracts sc FINAL ON sc.id = n.contract_id LIMIT 1")
            require_value "$row" "nfts" || return 2
            strkey="${row%%|*}"
            tokid="'${row#*|}'"
            echo "  nft (contract_strkey, token_id) = ($strkey, $tokid)"
        fi
        run_all_stmts "$FILE" "'$strkey'" "$tokid" "50" "NULL" "NULL"
        ;;

    18)
        # Params: $1=limit, $2=cursor_activity_ledger, $3=cursor_pool_id,
        # $4=asset_code filter, $5=pool_kind filter.
        run_all_stmts "$FILE" "50" "NULL" "NULL" "NULL" "NULL"
        ;;

    19|23)
        # 19: $1=pool_id (FixedString(32) — pass as unhex(hex)).
        # 23: + $2=limit, $3=cursor_shares, $4=cursor_account_id.
        local pool_hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "liquidity_pools" || return 2
            echo "  pool = $pool_hex"
        fi
        run_all_stmts "$FILE" "unhex('$pool_hex')" "50" "NULL" "NULL"
        ;;

    21)
        # Bound: $1=pool_id hex, $2=from_ms, $3=to_ms, $4..$6 leg A
        # (kind, code, issuer), $7..$9 leg B. Format fragments per interval —
        # the SAME table as get_pool_chart.rs `fetch_pool_chart` (and the 21
        # header); keep the three in step:
        #   1h → bucket_fn=toStartOfHour price_bucket_fn=toStartOfHour series_view=prices.price_usd_series_1h
        #   1d → bucket_fn=toStartOfDay  price_bucket_fn=toStartOfDay  series_view=prices.price_usd_series
        #   1w → bucket_fn=toMonday      price_bucket_fn=toStartOfDay  series_view=prices.price_usd_series
        #   carry = MAX_PRICE_CARRY_SECONDS = 172800
        # `series_view` is replaced by PRICES_SERIES_STUB (see its comment).
        local pool_hex from_ms to_ms
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"; from_ms="$DUMMY_FROM_MS"; to_ms="$DUMMY_TO_MS"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "liquidity_pools" || return 2
            to_ms=$(ch_oneshot "SELECT toUnixTimestamp64Milli(max(closed_at)) FROM ledgers")
            from_ms=$((to_ms - 7 * 86400 * 1000))
            echo "  pool = $pool_hex (last 7 days, prices stubbed)"
        fi
        local base; base=$(stmt "$FILE" 1 "'$pool_hex'" "$from_ms" "$to_ms" \
            "'native'" "'XLM'" "''" "'credit'" "'USDC'" "'$DUMMY_STRKEY_G'")
        local spec interval bucket_fn price_bucket_fn series_view
        for spec in "1h toStartOfHour toStartOfHour prices.price_usd_series_1h" \
                    "1d toStartOfDay toStartOfDay prices.price_usd_series" \
                    "1w toMonday toStartOfDay prices.price_usd_series"; do
            read -r interval bucket_fn price_bucket_fn series_view <<<"$spec"
            check 1 "interval=$interval, series_view=$series_view (stubbed)" \
                "$(substitute_fragments "$base" \
                    "bucket_fn=$bucket_fn" "price_bucket_fn=$price_bucket_fn" \
                    "series_view=$PRICES_SERIES_STUB" "carry=172800")"
        done
        ;;

    22)
        # File-wide params: $1=q, $2=q_hex, $3=strkey_prefix,
        # $4=per_group_limit, $5=code, $6=issuer, $7=ledger, $8=partition.
        # The contract-name and issuer steps take example literal IN lists.
        local prefix
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            prefix="GAAA"
        else
            prefix=$(ch_oneshot "SELECT substring(account_id, 1, 4) FROM accounts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$prefix" "accounts" || return 2
            echo "  query prefix = $prefix (StrKey path)"
        fi
        run_all_stmts "$FILE" "'$prefix'" "'$DUMMY_HASH_HEX'" "'$prefix'" "10" \
            "'USDC'" "'$DUMMY_STRKEY_G'" "$DUMMY_LEDGER_SEQ" "$((DUMMY_LEDGER_SEQ / 500000))"
        ;;

    24)
        # Step 1: $1=pool_id (64-char hex TEXT). Steps 2/3 take example
        # literal IN lists.
        local pool_hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "liquidity_pools" || return 2
            echo "  pool = $pool_hex"
        fi
        run_all_stmts "$FILE" "'$pool_hex'"
        ;;

    *)
        echo "  [FAIL] no runner arm for $(basename "$FILE") — add one to run_endpoint_ch.sh"
        CHECKS=$((CHECKS + 1)); FAILS=$((FAILS + 1))
        return 1 ;;
    esac

    # Coverage: every split section checked, and every `;` in its own section.
    local total n; total=$(statement_count "$FILE")
    for ((n=1; n<=total; n++)); do
        if [[ "$SEEN" != *" $n "* ]]; then
            echo "  [FAIL] statement $n of $total is not exercised by the runner arm"
            CHECKS=$((CHECKS + 1)); FAILS=$((FAILS + 1))
        fi
    done
    local terminated; terminated=$(terminated_count "$FILE")
    if [[ "$terminated" != "$total" ]]; then
        echo "  [FAIL] $terminated ';'-terminated statements but $total split sections — separate them with '-- @@ split @@'"
        CHECKS=$((CHECKS + 1)); FAILS=$((FAILS + 1))
    fi

    [[ "$FAILS" -eq "$fails_before" ]]
}

# =============================================================================
# Dispatch
# =============================================================================
ids=()
if [[ "$ID" == "all" ]]; then
    for f in "$QUERY_DIR"/[0-9][0-9]_*.sql; do
        b=$(basename "$f"); ids+=("${b%%_*}")
    done
else
    ids=("$(printf '%02d' "$((10#$ID))" 2>/dev/null || echo "$ID")")
fi

failed=(); skipped=()
for i in "${ids[@]}"; do
    [[ "${#ids[@]}" -gt 1 ]] && echo
    run_one "$i"
    case $? in
        0) ;;
        2) skipped+=("$i") ;;
        *) failed+=("$i") ;;
    esac
done

echo
if [[ "$SYNTAX_ONLY" == "1" ]]; then
    echo "Tier 1: $((CHECKS - FAILS)) of $CHECKS checks pass across ${#ids[@]} file(s)."
fi
if [[ "${#skipped[@]}" -gt 0 ]]; then
    echo "Skipped (no data): ${skipped[*]}"
fi
if [[ "${#failed[@]}" -gt 0 ]]; then
    echo "FAILED: ${failed[*]}"
    exit 1
fi
exit 0
