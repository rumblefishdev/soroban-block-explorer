#!/usr/bin/env bash
#
# run_endpoint_ch_hetzner.sh — task 0252 Phase A smoke runner.
#
# Adapts the local `run_endpoint_ch.sh` to drive the production
# Hetzner ClickHouse (`app-clickhouse-1` container, single host).
# Differences from the local script:
#
#   • Uses `docker exec` directly (no `docker compose exec`).
#   • No --explain mode (live data only — planner-only adds noise).
#   • No --syntax-only mode (validated upstream by local CI).
#   • Adds a wall-clock + row count summary per endpoint, captured
#     to a TSV at $OUT for the validation artifact.
#
# Connection: assumed run on the Hetzner box itself (no ssh wrapper);
# the SQL files are copied to $QUERY_DIR by the deploy step.
#
# Outputs:
#   - per-endpoint stdout (results)
#   - $OUT TSV summary (id, file, statements, total_rows, wall_ms,
#                       verdict=ok|partial|fail)

set -uo pipefail

CONTAINER="${SBE_CH_CONTAINER:-app-clickhouse-1}"
CH_USER="${SBE_CH_USER:-default}"
CH_PASS="${SBE_CH_PASS:-}"
CH_DB="${SBE_CH_DB:-default}"
QUERY_DIR="${SBE_QUERY_DIR:-/tmp/endpoint-queries-clickhouse}"
OUT="${SBE_OUT_TSV:-/tmp/sbe-artifacts/phase_a_smoke_$(date -u +%Y%m%d_%H%M).tsv}"

mkdir -p "$(dirname "$OUT")"

# Build the docker exec invocation. Password optional — if empty,
# omit the flag entirely (avoids `--password=''` being interpreted
# as "prompt for password" by clickhouse-client).
ch_args=(
  --user="$CH_USER"
  --database="$CH_DB"
)
[ -n "$CH_PASS" ] && ch_args+=(--password="$CH_PASS")

ch_exec() {
  # Run SQL, return exit + row count to stderr, results to stdout.
  local sql="$1"
  docker exec "$CONTAINER" clickhouse-client "${ch_args[@]}" --query="$sql"
}

ch_oneshot() {
  # Single scalar value, TSV.
  local sql="$1"
  docker exec "$CONTAINER" clickhouse-client "${ch_args[@]}" \
    --format=TabSeparatedRaw --query="$sql"
}

get_statement() {
  local file="$1" n="$2"
  awk -v n="$n" '
    BEGIN { stmt = 1 }
    /^-- @@ split @@/ { stmt++; next }
    { if (stmt == n) print }
  ' "$file"
}

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

# Discovery wrapper — used by per-endpoint runners to get a real input
# value. Echoes the value or empty string on failure.
discover() {
  local sql="$1"
  ch_oneshot "$sql" 2>/dev/null | head -1
}

# Endpoint runner. Echoes wall-ms + statement count + row count via
# global vars so the dispatcher can record TSV.
WALL_MS=0
STMT_COUNT=0
ROW_COUNT=0
VERDICT="ok"

# run_query <sql> — execute and accumulate stats.
run_query() {
  local sql="$1"
  STMT_COUNT=$((STMT_COUNT + 1))
  local start=$(date +%s%N)
  local out
  if ! out=$(ch_exec "$sql" 2>&1); then
    VERDICT="fail"
    echo "  STMT $STMT_COUNT: FAILED"
    echo "$out" | head -3 | sed 's/^/    /'
    return 1
  fi
  local end=$(date +%s%N)
  local wall=$(( (end - start) / 1000000 ))
  WALL_MS=$((WALL_MS + wall))
  local rows
  rows=$(echo "$out" | grep -c . || echo 0)
  ROW_COUNT=$((ROW_COUNT + rows))
  echo "  STMT $STMT_COUNT: $rows rows in ${wall}ms"
  # Preview first 3 rows of result for sanity check
  echo "$out" | head -3 | sed 's/^/    /'
}

