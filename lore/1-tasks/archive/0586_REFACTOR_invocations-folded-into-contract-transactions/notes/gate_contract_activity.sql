-- The gate of one slice [{LO}, {HI}) — read-only (chq). Six numbers: two
-- pairs that must be equal, and two that must be 0:
--   presence  distinct (contract, position) of contract_transactions
--   activity  distinct (contract, position) of contract_activity
--   invoked   distinct (contract, transaction) of soroban_invocations_appearances
--   callers   distinct (contract, position) of contract_activity with a caller
--   both      rows of contract_activity with both callers set (must be 0: an
--             invocation names exactly one caller)
--   count     rows where "has a caller" and "invocation_count > 0" disagree
--             (must be 0: invoked rows count their calls, others count 0)
-- presence = activity: every pair copied. invoked = callers: every invocation
-- found its pair and its caller (an invocation missing from
-- contract_transactions would show as callers < invoked). A transaction's
-- position is unique in its ledger, as its surrogate is, so the keys map one
-- to one. Measured 2026-09-25, 64,000,000–64,010,000: 2,915,824 = 2,915,824
-- and 1,978,709 = 1,978,709 (read-only dry run of the fill SELECT).
SELECT
  (SELECT uniqExact(contract_id, ledger_sequence, application_order) FROM contract_transactions
   WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}),
  (SELECT uniqExact(contract_id, ledger_sequence, application_order) FROM contract_activity
   WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}),
  (SELECT uniqExact(contract_id, ledger_sequence, transaction_id) FROM soroban_invocations_appearances
   WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}),
  (SELECT uniqExact(contract_id, ledger_sequence, application_order) FROM contract_activity
   WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}
     AND (isNotNull(caller_id) OR isNotNull(caller_contract_id))),
  (SELECT count() FROM contract_activity
   WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}
     AND isNotNull(caller_id) AND isNotNull(caller_contract_id)),
  (SELECT count() FROM contract_activity
   WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}
     AND (isNotNull(caller_id) OR isNotNull(caller_contract_id)) != (invocation_count > 0))
FORMAT TSV
