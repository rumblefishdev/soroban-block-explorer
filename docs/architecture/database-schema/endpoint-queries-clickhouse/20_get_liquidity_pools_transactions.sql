-- ============================================================================
-- ⚠️  CH 26.3 CORRECTION (task 0243) — do NOT implement the
--     `operation_types` / `contract_ids` arrays with the correlated scalar
--     subqueries shown below (`… WHERE oa.transaction_id = t.id`): ClickHouse
--     26.3 rejects them with `Code: 48 NOT_IMPLEMENTED`. Use the NON-correlated
--     two-step the shipped modules use — fetch the page of tx keys, then
--     aggregate per `(ledger_sequence, transaction_id) IN (…)` with
--     `GROUP BY transaction_id`. Reuse the shared Rust helper
--     `crates/api/src/common/ch.rs::fetch_tx_list_aggregates`.
-- ============================================================================
-- Endpoint:     GET /liquidity-pools/:id/transactions
-- Purpose:      Paginated transactions touching a given pool — deposits,
--               withdrawals, trades. Default: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.14
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :pool_id             FixedString(32)   raw 32-byte pool id
--   $2  :limit               Int               page size
--   $3  :cursor_ledger       Int64             NULL on first page
--   $4  :cursor_tx_id        Int64             NULL on first page
-- Indexes:      operations_appearances ORDER BY (ledger_sequence,
--                 transaction_id, id) + PARTITION BY intDiv. `pool_id` is a
--                 non-leading filter column → bloom filter / full partition
--                 scan; pool-bound predicate selectivity reduces cost.
--               transactions ORDER BY (ledger_sequence, application_order, id).
-- CH Engine:    All Replacing — FINAL.
-- CH Pattern:   same shape as E07 / E10, just with `pool_id` filter.
-- ADR 0044 §:   §4.3 (operations_appearances Replacing partitioned),
--                 §4.2 (transactions), §4.5 (accounts), §5.2 (no closed_at).
-- Notes:
--   • Driver is `operations_appearances` filtered by `pool_id`. Same shape
--     as PG E20.
--   • Cursor drops `created_at` (§5.2). Natural keyset
--     `(ledger_sequence, transaction_id)`.
--   • `operation_types[]` projected for frontend §6.14 trade-vs-LP-mgmt
--     categorization (same as PG); via correlated `groupUniqArray`.
--   • LIMIT BY transaction_id dedupes multi-op txs touching the same pool.

SELECT
    lower(hex(t.hash))                                                                  AS hash_hex,
    t.ledger_sequence,
    src.account_id                                                                      AS source_account,
    t.fee_charged,
    t.successful,
    t.operation_count,
    t.has_soroban,
    (
        SELECT groupUniqArray(oa.type)
        FROM operations_appearances oa FINAL
        WHERE oa.transaction_id = t.id
          AND oa.ledger_sequence = t.ledger_sequence
          AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                                                                   AS operation_types,
    t.id                                                                                AS cursor_tx_id
FROM (
    SELECT oa.transaction_id, oa.ledger_sequence
    FROM operations_appearances oa FINAL
    WHERE oa.pool_id = $1
      AND ($3 IS NULL OR (oa.ledger_sequence, oa.transaction_id) < ($3, $4))
    ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
    LIMIT 1 BY oa.transaction_id
    LIMIT $2 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $2;
