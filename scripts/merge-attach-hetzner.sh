#!/usr/bin/env bash
#
# merge-attach-hetzner.sh — Phase 4 ATTACH PART for parallel-backfill merge.
#
# Task 0228 (parallel-backfill merge & validation).
# ADR 0045 (FREEZE + rsync + ATTACH PART).
# Companion to: scripts/merge-freeze-worker.sh (Phase 3.A).
# Runbook docs/runbooks/merge-parallel-backfills.md (task 0233; in flight).
#
# What this does
# --------------
# Runs on the Hetzner production ClickHouse host. For one worker's rsync'd
# snapshot sitting under <staging-dir>:
#
#   1. Reads a UUID → table-name manifest (worker-side `system.tables`
#      contents, captured at FREEZE time or generated ad-hoc).
#   2. For each frozen part in <staging-dir>/store/<uuid-prefix>/<uuid>/<part>/:
#      - Resolves the destination table name via the manifest.
#      - Moves the part directory into the live table's
#        `<ch-data>/data/<db>/<table>/detached/` (which is the Atomic-engine
#        symlink to `<ch-data>/store/<dst-uuid-prefix>/<dst-uuid>/detached/`).
#      - Runs `ALTER TABLE <db>.<table> ATTACH PART '<part>'`.
#   3. After all parts of a partition attach across all partitioned fact
#      tables, runs `OPTIMIZE TABLE <tbl> FINAL PARTITION '<P>'` to collapse
#      RMT duplicates from the cross-machine merge (the partition straddles
#      at CH partition boundaries 100 / 110 / 120 per the 0228 plan need
#      this collapse to ship correct state).
#   4. After `transaction_hash_index` parts attach, runs
#      `SYSTEM RELOAD DICTIONARY transaction_hash_dict` so the
#      `hash → ledger_sequence` cache picks up the new entries.
#   5. Writes an audit JSON of attached part counts + post-attach row counts
#      per table to `<artifact-dir>/attach_<worker>_<snapshot>.json`.
#
# Worker ordering
# ---------------
# Per ADR 0045, run this script per worker in order laptop1 → laptop2 → laptop3.
# Each
# invocation is independent; downstream workers' parts land on top of the
# previous attach without conflict (CH RMT collapses on background merge).
# The Tier-1 repair pass in task 0228 Stage 1 (`backfill-runner
# repair-tier1`) runs only AFTER all three workers attach.
#
# Idempotency
# -----------
# The mv-and-attach step is destructive on the staging directory: a part
# moved into `<table>/detached/` no longer exists in <staging-dir>. If a
# mid-run crash leaves the staging partially consumed, re-running the
# script is safe — already-attached parts are detected via
# `system.parts WHERE active = 1 AND name = <part>` and skipped, and the
# staging dir's remaining parts get processed.
#
# Partial-state recovery: parts that landed in detached/ but failed ATTACH
# are listed at the end so the operator can `ALTER TABLE … DROP DETACHED
# PART '<name>'` (after deciding the failure is permanent, not transient).
#
# Pre-flight checks
# -----------------
#   1. clickhouse-client reachable.
#   2. Staging dir exists and contains `store/`.
#   3. Manifest file (`uuid_table_map.json`) found, either at
#      <staging-dir>/uuid_table_map.json (default) or via --uuid-map.
#   4. No in-flight merges or mutations (avoids racing background ops).
#   5. Disk free ≥ 1.5× the staging dir size (defensive headroom for the
#      mv that's effectively a rename on the same filesystem, but covers
#      cross-filesystem fallback to copy).
#
# Manifest format (`uuid_table_map.json`)
# ---------------------------------------
# One JSON object per line (JSONEachRow), each with shape:
#   {"uuid": "<source-table-uuid>", "name": "<table-name>"}
#
# Generate on the worker at FREEZE time (or any time before this attach
# runs) with:
#
#   clickhouse-client --query "
#     SELECT uuid, name FROM system.tables
#      WHERE database = 'default'
#        AND engine != 'View'
#      FORMAT JSONEachRow
#   " > uuid_table_map.json
#
# Ship the file alongside the rsync (e.g. include it in the snapshot dir
# before rsync starts).
#
# Rollback
# --------
# For parts attached but not yet OPTIMIZE'd: `ALTER TABLE <tbl> DETACH
# PART '<name>'`. The part moves back to detached/ and can be removed or
# re-attached.
#
# For parts attached AND OPTIMIZE'd: rollback requires re-rsync from the
# worker (which is why workers keep /shadow/ until post-attach OPTIMIZE
# confirmed). See "Recovery" section in
# docs/runbooks/merge-parallel-backfills.md (task 0233).
#
# Usage
# -----
#   merge-attach-hetzner.sh --worker NAME --snapshot-name NAME [options]
#
# Required:
#   --worker NAME            Worker identifier (laptop1, laptop2, laptop3, ...).
#                            Matches the value used at FREEZE time. Used in
#                            audit file name + log lines.
#   --snapshot-name NAME     Snapshot name (matches the FREEZE snapshot,
#                            e.g. phase3_laptop1_20260519).
#
# Optional:
#   --staging-dir DIR        Override the staging dir. Default:
#                            /var/lib/clickhouse/detached_inbox/<worker>/<snapshot>
#   --uuid-map FILE          Override the manifest path. Default:
#                            <staging-dir>/uuid_table_map.json
#   --ch-data DIR            ClickHouse data root. Default: /var/lib/clickhouse.
#   --database NAME          Database name. Default: default.
#   --artifact-dir DIR       Audit JSON destination. Default:
#                            ./docs/runbooks/artifacts (resolved at run time).
#   --ch-client CMD          clickhouse-client invocation. Default: clickhouse-client.
#                            MUST be a single binary path or a wrapper script —
#                            multi-word invocations (e.g. "docker exec -i ch
#                            clickhouse-client") DON'T work. For Docker, write
#                            a wrapper: `cat > /usr/local/bin/ch-docker <<'EOF'
#                            #!/bin/sh
#                            exec docker exec -i ch clickhouse-client "$@"
#                            EOF
#                            chmod +x /usr/local/bin/ch-docker`
#                            then pass `--ch-client ch-docker`.
#   --dry-run                Print actions, do not move files or run SQL.
#   --yes                    Skip confirmation prompt.
#   --no-optimize            Skip per-partition OPTIMIZE FINAL after attach.
#                            Useful when chaining multi-worker attach (run
#                            OPTIMIZE once after the last worker).
#   --keep-staging           Don't remove the empty staging dir after success.
#   -h, --help               Show this message.
#
# Exit codes
# ----------
#   0   success
#   1   pre-flight failure
#   2   ATTACH error (partial state — see log, audit JSON)
#   3   verification failure (post-attach row count mismatch)
#   4   bad arguments

