#!/usr/bin/env bash
#
# merge-freeze-worker.sh — Phase 3.A FREEZE for parallel-backfill merge into Hetzner CH.
#
# Task 0228 (parallel-backfill merge & validation).
# ADR 0045 (FREEZE + rsync + ATTACH PART).
# Runbook docs/runbooks/merge-parallel-backfills.md (task 0233; in flight).
#
# What this does
# --------------
# Per-worker `ALTER TABLE … FREEZE` for the 10 partitioned fact tables (per
# partition) + 9 non-partitioned state tables (whole table). All freezes
# land in /var/lib/clickhouse/shadow/<snapshot>/ as hard-link snapshots
# (zero disk overhead). Writes an audit JSON under
# docs/runbooks/artifacts/<snapshot>.json with per-table part counts,
# bytes-on-disk, and min/max block numbers as observed at freeze time.
#
# After this script completes successfully, the worker is ready for the
# next step (Phase 3.B rsync to Hetzner — separate script / runbook
# section). Live tables on the worker are NOT modified.
#
# Idempotency
# -----------
# Safe to re-run with the same --snapshot-name: if the snapshot dir already
# exists the script aborts with a clear error (caller decides: UNFREEZE
# first or pick a new name). With --force-unfreeze the script removes the
# existing snapshot via `SYSTEM UNFREEZE` first, then continues.
#
# Pre-flight checks
# -----------------
#   1. clickhouse-client reachable
#   2. no in-flight merges / mutations
#   3. no leftover /shadow/<snapshot>/ (unless --force-unfreeze)
#   4. expected partition list matches `system.parts` (warning, not error)
#
# Rollback
# --------
# `clickhouse-client --query "SYSTEM UNFREEZE WITH NAME '<snapshot>'"` undoes
# the entire freeze in one call. The artifact JSON is left in place as
# historical record.
#
# Usage
# -----
#   merge-freeze-worker.sh --worker NAME [options]
#
# Required:
#   --worker NAME            Worker identifier (e.g. laptop1, laptop2, laptop3).
#                            Used in the snapshot name and the audit file.
#
# Optional:
#   --snapshot-name NAME     Snapshot dir name. Default: phase3_<worker>_<YYYYMMDD>.
#   --range START END        Ledger range (inclusive). Asserts system.parts
#                            min/max match these bounds. Default: no check.
#   --ch-data DIR            ClickHouse data root. Default: /var/lib/clickhouse.
#   --artifact-dir DIR       Where to write the audit JSON. Default:
#                            ./docs/runbooks/artifacts (resolved relative to PWD).
#   --ch-client CMD          clickhouse-client invocation. Default: clickhouse-client.
#   --dry-run                Print SQL but don't execute.
#   --yes                    Skip the confirmation prompt.
#   --force-unfreeze         If a prior snapshot with the same name exists,
#                            SYSTEM UNFREEZE it before starting.
#   -h, --help               Show this message.
#
# Exit codes
# ----------
#   0   success
#   1   pre-flight failure
#   2   FREEZE error (partial state — see log; consider SYSTEM UNFREEZE)
#   3   verification failure (live vs shadow mismatch)
#   4   bad arguments
#
# Examples
# --------
#   ./scripts/merge-freeze-worker.sh --worker laptop1 --range 50457424 55103999 --yes
#   ./scripts/merge-freeze-worker.sh --worker laptop2 --dry-run
#   ./scripts/merge-freeze-worker.sh --worker laptop3 --force-unfreeze --yes

set -euo pipefail

# ────────────────────────────────────────────────────────────────────────────
# Constants
# ────────────────────────────────────────────────────────────────────────────

# Partitioned fact tables (PARTITION BY intDiv(ledger_sequence, 500000)).
# Source: crates/db-clickhouse/schema/init.sql.
PARTITIONED_TABLES=(
  ledgers
  transactions
  transaction_hash_index
  transaction_participants
  operations_appearances
  soroban_events
  soroban_invocations_appearances
  liquidity_pool_snapshots
  nft_ownership
)

# Non-partitioned state tables. FREEZE without PARTITION clause.
# Source: crates/db-clickhouse/schema/init.sql.
STATE_TABLES=(
  accounts
  soroban_contracts
  assets
  account_balances_current
  nfts
  liquidity_pools
  lp_positions
  wasm_interface_metadata
)

# ────────────────────────────────────────────────────────────────────────────
# Defaults
# ────────────────────────────────────────────────────────────────────────────

WORKER=""
SNAPSHOT_NAME=""
RANGE_START=""
RANGE_END=""
CH_DATA="/var/lib/clickhouse"
ARTIFACT_DIR="./docs/runbooks/artifacts"
CH_CLIENT="clickhouse-client"
DRY_RUN=false
SKIP_CONFIRM=false
FORCE_UNFREEZE=false

