-- Fill one slice [{A}, {B}) of a presence table's staging copy (task 0575):
-- the old row's transaction surrogate becomes the transaction's position.
-- {T} = transaction_participants | operation_asset_appearances
-- {K} = account_id               | asset_id
-- Writes production (chw). Re-running a slice is safe: the staging table is a
-- ReplacingMergeTree keyed ({K}, ledger_sequence, application_order), so a
-- repeated row collapses.
--
-- INNER JOIN on purpose: a row whose transaction is missing gets no position
-- and is left out, which gate_presence.sql sees as new_keys < old_keys and the
-- loop stops on. Rows without their transaction, measured 2026-09-23: 0 of
-- 7,600,702 and 0 of 5,657,343 on 64,450,000–64,460,000; 0 on the whole of
-- partition 128 of both tables. The SELECT reads 109 M / 51 M rows per
-- 50,000-ledger slice (1.2 s / 0.8 s).
INSERT INTO {T}_staging_position ({K}, ledger_sequence, application_order)
SELECT s.{K}, s.ledger_sequence, t.application_order
FROM {T} AS s
INNER JOIN
(
    SELECT id, ledger_sequence, application_order FROM transactions
    WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
) AS t ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
