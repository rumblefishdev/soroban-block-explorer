-- Task 0118 Phase 3 — post-backfill cleanup of NFT false positives (PG).
--
-- Run AFTER the full Soroban-era backfill has indexed `wasm_upload`
-- ops for every observed contract, so `soroban_contracts.contract_type`
-- is populated with the WASM-derived verdict. Until then, contracts
-- deployed before the indexed window remain `Other`-classified and
-- this cleanup will not act on them.
--
-- Idempotent: safe to re-run; subsequent runs become no-ops once the
-- target set is empty.
--
-- Patch C (parser whitelist, same task / same PR) already prevents new
-- fungible transfers from being emitted as NFT candidates at parse
-- time. This script removes the historical false-positive rows that
-- accumulated under the pre-Patch-C parser behavior.
--
-- Cross-store: equivalent script for ClickHouse lives at
-- `ops/clickhouse/0118_phase3_cleanup_nfts.sql`.
--
-- `soroban_contracts.contract_type` is SMALLINT (per ADR 0031 +
-- `domain::ContractType`). Discriminant mapping (source of truth:
-- `crates/domain/src/enums/contract_type.rs`):
--
--   0 = Token     (SAC, no WASM)
--   1 = Other     (no usable WASM observed yet — temporary; do not delete)
--   2 = Nft       (WASM-classified non-fungible)
--   3 = Fungible  (WASM-classified fungible)
--
-- The numeric form is used below; `contract_type_name(contract_type)`
-- is available if you want labels in ad-hoc inspection but the helper
-- only exists in PG, so the script keeps the portable numeric form.

BEGIN;

-- 1. Sanity probe — how many contracts are still `Other` (=1) AND have
--    `nfts` rows? A high count means backfill has not yet observed
--    those contracts' WASM uploads. Inspect this number BEFORE the
--    DELETE; it should be ~0 after a full Soroban-era backfill.
\echo '== Sanity: contracts still Other(=1) with nft rows (should drop to 0 post-full-backfill) =='
SELECT COUNT(DISTINCT contract_id) AS unclassified_with_nft_rows
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type = 1  -- Other
 );

-- 2. Count rows about to be removed (Fungible=3 / Token=0 classifications).
\echo '== About to delete (Fungible=3 / Token=0 classified contracts) =='
SELECT COUNT(*) AS rows_to_delete
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );

-- 3. Delete `nft_ownership` rows first (FK to `nfts` may exist depending
--    on schema; doing ownership first is always safe regardless).
DELETE FROM nft_ownership
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );

-- 4. Delete `nfts` rows.
DELETE FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN (0, 3)  -- Token, Fungible
 );

COMMIT;

-- 5. Reclaim space + refresh planner stats. Run OUTSIDE the
--    transaction (VACUUM cannot run inside).
VACUUM ANALYZE nfts;
VACUUM ANALYZE nft_ownership;
