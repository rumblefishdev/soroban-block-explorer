-- Fill one slice [{A}, {B}) of contract_activity (task 0586) from
-- contract_transactions (every (contract, transaction) pair) and
-- soroban_invocations_appearances (the caller of the invoked ones), the
-- latter located by position through transactions. Writes production (chw).
-- Re-running a slice is safe: the target is a ReplacingMergeTree keyed
-- (contract_id, ledger_sequence, application_order), and the rows are the
-- same on every run.
--
-- LEFT JOIN: a pair without an invocation keeps NULL callers (touched by an
-- operation event or an operation naming the contract only). any() over the
-- caller pair and the count as ONE value, not per column: two unmerged copies
-- of one invocation must not combine into a row with both callers set. A pair
-- without an invocation gets the tuple's default: (NULL, NULL, 0).
-- Measured read-only 2026-09-25 on 64,000,000–64,010,000: 17.4 M rows read,
-- 0.74 s, 1.1 GiB of memory; a 50,000-ledger slice exceeded the 3.73 GiB cap.
INSERT INTO contract_activity (contract_id, ledger_sequence, application_order, caller_id, caller_contract_id, invocation_count)
SELECT ct.contract_id, ct.ledger_sequence, ct.application_order, inv.caller.1, inv.caller.2, inv.caller.3
FROM (SELECT contract_id, ledger_sequence, application_order FROM contract_transactions
      WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS ct
LEFT JOIN
(
    SELECT s.contract_id AS contract_id, s.ledger_sequence AS ledger_sequence, t.application_order AS application_order,
           any((s.caller_id, s.caller_contract_id, s.amount)) AS caller
    FROM soroban_invocations_appearances AS s
    INNER JOIN
    (
        SELECT id, ledger_sequence, application_order FROM transactions
        WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
    ) AS t ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
    WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
    GROUP BY contract_id, ledger_sequence, application_order
) AS inv USING (contract_id, ledger_sequence, application_order)
