-- Endpoint:     GET /transactions
-- Purpose:      Paginated list of transactions. Optional filters:
--               source_account, contract_id, operation_type.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.3
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                Int          page size
--   $2  :cursor_ledger        Int64        NULL on first page
--   $3  :cursor_tx_id         Int64        NULL on first page
--   $4  :source_account_id    Int64        NULL = no filter (resolved by API)
--   $5  :contract_id          Int64        NULL = no filter (statement B path)
--   $6  :op_type              Int16        NULL = no filter (statement C path)
-- Indexes:      transactions ORDER BY (ledger_sequence, application_order, id)
--                 + PARTITION BY intDiv(ledger_sequence, 500000).
--               operations_appearances ORDER BY (ledger_sequence, transaction_id, id)
--                 — used for contract / op_type filter scans (statement B/C).
--               soroban_invocations_appearances ORDER BY (contract_id, ledger_sequence, transaction_id)
--                 — used in statement B contract UNION.
--               soroban_events ORDER BY (contract_id, ledger_sequence, transaction_id, event_index)
--                 — used in statement B contract UNION (replaces PG soroban_events_appearances per §5.1).
--               accounts ORDER BY (id) — source/account StrKey joins.
--               soroban_contracts ORDER BY (id) — contract_ids[] resolve.
-- CH Engine:    All Replacing — FINAL required.
-- CH Pattern:   3 statements (A no filter / B contract filter / C op_type filter)
--                 mirroring PG E02. Cursor uses (ledger_sequence, id) per §5.2.
--                 `operation_types[]` and `contract_ids[]` via correlated
--                 subqueries returning groupUniqArray (CH equivalent of
--                 PG's array_agg DISTINCT pattern).
-- ADR 0044 §:   §4.2 (transactions), §4.3 (operations_appearances,
--                 soroban_invocations_appearances), §4.4 (soroban_events
--                 — §5.1 full-content table replaces PG's appearance index),
--                 §4.5 (state Replacing), §5.2 (no created_at — partition prune
--                 via intDiv).
-- Notes:
--   • **§5.1 in E02 contract UNION:** PG's statement B unions
--     `operations_appearances + soroban_invocations_appearances +
--     soroban_events_appearances`. CH-side, the third table is `soroban_events`
--     (the full table, not an appearance index). The UNION shape is identical
--     for the purpose of "any tx that touched contract $5" — both tables
--     carry `(contract_id, transaction_id, ledger_sequence)`.
--   • Cursor `(ledger_sequence, transaction_id) < ($2, $3)` — Stellar
--     monotonicity means this is equivalent to PG's `(created_at, id)`
--     cursor. The API maintains the same opaque base64 cursor format.
--   • PG's `LATERAL (... LIMIT 1)` for `operation_types[]` becomes a
--     correlated scalar subquery returning `Array(Int16)` via
--     `groupUniqArray()`. CH 26.x handles this as a per-row sparse-PK
--     seek into the partition the row already came from.
--   • `contract_ids[]` projection: same UNION as the filter, FINAL'd join
--     to `soroban_contracts` for the C-StrKey display values.

-- ============================================================================
-- Statement A — no contract / op_type filter (default path)
-- ============================================================================
SELECT
    lower(hex(t.hash))                              AS hash_hex,
    t.ledger_sequence,
    t.application_order,
    src.account_id                                  AS source_account,
    t.fee_charged,
    lower(hex(t.inner_tx_hash))                     AS inner_tx_hash_hex,
    t.successful,
    t.operation_count,
    t.has_soroban,
    (
        SELECT groupUniqArray(oa.type)
        FROM operations_appearances oa FINAL
        WHERE oa.transaction_id = t.id
          AND oa.ledger_sequence = t.ledger_sequence
          AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                               AS operation_types,
    -- Returns Array(Int64) of contract surrogate ids; the API resolves
    -- ids → C-StrKeys via a batched `IN` lookup on `soroban_contracts`.
    -- CH 26.x does NOT support correlated subqueries with DISTINCT/UNION
    -- DISTINCT (NOT_IMPLEMENTED). We work around by:
    --   (a) using UNION ALL (no dedup) in the correlated branches, and
    --   (b) letting outer `groupUniqArray` dedup at the row level.
    -- Each branch is a single sparse-PK seek under the partition predicate,
    -- so duplicate rows are bounded (~3× the underlying count).
    arrayDistinct(arrayConcat(
        (SELECT groupArray(contract_id)
         FROM operations_appearances FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence AND contract_id IS NOT NULL
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)),
        (SELECT groupArray(contract_id)
         FROM soroban_invocations_appearances FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)),
        (SELECT groupArray(contract_id)
         FROM soroban_events FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000))
    ))                                              AS contract_surrogate_ids,
    t.id                                            AS cursor_id
FROM transactions t FINAL
JOIN accounts src FINAL ON src.id = t.source_id
WHERE
    ($2 IS NULL OR (t.ledger_sequence, t.id) < ($2, $3))
    AND ($4 IS NULL OR t.source_id = $4)
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $1;

-- @@ split @@

