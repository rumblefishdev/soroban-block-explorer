-- Endpoint:     GET /accounts/:account_id/transactions
-- Purpose:      Paginated transactions involving a given account (as source
--               OR as a participant). Default ordering: newest first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.7
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :account_strkey      String   G-form account ID (StrKey)
--   $2  :limit               Int      page size
--   $3  :cursor_ledger       Int64    NULL on first page
--   $4  :cursor_tx_id        Int64    NULL on first page
-- Indexes:      accounts ORDER BY (id) — leading CTE resolves StrKey → id.
--               transaction_participants ORDER BY (account_id, ledger_sequence,
--                 transaction_id) — natural keyset for this scan.
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 + PARTITION BY intDiv(ledger_sequence, 500000).
--               operations_appearances ORDER BY (ledger_sequence, transaction_id,
--                 id) — LATERAL-like subquery for operation_types.
-- CH Engine:    accounts, transactions, transaction_participants,
--               operations_appearances — all Replacing (FINAL required).
-- CH Pattern:   leading id resolve via subquery; partition prune on
--               transactions via intDiv on cursor's ledger; FINAL everywhere;
--               groupUniqArray for operation_types[].
-- ADR 0044 §:   §4.2/§4.3 (Replacing partitioned), §4.5 (state Replacing),
--               §5.2 (no `created_at` — partition predicate via intDiv).
-- Notes:
--   • PG's CTE `acc` resolves the StrKey via accounts.account_id UNIQUE.
--     CH has no equivalent UNIQUE constraint; we resolve via a subquery on
--     `accounts FINAL` instead. Selectivity is high; result is one Int64
--     after FINAL.
--   • Cursor tuple drops `created_at` (§5.2) — natural keyset for CH
--     transaction_participants is `(account_id, ledger_sequence,
--     transaction_id)`. We page on `(ledger_sequence, transaction_id) <
--     ($3, $4)` since account_id is pinned in WHERE.
--   • `operation_types[]` via correlated array subquery — CH does not
--     support `LEFT JOIN LATERAL ... ON TRUE` cleanly; the idiomatic CH
--     form is a SELECT subquery in the projection that returns Array(Int16):
--     `(SELECT groupUniqArray(oa.type) FROM operations_appearances oa FINAL
--       WHERE oa.transaction_id = t.id AND oa.ledger_sequence = t.ledger_sequence)`.
--     CH 26.x runs this as a per-row correlated read; on selective
--     transaction_id + partition predicate it's fast (≤1 partition + sparse
--     PK seek per row). The Int16 array is decoded to op_type_name strings
--     in the API layer.
--   • Source StrKey via JOIN accounts FINAL on source_id.

SELECT
    lower(hex(t.hash))                                                                  AS hash_hex,
    t.ledger_sequence,
    t.application_order,
    src.account_id                                                                      AS source_account,
    t.fee_charged,
    t.successful,
    t.operation_count,
    t.has_soroban,
    (
        SELECT groupUniqArray(oa.type)
        FROM operations_appearances oa FINAL
        WHERE oa.transaction_id  = t.id
          AND oa.ledger_sequence = t.ledger_sequence
          AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                                                                   AS operation_types,
    t.id                                                                                AS cursor_tx_id
FROM transaction_participants tp FINAL
JOIN transactions t FINAL
       ON t.id = tp.transaction_id
      AND t.ledger_sequence = tp.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
WHERE
    tp.account_id = (SELECT id FROM accounts FINAL WHERE account_id = $1 LIMIT 1)
    AND ($3 IS NULL OR (tp.ledger_sequence, tp.transaction_id) < ($3, $4))
ORDER BY tp.ledger_sequence DESC, tp.transaction_id DESC
LIMIT $2;
