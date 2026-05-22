#!/usr/bin/env bash
#
# 0252 — sample pool generator.
#
# Produces 4 sample files used by Phase B/B.5/C/D scripts:
#
#   samples_ledgers.txt    — 30K stratified ledgers from Horizon-retention
#                            range (≥ 56,657,428) + 3K adversarial edges.
#                            Used by ledger-keyed endpoints (E02, E03, E04,
#                            E05, indirectly E07).
#   samples_accounts.txt   — 30K stratified accounts (by id range buckets).
#                            Used by E06, E07.
#   samples_assets.txt     — full assets population (300K) is small enough
#                            to enumerate — script samples 30K random rows.
#                            Used by E08, E09, E10.
#   samples_pools.txt      — full population sample (50K total, take all).
#                            Used by E18, E19, E20, E21, E23.
#   samples_contracts.txt  — stratified by contract_type with oversample
#                            of rare types. Used by E11, E12, E13, E14.
#
# Each file: one identifier per line, UTF-8, no header. Stable across runs
# (same RANDOM seed via `intHash64(rowNumberInAllBlocks())` order).
#
# Run once before Phase B / C / D. Idempotent — overwrites existing files.

set -euo pipefail

CONTAINER="${SBE_CH_CONTAINER:-app-clickhouse-1}"
CH_USER="${SBE_CH_USER:-default}"
CH_DB="${SBE_CH_DB:-default}"
OUT_DIR="${SBE_OUT_DIR:-/tmp/sbe-artifacts/0252}"
HORIZON_FLOOR="${SBE_HORIZON_FLOOR:-56657428}"

mkdir -p "$OUT_DIR"

ch() {
  docker exec "$CONTAINER" clickhouse-client \
    --user="$CH_USER" --database="$CH_DB" \
    --format=TabSeparated --query="$1"
}

# ----- 1. Ledger samples: 30K stratified + 3K adversarial -----

echo "[1/5] Sampling ledgers..."
{
  # 12 buckets in retention range × 2500/bucket = 30K target.
  ch "
    SELECT sequence FROM ledgers
    WHERE sequence >= $HORIZON_FLOOR
    ORDER BY intDiv(sequence - $HORIZON_FLOOR, 500000) ASC, intHash64(sequence) ASC
    LIMIT 2500 BY intDiv(sequence - $HORIZON_FLOOR, 500000)
  "

  # Adversarial: first + last per 500K partition
  ch "
    SELECT arrayJoin(edges) AS seq_edge FROM (
      SELECT [min(sequence), max(sequence)] AS edges
        FROM ledgers
       GROUP BY intDiv(sequence, 500000)
    )
  "

  # Adversarial: top 200 max-tx ledgers
  ch "SELECT sequence FROM ledgers ORDER BY transaction_count DESC LIMIT 200"

  # Adversarial: 0-tx ledgers (8 known per Phase 6 Tier 4.3 false alarm)
  ch "SELECT sequence FROM ledgers WHERE transaction_count = 0"

  # Adversarial: ±5 around worker handoff boundaries
  ch "
    SELECT sequence FROM ledgers
     WHERE sequence BETWEEN 55103990 AND 55104010
        OR sequence BETWEEN 60095990 AND 60096010
     ORDER BY sequence
  "
} | awk 'NF && !seen[$1]++ { print }' | sort -n > "$OUT_DIR/samples_ledgers.txt"

L_COUNT=$(wc -l < "$OUT_DIR/samples_ledgers.txt")
echo "  → $L_COUNT ledger samples"

# ----- 2. Account samples: 30K stratified by id range -----

echo "[2/5] Sampling accounts..."
# Total accounts ~13.88M, ids are wide-range Int64 surrogates.
# CH `%` on signed Int64 returns signed result; positiveModulo gives
# the unsigned 0..bucket-1 needed for clean stratification.
# 1000 buckets × 30/bucket = 30K target.
ch "
  SELECT account_id FROM accounts FINAL
  ORDER BY positiveModulo(id, 1000) ASC, intHash64(id) ASC
  LIMIT 30 BY positiveModulo(id, 1000)
" > "$OUT_DIR/samples_accounts.txt"

A_COUNT=$(wc -l < "$OUT_DIR/samples_accounts.txt")
echo "  → $A_COUNT account samples"

# ----- 3. Asset samples: 30K random rows from 300K population -----

echo "[3/5] Sampling assets..."
# Identity = (asset_type, asset_code, issuer_id, contract_id) 4-tuple.
ch "
  SELECT toString(asset_type) || '|' || asset_code || '|' || toString(issuer_id) || '|' || toString(contract_id)
  FROM assets FINAL
  ORDER BY intHash64(issuer_id + contract_id) ASC
  LIMIT 30000
" > "$OUT_DIR/samples_assets.txt"

AS_COUNT=$(wc -l < "$OUT_DIR/samples_assets.txt")
echo "  → $AS_COUNT asset samples"

# ----- 4. Pool samples: full population (50K total) -----

echo "[4/5] Sampling pools..."
ch "
  SELECT lower(hex(pool_id)) FROM liquidity_pools
" > "$OUT_DIR/samples_pools.txt"

P_COUNT=$(wc -l < "$OUT_DIR/samples_pools.txt")
echo "  → $P_COUNT pool samples (full population)"

# ----- 5. Contract samples: stratified by contract_type -----

echo "[5/5] Sampling contracts..."
{
  # SAC tokens (294,963 total) — 1.7% sample = 5000
  ch "
    SELECT contract_id FROM soroban_contracts FINAL
    WHERE contract_type = 0
    ORDER BY intHash64(id) ASC
    LIMIT 5000
  "
  # Other (21,523) — sample 5000 (≈ 23%)
  ch "
    SELECT contract_id FROM soroban_contracts FINAL
    WHERE contract_type = 1
    ORDER BY intHash64(id) ASC
    LIMIT 5000
  "
  # Nft (1) + Fungible (2) — take all
  ch "
    SELECT contract_id FROM soroban_contracts FINAL
    WHERE contract_type IN (2, 3)
  "
  # NULL (4875) — sample 1000 (≈ 20.5%)
  ch "
    SELECT contract_id FROM soroban_contracts FINAL
    WHERE contract_type IS NULL
    ORDER BY intHash64(id) ASC
    LIMIT 1000
  "
} | awk 'NF && !seen[$1]++ { print }' > "$OUT_DIR/samples_contracts.txt"

C_COUNT=$(wc -l < "$OUT_DIR/samples_contracts.txt")
echo "  → $C_COUNT contract samples"

echo
echo "=== Sample pool summary ==="
echo "  Ledgers:   $L_COUNT"
echo "  Accounts:  $A_COUNT"
echo "  Assets:    $AS_COUNT"
echo "  Pools:     $P_COUNT"
echo "  Contracts: $C_COUNT"
echo "  Total:     $((L_COUNT + A_COUNT + AS_COUNT + P_COUNT + C_COUNT))"
echo
echo "Output: $OUT_DIR"
ls -la "$OUT_DIR/samples_"*.txt