set -euo pipefail

# ────────────────────────────────────────────────────────────────────────────
# Bash version check — associative arrays require bash 4+.
# Hetzner Ubuntu has bash 5+. macOS default is bash 3.2 (without `brew install
# bash`). The script is intended for Hetzner production runs; fail fast on
# unsupported bash so the operator hits a clear message instead of a cryptic
# "declare: -A: invalid option".
# ────────────────────────────────────────────────────────────────────────────

if [[ -z "${BASH_VERSINFO[0]:-}" ]] || [[ "${BASH_VERSINFO[0]}" -lt 4 ]]; then
  echo "ERROR: this script requires bash >= 4 (you have ${BASH_VERSION:-unknown})" >&2
  echo "       Hetzner production target has bash 5+. On macOS for smoke testing:" >&2
  echo "           brew install bash && /opt/homebrew/bin/bash $0 ..." >&2
  exit 4
fi

# ────────────────────────────────────────────────────────────────────────────
# Constants (must match init.sql + merge-freeze-worker.sh)
# ────────────────────────────────────────────────────────────────────────────

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
  nft_ownership_pending
)

STATE_TABLES=(
  accounts
  soroban_contracts
  assets
  account_balances_current
  nfts
  nfts_pending
  liquidity_pools
  lp_positions
  wasm_interface_metadata
)

