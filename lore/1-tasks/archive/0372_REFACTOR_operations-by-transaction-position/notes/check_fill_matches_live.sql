-- Read-only (chq). The gate compares key COUNTS, so a fill that shifted every
-- row the same way (an off-by-one operation index, a wrong position) would
-- still pass. This compares whole ROWS instead: the fill's SELECT, run over
-- ledgers the live indexer already wrote to both tables, must produce exactly
-- the rows the live writer produced. Run on a slice [{A}, {B}) at or after the
-- first dual-written ledger, once, before the fill starts. Pass: 0 and 0.
SELECT
  (SELECT count() FROM (
      SELECT s.ledger_sequence, t.application_order, s.application_order - 1, s.type,
             s.source_id, s.destination_id, s.contract_id, s.asset_code, s.asset_issuer_id, s.pool_ids
      FROM operations_appearances AS s
      INNER JOIN (SELECT id, ledger_sequence, application_order FROM transactions
                  WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS t
        ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
      WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
      EXCEPT DISTINCT
      SELECT ledger_sequence, application_order, operation_index, type,
             source_id, destination_id, contract_id, asset_code, asset_issuer_id, pool_ids
      FROM transaction_operations WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
  )) AS operations_rows_only_in_fill,
  (SELECT count() FROM (
      SELECT s.pool_id, s.ledger_sequence, t.application_order, s.application_order - 1, s.asset_id, s.amount
      FROM lp_operation_amounts AS s
      INNER JOIN (SELECT id, ledger_sequence, application_order FROM transactions
                  WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS t
        ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
      WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
      EXCEPT DISTINCT
      SELECT pool_id, ledger_sequence, application_order, operation_index, asset_id, amount
      FROM pool_operation_amounts WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
  )) AS amount_rows_only_in_fill
FORMAT TSV