# ────────────────────────────────────────────────────────────────────────────
# Helpers
# ────────────────────────────────────────────────────────────────────────────

log() {
  printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >&2
}

die() {
  local code="$1"
  shift
  log "ERROR: $*"
  exit "$code"
}

usage() {
  sed -n '/^# Usage/,/^set -euo/p' "$0" | sed -E 's/^# ?//;/^set/d' >&2
  exit 4
}

ch_query() {
  if [[ "$DRY_RUN" == "true" ]]; then
    log "DRY-RUN ch_query: $*"
    return 0
  fi
  "$CH_CLIENT" --query "$@"
}

ch_query_capture() {
  # Same as ch_query but stdout passes through so caller can grab the value.
  # Always executes — dry-run can't skip read-only queries used for checks.
  "$CH_CLIENT" --query "$@"
}

confirm() {
  if [[ "$SKIP_CONFIRM" == "true" || "$DRY_RUN" == "true" ]]; then
    return 0
  fi
  printf 'Proceed? [y/N] ' >&2
  read -r reply
  case "$reply" in
    [yY]|[yY][eE][sS]) return 0 ;;
    *) die 1 "aborted by user" ;;
  esac
}

# ────────────────────────────────────────────────────────────────────────────
# Argument parsing
# ────────────────────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --worker)         WORKER="$2"; shift 2 ;;
    --snapshot-name)  SNAPSHOT_NAME="$2"; shift 2 ;;
    --range)          RANGE_START="$2"; RANGE_END="$3"; shift 3 ;;
    --ch-data)        CH_DATA="$2"; shift 2 ;;
    --artifact-dir)   ARTIFACT_DIR="$2"; shift 2 ;;
    --ch-client)      CH_CLIENT="$2"; shift 2 ;;
    --dry-run)        DRY_RUN=true; shift ;;
    --yes)            SKIP_CONFIRM=true; shift ;;
    --force-unfreeze) FORCE_UNFREEZE=true; shift ;;
    -h|--help)        usage ;;
    *)                die 4 "unknown argument: $1" ;;
  esac
done

[[ -z "$WORKER" ]] && die 4 "--worker is required"

if [[ -z "$SNAPSHOT_NAME" ]]; then
  SNAPSHOT_NAME="phase3_${WORKER}_$(date -u +%Y%m%d)"
fi

# Sanitise snapshot name — only [A-Za-z0-9_-] allowed. ALTER TABLE … WITH NAME
# expects a quoted string but rejecting unsafe chars early prevents weird
# filesystem paths or SQL injection-ish surprises.
if [[ ! "$SNAPSHOT_NAME" =~ ^[A-Za-z0-9_-]+$ ]]; then
  die 4 "snapshot name contains invalid chars: $SNAPSHOT_NAME"
fi

SHADOW_DIR="${CH_DATA}/shadow/${SNAPSHOT_NAME}"

# Resolve artifact dir to absolute so logs are unambiguous.
mkdir -p "$ARTIFACT_DIR"
ARTIFACT_DIR="$(cd "$ARTIFACT_DIR" && pwd)"
ARTIFACT_FILE="${ARTIFACT_DIR}/${SNAPSHOT_NAME}.json"
LOG_FILE="${ARTIFACT_DIR}/${SNAPSHOT_NAME}.log"

# Mirror stderr to a log file from this point on.
exec > >(tee -a "$LOG_FILE") 2>&1

log "merge-freeze-worker: starting"
log "  worker          = $WORKER"
log "  snapshot name   = $SNAPSHOT_NAME"
log "  shadow dir      = $SHADOW_DIR"
log "  ch data         = $CH_DATA"
log "  artifact file   = $ARTIFACT_FILE"
log "  range           = ${RANGE_START:-<not asserted>} .. ${RANGE_END:-<not asserted>}"
log "  dry-run         = $DRY_RUN"
log "  force-unfreeze  = $FORCE_UNFREEZE"

# ────────────────────────────────────────────────────────────────────────────
# Pre-flight
# ────────────────────────────────────────────────────────────────────────────

log "pre-flight: clickhouse-client reachable"
if ! "$CH_CLIENT" --query "SELECT 1" >/dev/null 2>&1; then
  die 1 "$CH_CLIENT cannot connect — check CLICKHOUSE_URL or invocation"
fi

log "pre-flight: no in-flight merges"
in_flight_merges="$(ch_query_capture 'SELECT count() FROM system.merges')"
if [[ "$in_flight_merges" != "0" ]]; then
  die 1 "$in_flight_merges in-flight merge(s) — wait or kill before FREEZE"