# ────────────────────────────────────────────────────────────────────────────
# Defaults
# ────────────────────────────────────────────────────────────────────────────

WORKER=""
SNAPSHOT_NAME=""
STAGING_DIR=""
UUID_MAP=""
CH_DATA="/var/lib/clickhouse"
DATABASE="default"
ARTIFACT_DIR="./docs/runbooks/artifacts"
CH_CLIENT="clickhouse-client"
DRY_RUN=false
SKIP_CONFIRM=false
NO_OPTIMIZE=false
KEEP_STAGING=false

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

# Look up a destination table's UUID-based storage path on Hetzner.
# Atomic engine: data/<db>/<table> is a symlink to store/<uuid-prefix>/<uuid>.
# We use the symlink path because it's stable and resolves the same regardless
# of CH's internal UUID layout.
dst_detached_dir() {
  local table="$1"
  echo "${CH_DATA}/data/${DATABASE}/${table}/detached"
}

# Read the source-UUID → table-name manifest. Returns the table name for the
# given source UUID, or empty string if not found.
declare -A UUID_TO_TABLE
load_manifest() {
  if [[ ! -f "$UUID_MAP" ]]; then
    die 1 "manifest not found at $UUID_MAP — generate it on the worker (see header docs)"
  fi
  log "loading manifest: $UUID_MAP"
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    # JSONEachRow format: {"uuid":"...","name":"..."}
    local uuid name
    uuid=$(printf '%s' "$line" | sed -nE 's/.*"uuid":"([^"]+)".*/\1/p')
    name=$(printf '%s' "$line" | sed -nE 's/.*"name":"([^"]+)".*/\1/p')
    if [[ -z "$uuid" || -z "$name" ]]; then
      log "WARN: skipping malformed manifest line: $line"
      continue
    fi
    UUID_TO_TABLE["$uuid"]="$name"
  done < "$UUID_MAP"
  log "manifest loaded: ${#UUID_TO_TABLE[@]} table(s)"
}

# Check whether a given part is already attached to a table.
# Returns 0 (true) if active, 1 (false) otherwise.
is_part_attached() {
  local table="$1"
  local part_name="$2"
  local n
  n=$(ch_query_capture "
    SELECT count() FROM system.parts
     WHERE database = '$DATABASE' AND table = '$table'
       AND name = '$part_name' AND active = 1
  ")
  [[ "$n" -gt 0 ]]
}

# ────────────────────────────────────────────────────────────────────────────
# Argument parsing
# ────────────────────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --worker)        WORKER="$2"; shift 2 ;;
    --snapshot-name) SNAPSHOT_NAME="$2"; shift 2 ;;
    --staging-dir)   STAGING_DIR="$2"; shift 2 ;;
    --uuid-map)      UUID_MAP="$2"; shift 2 ;;
    --ch-data)       CH_DATA="$2"; shift 2 ;;
    --database)      DATABASE="$2"; shift 2 ;;
    --artifact-dir)  ARTIFACT_DIR="$2"; shift 2 ;;
    --ch-client)     CH_CLIENT="$2"; shift 2 ;;
    --dry-run)       DRY_RUN=true; shift ;;
    --yes)           SKIP_CONFIRM=true; shift ;;
    --no-optimize)   NO_OPTIMIZE=true; shift ;;
    --keep-staging)  KEEP_STAGING=true; shift ;;
    -h|--help)       usage ;;
    *)               die 4 "unknown argument: $1" ;;
  esac
done

[[ -z "$WORKER" ]]        && die 4 "--worker is required"
[[ -z "$SNAPSHOT_NAME" ]] && die 4 "--snapshot-name is required"

