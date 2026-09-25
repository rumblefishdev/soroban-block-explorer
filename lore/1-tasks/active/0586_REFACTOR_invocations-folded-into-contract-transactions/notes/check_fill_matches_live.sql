-- Read-only (chq). The gate compares key COUNTS, so a fill that put the
-- right number of callers on the wrong rows would still pass. This compares
-- whole ROWS: the fill's SELECT, run over ledgers the live indexer already
-- wrote to contract_activity, must produce exactly the rows the live writer
-- produced. Run once on a slice [{A}, {B}) at or after the first dual-written
-- ledger, before the fill starts, and again on a slice of every partition
-- before the drop. Pass: 0 and 0.
SELECT
  (SELECT count() FROM (
      SELECT ct.contract_id, ct.ledger_sequence, ct.application_order, inv.caller.1, inv.caller.2, inv.caller.3
      FROM (SELECT contract_id, ledger_sequence, application_order FROM contract_transactions
            WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS ct
      LEFT JOIN
      (
          SELECT s.contract_id AS contract_id, s.ledger_sequence AS ledger_sequence, t.application_order AS application_order,
                 any((s.caller_id, s.caller_contract_id, s.amount)) AS caller
          FROM soroban_invocations_appearances AS s
          INNER JOIN (SELECT id, ledger_sequence, application_order FROM transactions
                      WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS t
            ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
          WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
          GROUP BY contract_id, ledger_sequence, application_order
      ) AS inv USING (contract_id, ledger_sequence, application_order)
      EXCEPT DISTINCT
      SELECT contract_id, ledger_sequence, application_order, caller_id, caller_contract_id, invocation_count
      FROM contract_activity WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
  )) AS rows_only_in_old,
  (SELECT count() FROM (
      SELECT contract_id, ledger_sequence, application_order, caller_id, caller_contract_id, invocation_count
      FROM contract_activity WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
      EXCEPT DISTINCT
      SELECT ct.contract_id, ct.ledger_sequence, ct.application_order, inv.caller.1, inv.caller.2, inv.caller.3
      FROM (SELECT contract_id, ledger_sequence, application_order FROM contract_transactions
            WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS ct
      LEFT JOIN
      (
          SELECT s.contract_id AS contract_id, s.ledger_sequence AS ledger_sequence, t.application_order AS application_order,
                 any((s.caller_id, s.caller_contract_id, s.amount)) AS caller
          FROM soroban_invocations_appearances AS s
          INNER JOIN (SELECT id, ledger_sequence, application_order FROM transactions
                      WHERE ledger_sequence >= {A} AND ledger_sequence < {B}) AS t
            ON t.id = s.transaction_id AND t.ledger_sequence = s.ledger_sequence
          WHERE s.ledger_sequence >= {A} AND s.ledger_sequence < {B}
          GROUP BY contract_id, ledger_sequence, application_order
      ) AS inv USING (contract_id, ledger_sequence, application_order)
  )) AS rows_only_in_new
FORMAT TSV
