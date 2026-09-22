INSERT INTO soroban_events_staging_canonical
    (contract_id, ledger_sequence, transaction_index, operation_index, event_index,
     application_order, event_type, signature, topics_xdr, data_xdr)
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
    t.application_order                                            AS application_order,
    e.event_type                                                   AS event_type,
    e.signature                                                    AS signature,
    e.topics_xdr                                                   AS topics_xdr,
    e.data_xdr                                                     AS data_xdr
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
