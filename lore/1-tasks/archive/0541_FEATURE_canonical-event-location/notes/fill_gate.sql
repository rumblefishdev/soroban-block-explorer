-- Pre-fill gate for one slice [{A}, {B}) of 5,000 ledgers: substitute both,
-- run read-only (chq). The inner SELECT is fill_insert.sql's SELECT plus the
-- old key columns. Pass: new_keys = old_keys (a collision would be merged away
-- silently by ReplacingMergeTree); fee counts as in the implementation plan.
SELECT
    {A}                                                                                      AS lo,
    count()                                                                                  AS n,
    uniqExact(contract_id, ledger_sequence, transaction_index, operation_index, event_index) AS new_keys,
    uniqExact(contract_id, ledger_sequence, old_transaction_id, old_event_index)            AS old_keys,
    countIf(transaction_index = 0)                                                           AS before_all,
    countIf(operation_index = 4095)                                                          AS after_tx,
    countIf(transaction_index = 1048575)                                                     AS after_all
FROM
(
    SELECT
        e.contract_id                                                  AS contract_id,
        e.ledger_sequence                                              AS ledger_sequence,
        toUInt32(multiIf(o.has_op = 1,               t.application_order,
                         e.event_index = 0,          0,
                         e.ledger_sequence >= 58762518, 1048575,
                                                     t.application_order))  AS transaction_index,
        toUInt16(multiIf(o.has_op = 1,               o.op_index,
                         e.event_index = 0,          0,
                         e.ledger_sequence >= 58762518, 0,
                                                     4095))                 AS operation_index,
        toUInt32(multiIf(o.has_op = 1,               o.event_pos_in_op,
                         e.event_index = 0,          t.application_order - 1,
                         e.ledger_sequence >= 58762518, r.refund_rank,
                                                     0))                    AS event_index,
        e.transaction_id                                               AS old_transaction_id,
        e.event_index                                                  AS old_event_index
    FROM soroban_events AS e
    INNER JOIN
    (
        SELECT id, application_order FROM transactions
        WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
    ) AS t ON t.id = e.transaction_id
    LEFT JOIN
    (
        SELECT ledger_sequence, application_order, event_index,
               any(op_index) AS op_index, any(event_pos_in_op) AS event_pos_in_op, toUInt8(1) AS has_op
        FROM soroban_event_ops
        WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
        GROUP BY ledger_sequence, application_order, event_index
    ) AS o ON o.ledger_sequence = e.ledger_sequence
          AND o.application_order = t.application_order
          AND o.event_index = e.event_index
    LEFT JOIN
    (
        SELECT f.ledger_sequence AS ledger_sequence, f.transaction_id AS transaction_id,
               toUInt32(row_number() OVER (PARTITION BY f.ledger_sequence ORDER BY ft.application_order) - 1) AS refund_rank
        FROM soroban_events AS f
        INNER JOIN
        (
            SELECT id, application_order FROM transactions
            WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
        ) AS ft ON ft.id = f.transaction_id
        WHERE f.ledger_sequence >= {A} AND f.ledger_sequence < {B}
          AND f.contract_id = -6164601581949826601 AND f.signature = 'fee' AND f.event_index = 1
    ) AS r ON r.ledger_sequence = e.ledger_sequence AND r.transaction_id = e.transaction_id
    WHERE e.ledger_sequence >= {A} AND e.ledger_sequence < {B}
)
FORMAT TSV