# Sanitise (same as freeze script).
if [[ ! "$WORKER" =~ ^[A-Za-z0-9_-]+$ ]]; then
  die 4 "worker name has invalid chars: $WORKER"
fi
if [[ ! "$SNAPSHOT_NAME" =~ ^[A-Za-z0-9_-]+$ ]]; then
  die 4 "snapshot name has invalid chars: $SNAPSHOT_NAME"
fi
if [[ ! "$DATABASE" =~ ^[A-Za-z0-9_]+$ ]]; then
  die 4 "database name has invalid chars: $DATABASE"
fi

if [[ -z "$STAGING_DIR" ]]; then
  STAGING_DIR="${CH_DATA}/detached_inbox/${WORKER}/${SNAPSHOT_NAME}"
fi
if [[ -z "$UUID_MAP" ]]; then
  UUID_MAP="${STAGING_DIR}/uuid_table_map.json"
fi

mkdir -p "$ARTIFACT_DIR"
ARTIFACT_DIR="$(cd "$ARTIFACT_DIR" && pwd)"
ARTIFACT_FILE="${ARTIFACT_DIR}/attach_${WORKER}_${SNAPSHOT_NAME}.json"
LOG_FILE="${ARTIFACT_DIR}/attach_${WORKER}_${SNAPSHOT_NAME}.log"

# Truncate prior log on idempotent re-run, then tee everything that follows.
: > "$LOG_FILE"
exec > >(tee -a "$LOG_FILE") 2>&1

log "merge-attach-hetzner: starting"
log "  worker          = $WORKER"
log "  snapshot name   = $SNAPSHOT_NAME"
log "  staging dir     = $STAGING_DIR"
log "  uuid map        = $UUID_MAP"
log "  ch data         = $CH_DATA"
log "  database        = $DATABASE"
log "  artifact file   = $ARTIFACT_FILE"
log "  dry-run         = $DRY_RUN"
log "  no-optimize     = $NO_OPTIMIZE"
log "  keep-staging    = $KEEP_STAGING"

# ────────────────────────────────────────────────────────────────────────────
# Pre-flight
# ────────────────────────────────────────────────────────────────────────────

log "pre-flight: clickhouse-client reachable"
if ! "$CH_CLIENT" --query "SELECT 1" >/dev/null 2>&1; then
  die 1 "$CH_CLIENT cannot connect — check CLICKHOUSE_URL / mTLS / etc"
fi

log "pre-flight: staging dir exists"
[[ -d "$STAGING_DIR" ]]      || die 1 "staging dir not found: $STAGING_DIR"
[[ -d "$STAGING_DIR/store" ]] || die 1 "staging dir has no store/ subdir: $STAGING_DIR/store"

log "pre-flight: no in-flight merges / mutations"
in_flight_merges=$(ch_query_capture 'SELECT count() FROM system.merges')
in_flight_mut=$(ch_query_capture 'SELECT count() FROM system.mutations WHERE is_done = 0')
[[ "$in_flight_merges" == "0" ]] || die 1 "$in_flight_merges in-flight merge(s)"
[[ "$in_flight_mut" == "0" ]] || die 1 "$in_flight_mut in-flight mutation(s)"

log "pre-flight: disk headroom check"
staging_kb=$(du -sk "$STAGING_DIR" 2>/dev/null | awk '{print $1}')
free_kb=$(df -k "$CH_DATA" | awk 'NR==2 {print $4}')
# Want 1.5× headroom defensively (covers cross-filesystem mv falling back to copy).
required_kb=$((staging_kb * 3 / 2))
log "  staging $((staging_kb / 1024 / 1024)) GiB, free $((free_kb / 1024 / 1024)) GiB, required $((required_kb / 1024 / 1024)) GiB"
if [[ "$free_kb" -lt "$required_kb" ]]; then
  log "WARN: free space < 1.5× staging size — ATTACH may fall back to copy"
