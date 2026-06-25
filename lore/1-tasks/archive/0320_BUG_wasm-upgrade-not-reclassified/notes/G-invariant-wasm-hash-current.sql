-- G — Invariant: soroban_contracts.wasm_hash must equal the contract's CURRENT
-- on-chain executable (the latest executable_update event's new hash).
--
-- Task 0320. ClickHouse-only (the audit-harness is Postgres-only; there is no CH
-- invariant runner yet, so this is run standalone via `chq`). Run AFTER the
-- `backfill-runner wasm-upgrade-backfill` pass and as a recurring tripwire — if a
-- co-writer ever clobbers an upgraded row back to a stale hash (the RMT whole-row
-- hazard owned by task 0316), this goes non-zero.
--
-- PASS = 0 rows / violations = 0.
-- Baseline before the fix (measured 2026-06-24, prod): 1351 violations.
--
--   chq "$(cat G-invariant-wasm-hash-current.sql)"
--
-- The new hash is topic[3] of `["executable_update", old_exec, new_exec]`, where
-- each exec is vec[Symbol("Wasm"), Bytes(hash)] (base64 in the decoded typed-JSON).

SELECT count() AS violations
FROM (
  SELECT contract_id,
         -- tie-break within a ledger by event_index (two upgrades in one ledger)
         lower(hex(base64Decode(
           JSONExtractString(JSONExtractRaw(argMax(topics_xdr, (ledger_sequence, event_index)), 3), 'value', 2, 'value')
         ))) AS chain_hash
  FROM soroban_events
  WHERE signature = 'executable_update'
  GROUP BY contract_id
) ev
INNER JOIN (SELECT id, lower(hex(wasm_hash)) AS stored FROM soroban_contracts FINAL) sc
  ON sc.id = ev.contract_id
-- `stored IS NULL` is a violation too (clobbered-to-NULL, the 0316 hazard):
-- `NULL != x` is NULL (not TRUE), so it would otherwise slip through.
WHERE sc.stored IS NULL OR sc.stored != ev.chain_hash;
