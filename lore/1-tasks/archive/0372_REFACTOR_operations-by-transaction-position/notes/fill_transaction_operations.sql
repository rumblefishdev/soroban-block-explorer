-- Fill one slice [{A}, {B}) of transaction_operations from operations_appearances
-- (task 0372): the row's transaction surrogate becomes the transaction's
-- position, its 1-based operation position becomes the 0-based
-- operation_index, and the unread fold count `amount` is left behind.
-- Writes production (chw). Re-running a slice is safe: the target is a
-- ReplacingMergeTree keyed (ledger_sequence, application_order,
-- operation_index), so a repeated row collapses.
--
-- INNER JOIN on purpose, as in task 0575: a row whose transaction is missing
-- gets no position and is left out, which the gate sees as new < old.
-- Measured read-only 2026-09-25 on 64,000,000–64,050,000: the SELECT reads
-- 42.9 M rows in 1.2 s, 2.2 GB of memory.
INSERT INTO transaction_operations (ledger_sequence, application_order, operation_index, type,
    source_id, destination_id, contract_id, asset_code, asset_issuer_id, pool_ids)
SELECT s.ledger_sequence, t.application_order, s.application_order - 1, s.type,
    s.source_id, s.destination_id, s.contract_id, s.asset_code, s.asset_issuer_id, s.pool_ids
FROM operations_appearances AS s
INNER JOIN
(
    SELECT id, ledger_sequence, application_order FROM transactions
    WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
) AS t ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
