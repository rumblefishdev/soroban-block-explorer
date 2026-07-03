#!/usr/bin/env bash
# Task 0266 — launch the 8 total_ops-weighted backfill instances in the
# background. Each owns a disjoint ledger range + its own watermark + log.
# Run inside tmux so it survives SSH disconnect.
#
# Usage:  sbe-launch.sh <final_end_ledger>
#   <final_end_ledger> = latest ledger present in the public S3 archive
#   (NOT the live ClickHouse tip — the archive lags). Find it with:
#     NEWEST=$(aws s3 ls --no-sign-request s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/ | awk '{print $2}' | sort | head -1)
#     aws s3 ls --no-sign-request "s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/${NEWEST}" | sed -E 's/.*--([0-9]+)\.xdr\.zst/\1/' | sort -n | tail -1
#
# Env: SBE_DATA (default $HOME/sbe).
set -euo pipefail

END8=${1:?usage: sbe-launch.sh <final_end_ledger> (latest archived ledger)}
HERE="$(cd "$(dirname "$0")" && pwd)"
DATA="${SBE_DATA:-$HOME/sbe}"
LOOP="$HERE/sbe-loop.sh"
mkdir -p "$DATA"

run() {  # <start> <end> <n>
  SBE_DATA="$DATA" "$LOOP" "$1" "$2" "$DATA/wm$3.txt" > "$DATA/bf$3.log" 2>&1 &
  echo "  instance $3: $1..$2  (pid $!, log $DATA/bf$3.log)"
}

echo "launching 8 instances (total_ops-weighted split), data dir $DATA:"
run 50457424 51999999 1
run 52000000 54499999 2
run 54500000 55999999 3
run 56000000 57499999 4
run 57500000 58999999 5
run 59000000 59999999 6
run 60000000 61499999 7
run 61500000 "$END8"  8
echo
echo "all 8 backgrounded. monitor: $HERE/sbe-progress.sh   (or: tail -f $DATA/bf*.log)"
echo "watermarks: $DATA/wm*.txt   detach tmux with Ctrl-b d"
