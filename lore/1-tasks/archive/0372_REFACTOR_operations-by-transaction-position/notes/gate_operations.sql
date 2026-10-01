-- One count of the gate — read-only (chq). fill_operations.zsh runs it per
-- quarter [{LO}, {HI}) of a filled slice, on the old and the new table of each
-- pair, and compares the sums. Pass: equal.
--
-- Distinct keys, not count(): both sides are version-less ReplacingMergeTrees
-- with unmerged duplicates (64,000,000–64,050,000: 26,098,807 rows, 26,092,024
-- keys in operations_appearances). A transaction's position is unique in its
-- ledger, as its surrogate is, so the key maps one to one; equal sums mean the
-- same set of rows. Quarters keep each query under the read profile's memory
-- cap (a quarter of operations_appearances: 1.5 GB).
--   operations_appearances  {K} = ledger_sequence, transaction_id, application_order
--   transaction_operations  {K} = ledger_sequence, application_order, operation_index
--   lp_operation_amounts    {K} = pool_id, ledger_sequence, transaction_id, application_order, asset_id
--   pool_operation_amounts  {K} = pool_id, ledger_sequence, application_order, operation_index, asset_id
SELECT uniqExact({K}) FROM {TBL}
WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}
FORMAT TSV