-- ============================================================================
-- Statement B — contract filter set (with or without op_type)
-- ============================================================================
-- Build a small candidate set from the 3-table UNION (broad contract match),
-- then PK-join to transactions, apply optional op_type post-EXISTS, project.
SELECT
    lower(hex(t.hash))                              AS hash_hex,
    t.ledger_sequence,
    t.application_order,
    src.account_id                                  AS source_account,
    t.fee_charged,
    lower(hex(t.inner_tx_hash))                     AS inner_tx_hash_hex,
    t.successful,
    t.operation_count,
    t.has_soroban,
    (
        SELECT groupUniqArray(oa.type)
        FROM operations_appearances oa FINAL
        WHERE oa.transaction_id = t.id AND oa.ledger_sequence = t.ledger_sequence
          AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                               AS operation_types,
    -- Returns Array(Int64) of contract surrogate ids; the API resolves
    -- ids → C-StrKeys via a batched `IN` lookup on `soroban_contracts`.
    -- CH 26.x does NOT support correlated subqueries with DISTINCT/UNION
    -- DISTINCT (NOT_IMPLEMENTED). We work around by:
    --   (a) using UNION ALL (no dedup) in the correlated branches, and
    --   (b) letting outer `groupUniqArray` dedup at the row level.
    -- Each branch is a single sparse-PK seek under the partition predicate,
    -- so duplicate rows are bounded (~3× the underlying count).
    arrayDistinct(arrayConcat(
        (SELECT groupArray(contract_id)
         FROM operations_appearances FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence AND contract_id IS NOT NULL
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)),
        (SELECT groupArray(contract_id)
         FROM soroban_invocations_appearances FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)),
        (SELECT groupArray(contract_id)
         FROM soroban_events FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000))
    ))                                              AS contract_surrogate_ids,
    t.id                                            AS cursor_id
FROM (
    SELECT DISTINCT ledger_sequence, transaction_id
    FROM (
        SELECT ledger_sequence, transaction_id FROM operations_appearances FINAL
        WHERE contract_id = $5
          AND ($2 IS NULL OR (ledger_sequence, transaction_id) < ($2, $3))
        UNION DISTINCT
        SELECT ledger_sequence, transaction_id FROM soroban_invocations_appearances FINAL
        WHERE contract_id = $5
          AND ($2 IS NULL OR (ledger_sequence, transaction_id) < ($2, $3))
        UNION DISTINCT
        SELECT ledger_sequence, transaction_id FROM soroban_events FINAL
        WHERE contract_id = $5
          AND ($2 IS NULL OR (ledger_sequence, transaction_id) < ($2, $3))
    ) u
    ORDER BY ledger_sequence DESC, transaction_id DESC
    LIMIT $1 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
WHERE
    ($4 IS NULL OR t.source_id = $4)
    AND ($6 IS NULL OR (
        SELECT count() FROM operations_appearances oa2 FINAL
        WHERE oa2.transaction_id = t.id AND oa2.ledger_sequence = t.ledger_sequence AND oa2.type = $6
          AND intDiv(oa2.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    ) > 0)
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $1;

-- @@ split @@

-- ============================================================================
-- Statement C — op_type filter only (no contract filter)
-- ============================================================================
SELECT
    lower(hex(t.hash))                              AS hash_hex,
    t.ledger_sequence,
    t.application_order,
    src.account_id                                  AS source_account,
    t.fee_charged,
    lower(hex(t.inner_tx_hash))                     AS inner_tx_hash_hex,
    t.successful,
    t.operation_count,
    t.has_soroban,
    (
        SELECT groupUniqArray(oa.type)
        FROM operations_appearances oa FINAL
        WHERE oa.transaction_id = t.id AND oa.ledger_sequence = t.ledger_sequence
          AND intDiv(oa.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                               AS operation_types,
    -- Returns Array(Int64) of contract surrogate ids; the API resolves
    -- ids → C-StrKeys via a batched `IN` lookup on `soroban_contracts`.
    -- CH 26.x does NOT support correlated subqueries with DISTINCT/UNION
    -- DISTINCT (NOT_IMPLEMENTED). We work around by:
    --   (a) using UNION ALL (no dedup) in the correlated branches, and
    --   (b) letting outer `groupUniqArray` dedup at the row level.
    -- Each branch is a single sparse-PK seek under the partition predicate,
    -- so duplicate rows are bounded (~3× the underlying count).
    arrayDistinct(arrayConcat(
        (SELECT groupArray(contract_id)
         FROM operations_appearances FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence AND contract_id IS NOT NULL
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)),
        (SELECT groupArray(contract_id)
         FROM soroban_invocations_appearances FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)),
        (SELECT groupArray(contract_id)
         FROM soroban_events FINAL
         WHERE transaction_id = t.id AND ledger_sequence = t.ledger_sequence
           AND intDiv(ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000))
    ))                                              AS contract_surrogate_ids,
    t.id                                            AS cursor_id
FROM (
    SELECT DISTINCT transaction_id, ledger_sequence
    FROM operations_appearances FINAL
    WHERE type = $6
      AND ($2 IS NULL OR (ledger_sequence, transaction_id) < ($2, $3))
    ORDER BY ledger_sequence DESC, transaction_id DESC
    LIMIT $1 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
WHERE
    ($4 IS NULL OR t.source_id = $4)
    AND ($2 IS NULL OR (t.ledger_sequence, t.id) < ($2, $3))
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $1;
