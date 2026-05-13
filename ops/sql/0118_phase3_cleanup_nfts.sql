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

BEGIN;

-- 1. Sanity probe — how many contracts are still `Other` AND have
--    `nfts` rows? A high count means backfill has not yet observed
--    those contracts' WASM uploads, so a DELETE would either be a
--    no-op (no rows to drop because the cleanup targets fungible/token,
--    not other) or — if we later loosen the predicate — would risk
--    purging genuine NFTs whose WASM has simply not been classified
--    yet. Inspect this number BEFORE the DELETE.
\echo '== Sanity: contracts still Other with nft rows (should drop to 0 post-full-backfill) =='
SELECT COUNT(DISTINCT contract_id) AS unclassified_with_nft_rows
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type = 'other'
 );

-- 2. Count rows about to be removed (fungible / token classifications).
\echo '== About to delete (fungible / token-classified contracts) =='
SELECT COUNT(*) AS rows_to_delete
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN ('fungible', 'token')
 );

-- 3. Delete `nft_ownership` rows first (FK to `nfts` may exist depending
--    on schema; doing ownership first is always safe regardless).
DELETE FROM nft_ownership
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN ('fungible', 'token')
 );

-- 4. Delete `nfts` rows.
DELETE FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts
      WHERE contract_type IN ('fungible', 'token')
 );

COMMIT;

-- 5. Reclaim space + refresh planner stats. Run OUTSIDE the
--    transaction (VACUUM cannot run inside).
VACUUM ANALYZE nfts;
VACUUM ANALYZE nft_ownership;
