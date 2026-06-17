#!/usr/bin/env bash
# Task 0266 — live progress monitor across all running instances.
# Refreshes every 60s: per-instance watermark, row counts in local CH, free disk.
# Env: SBE_DATA (default $HOME/sbe).
DATA="${SBE_DATA:-$HOME/sbe}"
CH_PW="${CLICKHOUSE_PASSWORD:-clickhouse}"
while true; do
  clear
  echo "=== $(date '+%F %T') — pool_ids/gross_volume_a backfill ==="
  echo
  for wm in "$DATA"/wm*.txt; do
    [ -f "$wm" ] || continue
    printf "  %-14s last ledger: %s\n" "$(basename "$wm")" "$(cat "$wm")"
  done
  echo
  echo "rows in local CH:"
  docker exec sbe-local-ch clickhouse-client --password "$CH_PW" -q \
    "SELECT 'op_rows' AS t, count() AS n FROM operations_appearances
     UNION ALL SELECT 'snap_rows', count() FROM liquidity_pool_snapshots" 2>/dev/null \
    || echo "  (local CH not reachable)"
  echo
  echo "disk:"; df -h "$DATA" | awk 'NR==2 {print "  "$4" free ("$5" used) on "$6}'
  sleep 60
done
