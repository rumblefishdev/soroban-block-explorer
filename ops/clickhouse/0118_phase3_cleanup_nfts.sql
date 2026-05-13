-- Task 0118 Phase 3 — post-backfill cleanup of NFT false positives (CH).
--
-- Run AFTER the full Soroban-era backfill has indexed `wasm_upload`
-- ops for every observed contract, so `soroban_contracts.contract_type`
-- is populated with the WASM-derived verdict.
--
-- Patch C (parser whitelist, same task / same PR) prevents new fungible
-- transfers from being emitted as NFT candidates at parse time. This
-- script removes the historical false-positive rows that accumulated
-- under the pre-Patch-C parser behavior. Empirical baseline from the
-- 2026-05-12 CH pilot audit: 99.4% of `nfts` rows in the 15.7k-ledger
-- sample = misclassified fungibles (XLM SAC alone contributed 421k
-- rows of the 663k total).
--
-- Cross-store: equivalent script for Postgres lives at
-- `ops/sql/0118_phase3_cleanup_nfts.sql`.
--
-- ClickHouse semantics differ from PG:
--
-- - `ALTER TABLE ... DELETE` is asynchronous (a mutation). Track via
--   `system.mutations` until `is_done = 1` before running OPTIMIZE.
-- - `OPTIMIZE TABLE ... FINAL` collapses ReplacingMergeTree parts;
--   without it the deleted rows linger as tombstones until normal
--   merges run.
-- - `FINAL` in the subquery on `soroban_contracts` ensures we read
--   the latest version of each contract row (the table uses
--   ReplacingMergeTree with `wasm_observed_at` as the version key, so
--   non-FINAL reads can return stale `contract_type = 'other'` rows
--   even after reclassification).

-- 1. Sanity probe — contracts still `Other` with `nfts` rows.
SELECT countDistinct(contract_id) AS unclassified_with_nft_rows
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type = 'other'
 );

-- 2. Count rows about to be removed.
SELECT count() AS rows_to_delete
  FROM nfts
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN ('fungible', 'token')
 );

-- 3. Mutate-delete from both tables. Ordering doesn't matter on CH
--    (no FK), but issue both before OPTIMIZE so the latter compacts
--    everything in one pass.
ALTER TABLE nfts DELETE
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN ('fungible', 'token')
 );

ALTER TABLE nft_ownership DELETE
 WHERE contract_id IN (
     SELECT contract_id FROM soroban_contracts FINAL
      WHERE contract_type IN ('fungible', 'token')
 );

-- 4. Wait for both mutations to complete BEFORE running OPTIMIZE.
--    Operator script — typically a polling loop on
--    `system.mutations WHERE table IN ('nfts','nft_ownership') AND is_done = 0`.
--
-- SELECT table, mutation_id, command, is_done, latest_fail_reason
--   FROM system.mutations
--  WHERE table IN ('nfts', 'nft_ownership')
--  ORDER BY create_time DESC;

-- 5. Collapse parts after the mutation lands. `FINAL` forces a full
--    merge so the tombstones drop immediately rather than during the
--    next scheduled background merge.
OPTIMIZE TABLE nfts FINAL;
OPTIMIZE TABLE nft_ownership FINAL;
