#!/usr/bin/env bash
#
# run_endpoint_ch.sh — run any of the 23 CH endpoint-queries SQL files
# against the local Docker ClickHouse (canonical ADR 0044 schema applied
# by the `db-clickhouse-init` sidecar).
#
# Assumes:
#   • the CH container is up (`docker compose up -d clickhouse db-clickhouse-init`)
#   • the schema is applied (the sidecar exits 0 after the init.sql)
#   • for execution mode: tables are populated (otherwise discovery
#     returns empty and the endpoint is reported and skipped)
#
# Two modes:
#   • Default (execution): discover real input values from CH, thread
#     them between multi-statement endpoints, execute, print results.
#   • `--syntax-only`: substitute type-correct dummy literals, run each
#     statement through `--format=Null` (planner-only). Does NOT touch
#     live data — works against an empty `db-clickhouse-init`-applied
#     schema. This is the Tier 1 CI gate.
#
# Mirrors `endpoint-queries/run_endpoint.sh` (PG) in structure and CLI
# so reviewers can `./run_endpoint.sh 03 | tee pg.out
# && ./run_endpoint_ch.sh 03 | tee ch.out` and diff side-by-side.

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
  --explain        wrap each statement in EXPLAIN PLAN actions=1
  --syntax-only    Tier 1 CI gate. Substitute type-correct dummy literals
                   per endpoint and route through \`--format=Null\`. No
                   discovery against live data; works on an empty schema.
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
        if ch_parse_only "$sql" >/dev/null 2>&1; then
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
    # Always runs against live data — DO NOT call from syntax-only paths.
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

# explain_wrap <sql> — prepend `EXPLAIN PLAN actions=1` iff --explain set.
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
# Dummy literals for `--syntax-only` mode
# =============================================================================
# Hardcoded type-correct dummy values per discovery slot. These make
# Tier 1 parse-check independent of CH being populated — the planner
# only needs concrete types, not real rows.
#
# Dummy choices (all valid CH literals, none of which need to exist):
#   hash_hex       — 32-byte hex string of zeros
#   strkey         — 56-char G… literal (any valid string works)
#   contract_strkey— 56-char C… literal
#   surrogate_id_i32  — 0 (Int32)
#   surrogate_id_i64  — 0 (Int64)
#   ledger_seq     — 12345 (Int64)
#   pool_hex       — same as hash_hex
DUMMY_HASH_HEX="0000000000000000000000000000000000000000000000000000000000000000"
DUMMY_POOL_HEX="$DUMMY_HASH_HEX"
DUMMY_STRKEY_G="GAAA"
DUMMY_STRKEY_C="CAAA"
DUMMY_ID_I32="0"
DUMMY_ID_I64="0"
DUMMY_LEDGER_SEQ="12345"

# =============================================================================
# Per-endpoint runners
# =============================================================================
run_one() {
    local id="$1"
    local FILE
    local nn; nn=$(printf '%02d' "$((10#$id))" 2>/dev/null || echo "$id")
    FILE=$(ls "$QUERY_DIR"/${nn}_*.sql 2>/dev/null | head -1)
    if [[ -z "$FILE" ]]; then
        echo "unknown id: $id (no ${nn}_*.sql in $QUERY_DIR)" >&2
        return 1
    fi

    if [[ "$SYNTAX_ONLY" == "1" ]]; then
        echo "=== Tier 1 parse: $(basename "$FILE") ==="
    fi

    case "$id" in
    01) [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E01: GET /network/stats ==="
        ch_exec "$(explain_wrap "$(<"$FILE")")" ;;

    02)
        # Statement A — no filter. PR #175 amendment: $7 = latest_partition
        # (intDiv(max_ledger, 500000)) bounds the scan to one partition
        # to avoid full-table FINAL memory blowup.
        # Params: $1=limit, $2=cursor_ledger, $3=cursor_tx_id, $4=source_id,
        # $5=contract_id, $6=op_type, $7=latest_partition.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E02: GET /transactions (statement A) ==="
        local latest_part
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            latest_part="124"  # intDiv(62016000, 500000)
        else
            latest_part=$(ch_oneshot "SELECT intDiv(max(ledger_sequence), 500000) FROM transactions FINAL")
            require_value "$latest_part" "transactions" || return 1
            echo "  latest_partition = $latest_part"
        fi
        local STMT; STMT=$(get_statement "$FILE" 1)
        local SUB; SUB=$(substitute_params "$STMT" "50" "NULL" "NULL" "NULL" "NULL" "NULL" "$latest_part")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    03)
        # 6 statements; A=dictGet(hash→ledger_sequence), B=header,
        # C=ops, D=participants, E=events (full payload §5.1), F=invocations.
        # All take $1=hash (FixedString(32) — pass as unhex(hex)).
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E03: GET /transactions/:hash ==="
        local hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            hex="$DUMMY_HASH_HEX"
        else
            hex=$(ch_oneshot "SELECT lower(hex(hash)) FROM transaction_hash_index FINAL ORDER BY ledger_sequence DESC LIMIT 1")
            require_value "$hex" "transaction_hash_index" || return 1
            echo "  hash = $hex"
        fi
        local stmt_idx
        for stmt_idx in 1 2 3 4 5 6; do
            local STMT SUB
            STMT=$(get_statement "$FILE" "$stmt_idx")
            SUB=$(substitute_params "$STMT" "unhex('$hex')")
            [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement $stmt_idx ---"
            ch_exec "$(explain_wrap "$SUB")" || true
        done
        ;;

    04)
        # Stmt A params: $1=limit, $2=cursor_closed_at, $3=cursor_sequence.
        # Stmt B params: $1=sequence list, $2=partition list (task 0445).
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E04: GET /ledgers ==="
        local stmt_idx
        for stmt_idx in 1 2; do
            local STMT SUB
            STMT=$(get_statement "$FILE" "$stmt_idx")
            if [[ "$stmt_idx" == "1" ]]; then
                SUB=$(substitute_params "$STMT" "50" "NULL" "NULL")
            else
                SUB=$(substitute_params "$STMT" "$DUMMY_LEDGER_SEQ" \
                    "$((DUMMY_LEDGER_SEQ / 500000))")
            fi
            [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement $stmt_idx ---"
            ch_exec "$(explain_wrap "$SUB")" || true
        done
        ;;

    05)
        # Params: $1=sequence (used in both stmts), $2=cursor_lseq,
        # $3=cursor_id, $4=limit.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E05: GET /ledgers/:sequence ==="
        local seq
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            seq="$DUMMY_LEDGER_SEQ"
        else
            seq=$(ch_oneshot "SELECT max(sequence) FROM ledgers")
            require_value "$seq" "ledgers" || return 1
            echo "  sequence = $seq"
        fi
        local stmt_idx
        for stmt_idx in 1 2 3; do
            local STMT SUB
            STMT=$(get_statement "$FILE" "$stmt_idx")
            case "$stmt_idx" in
                1) SUB=$(substitute_params "$STMT" "$seq") ;;
                2) SUB=$(substitute_params "$STMT" "$seq" "NULL" "NULL" "50") ;;
                # Stmt C (task 0445): $1=sequence list, $2=partition list.
                3) SUB=$(substitute_params "$STMT" "$seq" "$((seq / 500000))") ;;
            esac
            [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement $stmt_idx ---"
            ch_exec "$(explain_wrap "$SUB")" || true
        done
        ;;

    06)
        # 2 statements. A: WHERE account_id = $1 (StrKey). B: WHERE
        # abc.account_id = $1 (Int64 surrogate from A). Capture A's
        # `id` projection and thread into B.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E06: GET /accounts/:account_id ==="
        local strkey acc_id STMT_A STMT_B SUB_A SUB_B
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_G"
            acc_id="$DUMMY_ID_I64"
        else
            strkey=$(ch_oneshot "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "accounts" || return 1
            echo "  account = $strkey"
            acc_id=$(ch_oneshot "SELECT id FROM accounts FINAL WHERE account_id = '$strkey' LIMIT 1")
            require_value "$acc_id" "accounts.id" || return 1
            echo "  id = $acc_id"
        fi
        STMT_A=$(get_statement "$FILE" 1)
        STMT_B=$(get_statement "$FILE" 2)
        SUB_A=$(substitute_params "$STMT_A" "'$strkey'")
        SUB_B=$(substitute_params "$STMT_B" "$acc_id")
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement 1 (header) ---"
        ch_exec "$(explain_wrap "$SUB_A")" || true
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement 2 (balances) ---"
        ch_exec "$(explain_wrap "$SUB_B")" || true
        ;;

    07)
        # Params: $1=account_strkey, $2=limit, $3=cursor_ledger, $4=cursor_tx_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E07: GET /accounts/:account_id/transactions ==="
        local strkey
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_G"
        else
            strkey=$(ch_oneshot "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "accounts" || return 1
            echo "  account = $strkey"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "50" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    08)
        # PR #175: assets dropped surrogate id; cursor is 4-tuple natural key.
        # Params: $1=limit, $2..$5=cursor 4-tuple, $6=asset_type_filter, $7=asset_code_filter.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E08: GET /assets ==="
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "50" "NULL" "NULL" "NULL" "NULL" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    09)
        # PR #175: takes natural 4-tuple instead of single surrogate id.
        # Params: $1=asset_type, $2=asset_code, $3=issuer_id, $4=contract_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E09: GET /assets/:id ==="
        local atype acode aiss actr
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            atype="1"; acode="'USDC'"; aiss="0"; actr="0"
        else
            # Discover any real asset's 4-tuple
            local row
            row=$(ch_oneshot "SELECT toString(asset_type) || '|' || asset_code || '|' || toString(issuer_id) || '|' || toString(contract_id) FROM assets FINAL LIMIT 1")
            require_value "$row" "assets" || return 1
            atype="${row%%|*}"; row="${row#*|}"
            acode="'${row%%|*}'"; row="${row#*|}"
            aiss="${row%%|*}"; row="${row#*|}"
            actr="$row"
            echo "  asset (type, code, issuer_id, contract_id) = ($atype, $acode, $aiss, $actr)"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "$atype" "$acode" "$aiss" "$actr")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    10)
        # 2 variants. PR #175 amendment:
        #  Variant A (classic identity): $1=asset_code, $2=asset_issuer_id, $3=limit, $4=cursor_ledger, $5=cursor_tx_id.
        #  Variant B (contract identity): $1=contract_id (Int64), $2=limit, $3=cursor_ledger, $4=cursor_tx_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E10: GET /assets/:id/transactions ==="
        # Run variant A with first classic-identity asset (asset_code+issuer_id)
        local STMT_A SUB_A acode aiss
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            acode="'USDC'"; aiss="1234"
        else
            local row_a
            row_a=$(ch_oneshot "SELECT asset_code || '|' || toString(issuer_id) FROM assets FINAL WHERE length(asset_code) > 0 AND issuer_id != 0 LIMIT 1")
            if [[ -n "$row_a" ]]; then
                acode="'${row_a%%|*}'"; aiss="${row_a#*|}"
                echo "  variant A (asset_code, issuer_id) = ($acode, $aiss)"
            else
                echo "  variant A — no classic asset; skipping"
                acode=""
            fi
        fi
        if [[ -n "$acode" ]]; then
            STMT_A=$(get_statement "$FILE" 1)
            SUB_A=$(substitute_params "$STMT_A" "$acode" "$aiss" "50" "NULL" "NULL")
            [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- variant A (classic identity) ---"
            ch_exec "$(explain_wrap "$SUB_A")" || true
        fi
        # Variant B with first contract-identity asset
        local STMT_B SUB_B actr_b
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            actr_b="1234"
        else
            actr_b=$(ch_oneshot "SELECT toString(contract_id) FROM assets FINAL WHERE contract_id != 0 LIMIT 1")
        fi
        if [[ -n "$actr_b" ]]; then
            echo "  variant B (contract_id) = $actr_b"
            STMT_B=$(get_statement "$FILE" 2)
            SUB_B=$(substitute_params "$STMT_B" "$actr_b" "50" "NULL" "NULL")
            [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- variant B (contract identity) ---"
            ch_exec "$(explain_wrap "$SUB_B")" || true
        fi
        ;;

    11)
        # 2 statements. A: WHERE contract_id = $1 (StrKey). B: WHERE
        # sia.contract_id = $1 (Int64 surrogate from A) AND closed_at filter
        # with $2 = window_days. Capture A's `id` projection and thread into B.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E11: GET /contracts/:contract_id ==="
        local strkey ctr_id STMT_A STMT_B SUB_A SUB_B
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"
            ctr_id="$DUMMY_ID_I64"
        else
            strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "soroban_contracts" || return 1
            echo "  contract = $strkey"
            ctr_id=$(ch_oneshot "SELECT id FROM soroban_contracts FINAL WHERE contract_id = '$strkey' LIMIT 1")
            require_value "$ctr_id" "soroban_contracts.id" || return 1
            echo "  id = $ctr_id"
        fi
        STMT_A=$(get_statement "$FILE" 1)
        STMT_B=$(get_statement "$FILE" 2)
        SUB_A=$(substitute_params "$STMT_A" "'$strkey'")
        SUB_B=$(substitute_params "$STMT_B" "$ctr_id" "7")
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement 1 (header) ---"
        ch_exec "$(explain_wrap "$SUB_A")" || true
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "--- statement 2 (stats, window=7d) ---"
        ch_exec "$(explain_wrap "$SUB_B")" || true
        ;;

    12)
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E12: GET /contracts/:contract_id/interface ==="
        local strkey
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"
        else
            strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "soroban_contracts" || return 1
            echo "  contract = $strkey"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$strkey'")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    13)
        # Params: $1=contract_strkey, $2=limit, $3=cursor_ledger, $4=cursor_tx_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E13: GET /contracts/:contract_id/invocations ==="
        local strkey
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"
        else
            strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "soroban_contracts" || return 1
            echo "  contract = $strkey"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "50" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    14)
        # Params: $1=contract_strkey, $2=limit, $3=cursor_ledger,
        # $4=cursor_tx_id, $5=cursor_event_index. Review #5 fix: cursor
        # tuple now includes event_index as final tiebreaker.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E14: GET /contracts/:contract_id/events ==="
        local strkey
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"
        else
            strkey=$(ch_oneshot "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$strkey" "soroban_contracts" || return 1
            echo "  contract = $strkey"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "50" "NULL" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    15)
        # PR #175: nfts dropped surrogate id; cursor is (contract_id, token_id) tuple.
        # Params: $1=limit, $2=cursor_contract_id, $3=cursor_token_id,
        #         $4=collection_name, $5=contract_strkey_filter, $6=name_filter.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E15: GET /nfts ==="
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "50" "NULL" "NULL" "NULL" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    16)
        # PR #175: takes (contract_strkey, token_id) tuple instead of surrogate id.
        # Params: $1=contract_strkey, $2=token_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E16: GET /nfts/:id ==="
        local strkey tokid
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey="$DUMMY_STRKEY_C"; tokid="'1'"
        else
            local row
            row=$(ch_oneshot "SELECT sc.contract_id || '|' || n.token_id FROM nfts n FINAL JOIN soroban_contracts sc FINAL ON sc.id = n.contract_id LIMIT 1")
            require_value "$row" "nfts" || return 1
            strkey="${row%%|*}"
            tokid="'${row#*|}'"
            echo "  nft (contract_strkey, token_id) = ($strkey, $tokid)"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$strkey'" "$tokid")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    17)
        # PR #175: takes (contract_strkey, token_id) plus cursor.
        # Params: $1=contract_strkey, $2=token_id, $3=limit, $4=cursor_ledger, $5=cursor_event_order.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E17: GET /nfts/:id/transfers ==="
        local strkey17 tokid17
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            strkey17="$DUMMY_STRKEY_C"; tokid17="'1'"
        else
            local row17
            row17=$(ch_oneshot "SELECT sc.contract_id || '|' || n.token_id FROM nfts n FINAL JOIN soroban_contracts sc FINAL ON sc.id = n.contract_id LIMIT 1")
            require_value "$row17" "nfts" || return 1
            strkey17="${row17%%|*}"
            tokid17="'${row17#*|}'"
            echo "  nft (contract_strkey, token_id) = ($strkey17, $tokid17)"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$strkey17'" "$tokid17" "50" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    18)
        # Params: $1=limit, $2=cursor_created_at_ledger, $3=cursor_pool_id,
        # $4..$7=asset pair filters, $8=min_tvl.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E18: GET /liquidity-pools ==="
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "50" "NULL" "NULL" "NULL" "NULL" "NULL" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    19)
        # Params: $1=pool_id (FixedString(32) — pass as unhex(hex)).
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E19: GET /liquidity-pools/:id ==="
        local pool_hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "liquidity_pools" || return 1
            echo "  pool = $pool_hex"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    20)
        # Params: $1=pool_id, $2=limit, $3=cursor_ledger, $4=cursor_tx_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E20: GET /liquidity-pools/:id/transactions ==="
        local pool_hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "liquidity_pools" || return 1
            echo "  pool = $pool_hex"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')" "50" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    21)
        # Params: $1=pool_id, $2=from_ledger, $3=to_ledger, $4=interval_seconds.
        # Review #6 fix: partition prune uses intDiv($3 - 1, 500000) for the
        # half-open upper bound — applied in the SQL file.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E21: GET /liquidity-pools/:id/chart ==="
        local pool_hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "liquidity_pools" || return 1
            echo "  pool = $pool_hex (range: 0..100000, bucket=1d)"
        fi
        # Range 0..100000, interval 86400s (1 day) — works on empty CH for parse.
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')" "0" "100000" "86400")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    22)
        # Params: $1=q (raw String), $2=hash_bytes (FixedString(32) or NULL),
        # $3=strkey_prefix, $4=per_group_limit, $5..$10=include_* flags.
        # Review #4 fix: tx bucket uses $2 hash_bytes (not unhex($1)) so it
        # works for base64-shaped input — applied in the SQL file.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E22: GET /search ==="
        local prefix
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            prefix="GAAA"
        else
            prefix=$(ch_oneshot "SELECT substring(account_id, 1, 4) FROM accounts FINAL ORDER BY id DESC LIMIT 1")
            require_value "$prefix" "accounts" || return 1
            echo "  query prefix = $prefix (StrKey path)"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "'$prefix'" "NULL" "'$prefix'" "10" "true" "true" "true" "true" "true" "true")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

    23)
        # Params: $1=pool_id, $2=limit, $3=cursor_shares, $4=cursor_account_id.
        [[ "$SYNTAX_ONLY" == "0" ]] && echo "=== E23: GET /liquidity-pools/:id/participants ==="
        local pool_hex
        if [[ "$SYNTAX_ONLY" == "1" ]]; then
            pool_hex="$DUMMY_POOL_HEX"
        else
            pool_hex=$(ch_oneshot "SELECT lower(hex(lp.pool_id)) FROM liquidity_pools lp JOIN lp_positions p FINAL ON p.pool_id = lp.pool_id WHERE p.shares > 0 ORDER BY lp.last_updated_ledger DESC LIMIT 1")
            require_value "$pool_hex" "lp_positions (with shares > 0)" || return 1
            echo "  pool = $pool_hex"
        fi
        local SUB; SUB=$(substitute_params "$(<"$FILE")" "unhex('$pool_hex')" "50" "NULL" "NULL")
        ch_exec "$(explain_wrap "$SUB")"
        ;;

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