fi

log "pre-flight: no in-flight mutations"
in_flight_mut="$(ch_query_capture 'SELECT count() FROM system.mutations WHERE is_done = 0')"
if [[ "$in_flight_mut" != "0" ]]; then
  die 1 "$in_flight_mut in-flight mutation(s) — wait or kill before FREEZE"
fi

if [[ -d "$SHADOW_DIR" ]]; then
  if [[ "$FORCE_UNFREEZE" == "true" ]]; then
    log "pre-flight: snapshot dir exists, --force-unfreeze → SYSTEM UNFREEZE"
    ch_query "SYSTEM UNFREEZE WITH NAME '$SNAPSHOT_NAME'"
  else
    die 1 "$SHADOW_DIR already exists — pick a different --snapshot-name or use --force-unfreeze"
  fi
fi

# Optional: ledger range assertion against system.parts.
if [[ -n "$RANGE_START" && -n "$RANGE_END" ]]; then
  log "pre-flight: range assertion ($RANGE_START .. $RANGE_END)"
  observed_min="$(ch_query_capture "SELECT min(sequence) FROM ledgers")"
  observed_max="$(ch_query_capture "SELECT max(sequence) FROM ledgers")"
  observed_count="$(ch_query_capture "SELECT count() FROM ledgers WHERE sequence BETWEEN $RANGE_START AND $RANGE_END")"
  total_count="$(ch_query_capture "SELECT count() FROM ledgers")"

  log "  observed min/max  = $observed_min .. $observed_max"
  log "  in-range count    = $observed_count"
  log "  total count       = $total_count"

  if [[ "$observed_count" != "$total_count" ]]; then
    log "WARN: $((total_count - observed_count)) ledger rows OUTSIDE the asserted range"
    log "      (this worker may not be cleanly scoped — investigate before proceeding)"
  fi
  if [[ "$observed_min" != "$RANGE_START" ]]; then
    log "WARN: actual min ledger ($observed_min) ≠ asserted start ($RANGE_START)"
  fi
  if [[ "$observed_max" != "$RANGE_END" ]]; then
    log "WARN: actual max ledger ($observed_max) ≠ asserted end ($RANGE_END)"
  fi
fi

# Discover CH partitions present in the data.
log "pre-flight: discovering active CH partitions"
mapfile -t PARTITIONS < <(
  "$CH_CLIENT" --query "
    SELECT DISTINCT partition_id
      FROM system.parts
     WHERE active = 1
       AND table = 'ledgers'
       AND database = 'default'
     ORDER BY partition_id
  "
)
if [[ "${#PARTITIONS[@]}" -eq 0 ]]; then
  die 1 "no active partitions found in ledgers — wrong CH or empty worker"
fi
log "  partitions: ${PARTITIONS[*]}"
log "  partition count: ${#PARTITIONS[@]}"

# ────────────────────────────────────────────────────────────────────────────
# Confirm
# ────────────────────────────────────────────────────────────────────────────

cat <<EOF >&2

