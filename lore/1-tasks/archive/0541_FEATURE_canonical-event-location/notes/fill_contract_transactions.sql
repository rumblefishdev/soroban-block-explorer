INSERT INTO contract_transactions (contract_id, ledger_sequence, application_order)
SELECT DISTINCT contract_id, ledger_sequence, application_order
FROM
(
    -- Operation events only: a fee event's rpc id carries a sentinel, so only an
    -- operation event names its own transaction in `transaction_index` (the
    -- live writer reads the parser's `EventSource` instead). Reads the rekeyed
    -- slice, so it runs after `fill_insert.sql` for the same slice; after the
    -- swap, name `soroban_events` here instead.
    SELECT contract_id, ledger_sequence, application_order
    FROM soroban_events_staging_canonical
    WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
      AND transaction_index = application_order AND operation_index != 4095
    UNION ALL
    SELECT i.contract_id, i.ledger_sequence, t.application_order
    FROM soroban_invocations_appearances AS i
    INNER JOIN
    (
        SELECT id, ledger_sequence, application_order FROM transactions
        WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
    ) AS t ON t.id = i.transaction_id AND t.ledger_sequence = i.ledger_sequence
    WHERE i.ledger_sequence >= {A} AND i.ledger_sequence < {B}
    UNION ALL
    SELECT assumeNotNull(o.contract_id), o.ledger_sequence, t.application_order
    FROM operations_appearances AS o
    INNER JOIN
    (
        SELECT id, ledger_sequence, application_order FROM transactions
        WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
    ) AS t ON t.id = o.transaction_id AND t.ledger_sequence = o.ledger_sequence
    WHERE o.ledger_sequence >= {A} AND o.ledger_sequence < {B}
      AND o.contract_id IS NOT NULL
)