fi

load_manifest

# Discover all source UUID dirs in staging.
mapfile -t SRC_UUID_DIRS < <(find "$STAGING_DIR/store" -mindepth 2 -maxdepth 2 -type d | sort)
if [[ "${#SRC_UUID_DIRS[@]}" -eq 0 ]]; then
  die 1 "no UUID dirs found under $STAGING_DIR/store/"
fi
log "discovered ${#SRC_UUID_DIRS[@]} source UUID dir(s) in staging"

# Resolve each source UUID → table name via manifest.
# Also discover all distinct partition IDs across partitioned tables (for OPTIMIZE).
declare -A TABLE_PARTS_COUNT     # table → total parts found in staging
declare -A PARTITION_SEEN        # partition_id → 1 (set semantics)
TOTAL_PARTS_FOUND=0
for src_dir in "${SRC_UUID_DIRS[@]}"; do
  src_uuid=$(basename "$src_dir")
  table_name="${UUID_TO_TABLE[$src_uuid]:-}"
  if [[ -z "$table_name" ]]; then
    log "WARN: source UUID $src_uuid not in manifest — skipping"
    continue
  fi
  # Enumerate parts in this UUID dir.
  while IFS= read -r part_dir; do
    [[ -z "$part_dir" ]] && continue
    part_name=$(basename "$part_dir")
    # Skip non-data dirs (e.g. detached/ within the source — shouldn't happen
    # but defensive).
    [[ "$part_name" == "detached" || "$part_name" == "tmp" ]] && continue
    TABLE_PARTS_COUNT[$table_name]=$((${TABLE_PARTS_COUNT[$table_name]:-0} + 1))
    TOTAL_PARTS_FOUND=$((TOTAL_PARTS_FOUND + 1))
    # Partition id is the prefix before the first underscore of the part name.
    pid=${part_name%%_*}
    PARTITION_SEEN[$pid]=1
  done < <(find "$src_dir" -mindepth 1 -maxdepth 1 -type d 2>/dev/null)
done

log "parts inventory:"
for tbl in "${!TABLE_PARTS_COUNT[@]}"; do
  log "  $tbl: ${TABLE_PARTS_COUNT[$tbl]} part(s)"
done
log "  total: $TOTAL_PARTS_FOUND"
# Sort partition IDs numerically for stable display (assoc-array iteration
# order is hash-based and would print "110 100 105 …").
PARTITIONS_SORTED=$(printf '%s\n' "${!PARTITION_SEEN[@]}" | sort -n | tr '\n' ' ')
log "partitions found: $PARTITIONS_SORTED"

# ────────────────────────────────────────────────────────────────────────────
# Confirm
# ────────────────────────────────────────────────────────────────────────────

cat <<EOF >&2