Ready to FREEZE:
  worker        = $WORKER
  snapshot name = $SNAPSHOT_NAME
  partitions    = ${PARTITIONS[*]} (count: ${#PARTITIONS[@]})
  partitioned tables = ${#PARTITIONED_TABLES[@]}
  state tables       = ${#STATE_TABLES[@]}
  total FREEZE calls = $(( ${#PARTITIONS[@]} * ${#PARTITIONED_TABLES[@]} + ${#STATE_TABLES[@]} ))
EOF
confirm

# ────────────────────────────────────────────────────────────────────────────
# FREEZE
# ────────────────────────────────────────────────────────────────────────────

# On any error mid-freeze, surface a hint for cleanup.
trap '
  rc=$?
  if [[ $rc -ne 0 ]]; then
    log ""
    log "FREEZE failed (exit $rc). To clean up partial snapshot:"
    log "  $CH_CLIENT --query \"SYSTEM UNFREEZE WITH NAME '\''$SNAPSHOT_NAME'\''\""
  fi
' EXIT

log "phase 3a/1: FREEZE partitioned fact tables ($((${#PARTITIONS[@]} * ${#PARTITIONED_TABLES[@]})) ops)"
freeze_count=0
for p in "${PARTITIONS[@]}"; do
  for t in "${PARTITIONED_TABLES[@]}"; do
    freeze_count=$((freeze_count + 1))
    log "  [$freeze_count] FREEZE $t partition $p"
    ch_query "ALTER TABLE $t FREEZE PARTITION '$p' WITH NAME '$SNAPSHOT_NAME'"
  done
done

log "phase 3a/2: FREEZE state tables (${#STATE_TABLES[@]} ops)"
for t in "${STATE_TABLES[@]}"; do
  freeze_count=$((freeze_count + 1))
  log "  [$freeze_count] FREEZE $t (whole table)"
  ch_query "ALTER TABLE $t FREEZE WITH NAME '$SNAPSHOT_NAME'"
done

log "phase 3a: FREEZE complete ($freeze_count operations)"

# ────────────────────────────────────────────────────────────────────────────
# Verification
# ────────────────────────────────────────────────────────────────────────────

if [[ "$DRY_RUN" == "true" ]]; then
  log "dry-run: skipping shadow-dir verification"
else
  log "verification: shadow dir contents"

  if [[ ! -d "$SHADOW_DIR" ]]; then
    die 3 "snapshot dir $SHADOW_DIR not created — FREEZE silently failed"
  fi

  shadow_files=$(find "$SHADOW_DIR" -type f 2>/dev/null | wc -l | tr -d ' ')
  shadow_size=$(du -sk "$SHADOW_DIR" 2>/dev/null | awk '{print $1}')
  log "  $shadow_files file(s), $((shadow_size / 1024)) MiB logical (hard-links — no extra disk)"

  if [[ "$shadow_files" -eq 0 ]]; then
    die 3 "shadow dir has 0 files — FREEZE silently failed"
  fi

  # Per-table sanity: shadow part count vs live active part count.
  log "verification: per-table part count parity (shadow vs live)"
  mismatch=0
  for t in "${PARTITIONED_TABLES[@]}" "${STATE_TABLES[@]}"; do
    live_parts="$(ch_query_capture "
      SELECT count(DISTINCT name)
        FROM system.parts
       WHERE table = '$t' AND active = 1 AND database = 'default'
    ")"
    table_uuid="$(ch_query_capture "
      SELECT uuid FROM system.tables WHERE name = '$t' AND database = 'default'
    ")"
    shadow_parts=0
    if [[ -d "$SHADOW_DIR/store/${table_uuid:0:3}/$table_uuid" ]]; then
      shadow_parts=$(
        find "$SHADOW_DIR/store/${table_uuid:0:3}/$table_uuid" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l | tr -d ' '
      )
    fi
    if [[ "$live_parts" != "$shadow_parts" ]]; then
      log "  MISMATCH $t: live=$live_parts shadow=$shadow_parts"
      mismatch=$((mismatch + 1))
    else
      log "  OK       $t: $live_parts parts"
    fi
  done

  if [[ "$mismatch" -gt 0 ]]; then
    die 3 "$mismatch table(s) have shadow/live mismatch — investigate before rsync"
  fi
fi

# ────────────────────────────────────────────────────────────────────────────
# Audit artifact (JSON)
# ────────────────────────────────────────────────────────────────────────────

log "writing audit artifact: $ARTIFACT_FILE"
if [[ "$DRY_RUN" == "true" ]]; then
  log "  (dry-run: skipping write)"
else
  "$CH_CLIENT" --query "
    SELECT
        toJSONString(map(
          'worker',          '$WORKER',
          'snapshot_name',   '$SNAPSHOT_NAME',
          'created_at',      toString(now64(3, 'UTC')),
          'partition_count', toString(${#PARTITIONS[@]}),
          'tables',          arrayMap(t -> map(
              'name',           t.1,
              'parts',          toString(t.2),
              'bytes_on_disk',  toString(t.3),
              'min_block',      toString(t.4),
              'max_block',      toString(t.5)
          ), groupArray((name, parts, bytes, min_block, max_block)))
        ))
      FROM (
        SELECT
            table AS name,
            count() AS parts,
            sum(bytes_on_disk) AS bytes,
            min(min_block_number) AS min_block,
            max(max_block_number) AS max_block
          FROM system.parts
         WHERE active = 1
           AND database = 'default'
         GROUP BY table
         ORDER BY table
      )
  " > "$ARTIFACT_FILE"

  log "  wrote $(wc -c < "$ARTIFACT_FILE") bytes"
fi

# Trap was for cleanup hint on error — clear it on success.
trap - EXIT

log "merge-freeze-worker: done"
log ""
log "Next steps (NOT executed by this script):"
log "  1. Verify $ARTIFACT_FILE matches the expected per-table shape for $WORKER."
log "  2. rsync $SHADOW_DIR → hetzner-ch:/var/lib/clickhouse/detached_inbox/${WORKER}/$SNAPSHOT_NAME/"
log "     (separate script / runbook section)."
log "  3. After rsync confirmed: SYSTEM UNFREEZE on this worker to release shadow."
log ""
log "Rollback (if you need to undo this freeze):"
log "  $CH_CLIENT --query \"SYSTEM UNFREEZE WITH NAME '$SNAPSHOT_NAME'\""