run_one() {
  local id="$1"
  local nn
  nn=$(printf '%02d' "$((10#$id))")
  local FILE
  FILE=$(ls "$QUERY_DIR"/${nn}_*.sql 2>/dev/null | head -1)
  if [ -z "$FILE" ]; then
    echo "unknown id: $id"
    return 1
  fi
  WALL_MS=0; STMT_COUNT=0; ROW_COUNT=0; VERDICT="ok"

  echo "==================== E$nn: $(basename "$FILE") ===================="

  case "$id" in
    01) run_query "$(cat "$FILE")" ;;

    02)
      local latest_part
      latest_part=$(discover "SELECT intDiv(max(ledger_sequence), 500000) FROM transactions FINAL")
      [ -z "$latest_part" ] && { VERDICT="fail"; echo "  SKIP: no transactions"; return; }
      echo "  latest_partition=$latest_part"
      local SUB
      SUB=$(substitute_params "$(get_statement "$FILE" 1)" "50" "NULL" "NULL" "NULL" "NULL" "NULL" "$latest_part")
      run_query "$SUB"
      ;;

    03)
      local hex
      hex=$(discover "SELECT lower(hex(hash)) FROM transaction_hash_index FINAL ORDER BY ledger_sequence DESC LIMIT 1")
      [ -z "$hex" ] && { VERDICT="fail"; echo "  SKIP: no transactions"; return; }
      echo "  hash=$hex"
      for stmt_idx in 1 2 3 4 5 6; do
        local SUB
        SUB=$(substitute_params "$(get_statement "$FILE" "$stmt_idx")" "unhex('$hex')")
        run_query "$SUB" || true
      done
      ;;

    04)
      local SUB
      SUB=$(substitute_params "$(cat "$FILE")" "50" "NULL" "NULL")
      run_query "$SUB"
      ;;

    05)
      local seq
      seq=$(discover "SELECT max(sequence) FROM ledgers")
      [ -z "$seq" ] && { VERDICT="fail"; echo "  SKIP: no ledgers"; return; }
      echo "  sequence=$seq"
      for stmt_idx in 1 2; do
        local SUB STMT
        STMT=$(get_statement "$FILE" "$stmt_idx")
        if [ "$stmt_idx" = "1" ]; then
          SUB=$(substitute_params "$STMT" "$seq")
        else
          SUB=$(substitute_params "$STMT" "$seq" "NULL" "NULL" "50")
        fi
        run_query "$SUB" || true
      done
      ;;

    06)
      local strkey acc_id
      strkey=$(discover "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$strkey" ] && { VERDICT="fail"; echo "  SKIP: no accounts"; return; }
      acc_id=$(discover "SELECT id FROM accounts FINAL WHERE account_id = '$strkey' LIMIT 1")
      echo "  account=$strkey id=$acc_id"
      run_query "$(substitute_params "$(get_statement "$FILE" 1)" "'$strkey'")" || true
      run_query "$(substitute_params "$(get_statement "$FILE" 2)" "$acc_id")" || true
      ;;

    07)
      local strkey
      strkey=$(discover "SELECT account_id FROM accounts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$strkey" ] && { VERDICT="fail"; echo "  SKIP: no accounts"; return; }
      echo "  account=$strkey"
      run_query "$(substitute_params "$(cat "$FILE")" "'$strkey'" "50" "NULL" "NULL")"
      ;;

    08)
      run_query "$(substitute_params "$(cat "$FILE")" "50" "NULL" "NULL" "NULL" "NULL" "NULL" "NULL")"
      ;;

    09)
      local row atype acode aiss actr
      row=$(discover "SELECT toString(asset_type) || '|' || asset_code || '|' || toString(issuer_id) || '|' || toString(contract_id) FROM assets FINAL LIMIT 1")
      [ -z "$row" ] && { VERDICT="fail"; echo "  SKIP: no assets"; return; }
      atype="${row%%|*}"; row="${row#*|}"
      acode="'${row%%|*}'"; row="${row#*|}"
      aiss="${row%%|*}"; row="${row#*|}"
      actr="$row"
      echo "  asset=($atype,$acode,$aiss,$actr)"
      run_query "$(substitute_params "$(cat "$FILE")" "$atype" "$acode" "$aiss" "$actr")"
      ;;

    10)
      local row_a acode aiss
      row_a=$(discover "SELECT asset_code || '|' || toString(issuer_id) FROM assets FINAL WHERE length(asset_code) > 0 AND issuer_id != 0 LIMIT 1")
      if [ -n "$row_a" ]; then
        acode="'${row_a%%|*}'"; aiss="${row_a#*|}"
        echo "  variant A (acode=$acode aiss=$aiss)"
        run_query "$(substitute_params "$(get_statement "$FILE" 1)" "$acode" "$aiss" "50" "NULL" "NULL")" || true
      fi
      local actr_b
      actr_b=$(discover "SELECT toString(contract_id) FROM assets FINAL WHERE contract_id != 0 LIMIT 1")
      if [ -n "$actr_b" ]; then
        echo "  variant B (contract_id=$actr_b)"
        run_query "$(substitute_params "$(get_statement "$FILE" 2)" "$actr_b" "50" "NULL" "NULL")" || true
      fi
      ;;

    11)
      local strkey ctr_id
      strkey=$(discover "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$strkey" ] && { VERDICT="fail"; echo "  SKIP: no soroban_contracts"; return; }
      ctr_id=$(discover "SELECT id FROM soroban_contracts FINAL WHERE contract_id = '$strkey' LIMIT 1")
      echo "  contract=$strkey id=$ctr_id"
      run_query "$(substitute_params "$(get_statement "$FILE" 1)" "'$strkey'")" || true
      run_query "$(substitute_params "$(get_statement "$FILE" 2)" "$ctr_id" "7")" || true
      ;;

    12)
      local strkey
      strkey=$(discover "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$strkey" ] && { VERDICT="fail"; echo "  SKIP: no soroban_contracts"; return; }
      echo "  contract=$strkey"
      run_query "$(substitute_params "$(cat "$FILE")" "'$strkey'")"
      ;;

    13)
      local strkey
      strkey=$(discover "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$strkey" ] && { VERDICT="fail"; echo "  SKIP: no soroban_contracts"; return; }
      echo "  contract=$strkey"
      run_query "$(substitute_params "$(cat "$FILE")" "'$strkey'" "50" "NULL" "NULL")"
      ;;

    14)
      local strkey
      strkey=$(discover "SELECT contract_id FROM soroban_contracts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$strkey" ] && { VERDICT="fail"; echo "  SKIP: no soroban_contracts"; return; }
      echo "  contract=$strkey"
      run_query "$(substitute_params "$(cat "$FILE")" "'$strkey'" "50" "NULL" "NULL" "NULL")"
      ;;

    15)
      run_query "$(substitute_params "$(cat "$FILE")" "50" "NULL" "NULL" "NULL" "NULL" "NULL")"
      ;;

    16)
      local row strkey tokid
      row=$(discover "SELECT sc.contract_id || '|' || n.token_id FROM nfts n FINAL JOIN soroban_contracts sc FINAL ON sc.id = n.contract_id LIMIT 1")
      if [ -z "$row" ]; then
        VERDICT="partial"; echo "  SKIP: no nfts (expected — empty per 0228 nft-reclassify)"; return
      fi
      strkey="${row%%|*}"; tokid="'${row#*|}'"
      run_query "$(substitute_params "$(cat "$FILE")" "'$strkey'" "$tokid")"
      ;;

    17)
      local row strkey tokid
      row=$(discover "SELECT sc.contract_id || '|' || n.token_id FROM nfts n FINAL JOIN soroban_contracts sc FINAL ON sc.id = n.contract_id LIMIT 1")
      if [ -z "$row" ]; then
        VERDICT="partial"; echo "  SKIP: no nfts (expected)"; return
      fi
      strkey="${row%%|*}"; tokid="'${row#*|}'"
      run_query "$(substitute_params "$(cat "$FILE")" "'$strkey'" "$tokid" "50" "NULL" "NULL")"
      ;;

    18)
      run_query "$(substitute_params "$(cat "$FILE")" "50" "NULL" "NULL" "NULL" "NULL" "NULL" "NULL" "NULL")"
      ;;

    19)
      local pool_hex
      pool_hex=$(discover "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
      [ -z "$pool_hex" ] && { VERDICT="fail"; echo "  SKIP: no liquidity_pools"; return; }
      echo "  pool=$pool_hex"
      run_query "$(substitute_params "$(cat "$FILE")" "unhex('$pool_hex')")"
      ;;

    20)
      local pool_hex
      pool_hex=$(discover "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
      [ -z "$pool_hex" ] && { VERDICT="fail"; echo "  SKIP: no liquidity_pools"; return; }
      echo "  pool=$pool_hex"
      run_query "$(substitute_params "$(cat "$FILE")" "unhex('$pool_hex')" "50" "NULL" "NULL")"
      ;;

    21)
      local pool_hex
      pool_hex=$(discover "SELECT lower(hex(pool_id)) FROM liquidity_pools ORDER BY last_updated_ledger DESC LIMIT 1")
      [ -z "$pool_hex" ] && { VERDICT="fail"; echo "  SKIP: no liquidity_pools"; return; }
      local lo hi
      lo=$(discover "SELECT min(ledger_sequence) FROM liquidity_pool_snapshots WHERE pool_id = unhex('$pool_hex')")
      hi=$(discover "SELECT max(ledger_sequence) FROM liquidity_pool_snapshots WHERE pool_id = unhex('$pool_hex')")
      [ -z "$lo" ] && lo=0
      [ -z "$hi" ] && hi=100000
      echo "  pool=$pool_hex range=$lo..$hi bucket=86400s"
      run_query "$(substitute_params "$(cat "$FILE")" "unhex('$pool_hex')" "$lo" "$hi" "86400")"
      ;;

    22)
      local prefix
      prefix=$(discover "SELECT substring(account_id, 1, 4) FROM accounts FINAL ORDER BY id DESC LIMIT 1")
      [ -z "$prefix" ] && { VERDICT="fail"; echo "  SKIP: no accounts"; return; }
      echo "  query prefix=$prefix"
      run_query "$(substitute_params "$(cat "$FILE")" "'$prefix'" "NULL" "'$prefix'" "10" "true" "true" "true" "true" "true" "true")"
      ;;

    23)
      local pool_hex
      pool_hex=$(discover "SELECT lower(hex(lp.pool_id)) FROM liquidity_pools lp JOIN lp_positions p FINAL ON p.pool_id = lp.pool_id WHERE p.shares > 0 ORDER BY lp.last_updated_ledger DESC LIMIT 1")
      [ -z "$pool_hex" ] && { VERDICT="partial"; echo "  SKIP: no lp_positions with shares > 0"; return; }
      echo "  pool=$pool_hex"
      run_query "$(substitute_params "$(cat "$FILE")" "unhex('$pool_hex')" "50" "NULL" "NULL")"
      ;;

    *)
      echo "unknown id: $id"
      return 1
      ;;
  esac

  printf '%s\t%s\t%d\t%d\t%d\t%s\n' \
    "E$nn" "$(basename "$FILE")" "$STMT_COUNT" "$ROW_COUNT" "$WALL_MS" "$VERDICT" >> "$OUT"
}

# ----- main -----
if [ "${1:-all}" = "all" ]; then
  echo -e "endpoint\tfile\tstatements\ttotal_rows\twall_ms\tverdict" > "$OUT"
  for i in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15 16 17 18 19 20 21 22 23; do
    echo
    run_one "$i" || echo "  -> dispatcher fail (id=$i)"
  done
  echo
  echo "================================================================"
  echo "Phase A smoke summary at $OUT:"
  column -t -s $'\t' < "$OUT"
else
  run_one "$1"
fi
