#!/usr/bin/env bash
# Task 0266 — one backfill instance: per 64k S3 folder do sync -> worker -> delete.
# Self-contained + resumable (watermark). Run several copies on disjoint ranges
# for parallelism (see sbe-launch.sh).
#
# Usage:  sbe-loop.sh <start_ledger> <end_ledger> <watermark_file>
# Env:
#   SBE_DATA   staging + watermark dir            (default: $HOME/sbe)
#   SBE_CH_URL local ClickHouse HTTP endpoint     (default: http://localhost:8125)
#   STELLAR_NETWORK_PASSPHRASE                     (default: mainnet)
set -uo pipefail

START=${1:?usage: sbe-loop.sh <start> <end> <watermark_file>}
END=${2:?usage: sbe-loop.sh <start> <end> <watermark_file>}
WM=${3:?usage: sbe-loop.sh <start> <end> <watermark_file>}

HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="$HERE/../../target/release/pool-ids-backfill"
DATA="${SBE_DATA:-$HOME/sbe}"
P=64000

export STELLAR_NETWORK_PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Public Global Stellar Network ; September 2015}"
export CLICKHOUSE_URL="${SBE_CH_URL:-http://localhost:8125}"
export CLICKHOUSE_USER="${CLICKHOUSE_USER:-default}"
export CLICKHOUSE_PASSWORD="${CLICKHOUSE_PASSWORD:-clickhouse}"
export CLICKHOUSE_DATABASE="${CLICKHOUSE_DATABASE:-default}"

[ -x "$BIN" ] || { echo "ERROR: worker binary not found at $BIN (run cargo build --release first)"; exit 1; }
mkdir -p "$DATA/backfill"

fstart=$(( START - (START % P) ))
for pstart in $(seq "$fstart" "$P" "$END"); do
  pend=$(( pstart + P - 1 ))
  lo=$(( pstart > START ? pstart : START ))
  hi=$(( pend < END ? pend : END ))
  # skip if already past this slice (resume)
  if [ -f "$WM" ] && [ "$(cat "$WM")" -ge "$hi" ]; then continue; fi

  folder="$(printf '%08X' $((4294967295 - pstart)))--${pstart}-${pend}"
  dir="$DATA/backfill/$folder"

  for t in 1 2 3; do
    echo "[$(date '+%F %T')] sync $folder (try $t)"
    mkdir -p "$dir"
    aws s3 sync --no-sign-request --only-show-errors \
      "s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/$folder/" "$dir/" && break
    sleep 15
  done

  for t in 1 2 3; do
    echo "[$(date '+%F %T')] worker $lo..$hi (try $t)"
    "$BIN" --start "$lo" --end "$hi" --local-dir "$DATA/backfill" --watermark "$WM" && break
    sleep 30
  done

  rm -rf "$dir"   # free disk: keep only one 64k folder at a time
done
echo "[$(date '+%F %T')] DONE $START..$END"
