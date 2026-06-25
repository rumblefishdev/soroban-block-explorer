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
  -- `event_type = 0` = SYSTEM: only the host emits `executable_update`, always as
  -- a System event. A contract can emit a Contract-typed (event_type = 1) event
  -- with the same topic shape; restrict to System so a spoof can't skew chain_hash.
  WHERE signature = 'executable_update' AND event_type = 0
  GROUP BY contract_id
) ev
-- LEFT JOIN (not INNER): a contract with an upgrade event but NO `soroban_contracts`
-- row is itself a violation — INNER would silently drop it and mask the anomaly.
LEFT JOIN (SELECT id, lower(hex(wasm_hash)) AS stored FROM soroban_contracts FINAL) sc
  ON sc.id = ev.contract_id
-- Violations: no current row at all (`sc.id IS NULL`), a clobbered-to-NULL hash
-- (`sc.stored IS NULL`, the 0316 hazard — `NULL != x` is NULL, would slip through),
-- or a stored hash that differs from the chain-current hash.
WHERE sc.id IS NULL OR sc.stored IS NULL OR sc.stored != ev.chain_hash;