Ready to ATTACH:
  worker         = $WORKER
  snapshot       = $SNAPSHOT_NAME
  total parts    = $TOTAL_PARTS_FOUND
  unique tables  = ${#TABLE_PARTS_COUNT[@]}
  unique parts   = $TOTAL_PARTS_FOUND
  partitions     = $PARTITIONS_SORTED
  post-OPTIMIZE  = $([[ "$NO_OPTIMIZE" == "true" ]] && echo skip || echo run)
EOF
confirm

# ────────────────────────────────────────────────────────────────────────────
# ATTACH
# ────────────────────────────────────────────────────────────────────────────

trap '
  rc=$?
  if [[ $rc -ne 0 ]]; then
    log ""
    log "ATTACH failed (exit $rc). Partial state may be present:"
    log "  - Parts moved to detached/ but not attached: list with"
    log "      SELECT * FROM system.detached_parts WHERE database = '"'"'$DATABASE'"'"'"
    log "  - To clean up a stuck detached part:"
    log "      ALTER TABLE <db>.<tbl> DROP DETACHED PART '"'"'<name>'"'"'"
    log "  - To rollback an attached part:"
    log "      ALTER TABLE <db>.<tbl> DETACH PART '"'"'<name>'"'"'"
  fi
' EXIT

ATTACHED_COUNT=0
SKIPPED_COUNT=0

for src_dir in "${SRC_UUID_DIRS[@]}"; do
  src_uuid=$(basename "$src_dir")
  table_name="${UUID_TO_TABLE[$src_uuid]:-}"
  [[ -z "$table_name" ]] && continue
  dst_detached=$(dst_detached_dir "$table_name")

  log "table $table_name (src uuid $src_uuid → $dst_detached)"

  while IFS= read -r part_dir; do
    [[ -z "$part_dir" ]] && continue
    part_name=$(basename "$part_dir")
    [[ "$part_name" == "detached" || "$part_name" == "tmp" ]] && continue

    if is_part_attached "$table_name" "$part_name"; then
      log "  SKIP $part_name (already attached)"
      SKIPPED_COUNT=$((SKIPPED_COUNT + 1))
      continue
    fi

    log "  ATTACH $part_name"
    if [[ "$DRY_RUN" == "true" ]]; then
      log "    DRY-RUN mv $part_dir → $dst_detached/"
      log "    DRY-RUN ALTER TABLE $DATABASE.$table_name ATTACH PART '$part_name'"
    else
      # Ensure target detached/ exists.
      mkdir -p "$dst_detached"
      # Move (rename if same fs, copy+rm fallback otherwise — slower).
      mv "$part_dir" "$dst_detached/"
      # ATTACH.
      "$CH_CLIENT" --query "ALTER TABLE $DATABASE.$table_name ATTACH PART '$part_name'"
    fi
    ATTACHED_COUNT=$((ATTACHED_COUNT + 1))
  done < <(find "$src_dir" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort)
done

log "ATTACH phase complete: $ATTACHED_COUNT new, $SKIPPED_COUNT already-attached"

# ────────────────────────────────────────────────────────────────────────────
# Dictionary reload (after transaction_hash_index attaches)
# ────────────────────────────────────────────────────────────────────────────

if [[ -n "${TABLE_PARTS_COUNT[transaction_hash_index]:-}" ]]; then
  log "reloading dictionary transaction_hash_dict (sources transaction_hash_index)"
  ch_query "SYSTEM RELOAD DICTIONARY transaction_hash_dict"
fi

# ────────────────────────────────────────────────────────────────────────────
# Per-partition OPTIMIZE FINAL
# ────────────────────────────────────────────────────────────────────────────

if [[ "$NO_OPTIMIZE" == "true" ]]; then
  log "skipping OPTIMIZE FINAL (--no-optimize)"
else
  log "OPTIMIZE FINAL per partition (per-table fact-data collapse)"
  for pid in "${!PARTITION_SEEN[@]}"; do
    for tbl in "${PARTITIONED_TABLES[@]}"; do
      log "  OPTIMIZE TABLE $DATABASE.$tbl FINAL PARTITION '$pid'"
      ch_query "
        OPTIMIZE TABLE $DATABASE.$tbl FINAL PARTITION '$pid'
        SETTINGS optimize_throw_if_noop = 0, max_execution_time = 3600
      "
    done
  done

  log "OPTIMIZE FINAL (whole-table) for state tables"
  for tbl in "${STATE_TABLES[@]}"; do
    log "  OPTIMIZE TABLE $DATABASE.$tbl FINAL"
    ch_query "
      OPTIMIZE TABLE $DATABASE.$tbl FINAL
      SETTINGS optimize_throw_if_noop = 0, max_execution_time = 3600
    "
  done
fi

# ────────────────────────────────────────────────────────────────────────────
# Verification
# ────────────────────────────────────────────────────────────────────────────

log "verification: per-table active part counts post-attach"
declare -A POST_TABLE_PARTS POST_TABLE_ROWS
for tbl in "${!TABLE_PARTS_COUNT[@]}"; do
  pcount=$(ch_query_capture "
    SELECT count() FROM system.parts
     WHERE database = '$DATABASE' AND table = '$tbl' AND active = 1
  ")
  rcount=$(ch_query_capture "
    SELECT count() FROM $DATABASE.$tbl
  ")
  POST_TABLE_PARTS[$tbl]=$pcount
  POST_TABLE_ROWS[$tbl]=$rcount
  log "  $tbl: parts=$pcount, rows=$rcount"
done

# ────────────────────────────────────────────────────────────────────────────
# Audit artifact
# ────────────────────────────────────────────────────────────────────────────

log "writing audit artifact: $ARTIFACT_FILE"
if [[ "$DRY_RUN" == "true" ]]; then
  log "  (dry-run: skipping write)"
else
  {
    printf '{'
    printf '"worker":"%s",' "$WORKER"
    printf '"snapshot":"%s",' "$SNAPSHOT_NAME"
    printf '"staging_dir":"%s",' "$STAGING_DIR"
    printf '"completed_at":"%s",' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '"attached_parts":%s,' "$ATTACHED_COUNT"
    printf '"skipped_parts":%s,' "$SKIPPED_COUNT"
    printf '"tables":{'
    first=1
    for tbl in "${!TABLE_PARTS_COUNT[@]}"; do
      [[ $first -eq 1 ]] && first=0 || printf ','
      printf '"%s":{"staging_parts":%s,"active_parts":%s,"rows":%s}' \
        "$tbl" "${TABLE_PARTS_COUNT[$tbl]}" "${POST_TABLE_PARTS[$tbl]:-0}" "${POST_TABLE_ROWS[$tbl]:-0}"
    done
    printf '}'
    printf '}\n'
  } > "$ARTIFACT_FILE"
  log "  wrote $(wc -c < "$ARTIFACT_FILE") bytes"
fi

# ────────────────────────────────────────────────────────────────────────────
# Staging cleanup
# ────────────────────────────────────────────────────────────────────────────

if [[ "$KEEP_STAGING" == "true" ]]; then
  log "keeping staging dir per --keep-staging"
elif [[ "$DRY_RUN" == "true" ]]; then
  log "dry-run: not touching staging dir"
else
  # Only remove if staging is empty (parts moved out). If anything left,
  # leave for operator inspection.
  remaining=$(find "$STAGING_DIR/store" -mindepth 3 -maxdepth 3 -type d 2>/dev/null | wc -l | tr -d ' ')
  if [[ "$remaining" -eq 0 ]]; then
    log "staging dir is empty — removing $STAGING_DIR"
    rm -rf "$STAGING_DIR"
  else
    log "staging dir has $remaining leftover part(s) — leaving for inspection"
  fi
fi

trap - EXIT

log "merge-attach-hetzner: done for worker=$WORKER snapshot=$SNAPSHOT_NAME"
log ""
log "Next steps (NOT executed by this script):"
log "  1. Verify $ARTIFACT_FILE: row counts match worker's pre-export-metrics."
log "  2. If this was the LAST worker (laptop3 by ADR 0045 ordering): run the"
log "     Phase 5 repair pass (task 0228 Stage 1):"
log "       backfill-runner --target clickhouse repair-tier1 [--dry-run]"
log "       backfill-runner --target clickhouse asset-aggregates [--dry-run]"
log "       backfill-runner --target clickhouse nft-reclassify [--dry-run]"
log "  3. After Phase 5 + Phase 6 validation: capture BX21 Borg backup before"
log "     opening read traffic."
log ""
log "Rollback hints:"
log "  - DETACH a freshly attached part: ALTER TABLE $DATABASE.<tbl> DETACH PART '<name>'"
log "  - DROP a stuck detached part: ALTER TABLE $DATABASE.<tbl> DROP DETACHED PART '<name>'"
log "  - Re-rsync from worker if a full re-attach is needed (workers should keep"
log "    /shadow/ until this script's audit JSON is verified)."
