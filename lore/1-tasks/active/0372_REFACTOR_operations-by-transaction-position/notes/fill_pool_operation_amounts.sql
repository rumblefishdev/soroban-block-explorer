-- Fill one slice [{A}, {B}) of pool_operation_amounts from lp_operation_amounts
-- (task 0372): transaction surrogate → transaction position, 1-based operation
-- position → 0-based operation_index. Same INNER JOIN contract as
-- fill_transaction_operations.sql. The source leads its key with pool_id, so
-- a ledger slice reads its whole partition's ledger column — cheap here
-- (2026-09-25, 64,000,000–64,050,000: 32.2 M rows read, 0.8 s, 1.7 GB).
INSERT INTO pool_operation_amounts (pool_id, ledger_sequence, application_order, operation_index,
    asset_id, amount)
SELECT s.pool_id, s.ledger_sequence, t.application_order, s.application_order - 1,
    s.asset_id, s.amount
FROM lp_operation_amounts AS s
INNER JOIN
(
    SELECT id, ledger_sequence, application_order FROM transactions
    WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
) AS t ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
