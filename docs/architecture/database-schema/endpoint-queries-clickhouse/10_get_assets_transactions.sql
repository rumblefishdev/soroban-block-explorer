-- Endpoint:     GET /assets/:id/transactions
-- Purpose:      Paginated transactions involving a given asset. The driver
--               table is `operations_appearances`, filtered by either
--               (asset_code, asset_issuer_id) for classic-form references
--               or by `contract_id` for SAC/Soroban-form references.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.9
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :asset_id            Int32     assets surrogate id
--   $2  :limit               Int       page size
--   $3  :cursor_ledger       Int64     NULL on first page
--   $4  :cursor_tx_id        Int64     NULL on first page
-- Indexes:      assets ORDER BY (id) — FINAL'd resolve.
--               operations_appearances ORDER BY (ledger_sequence, transaction_id, id)
--                 + PARTITION BY intDiv(ledger_sequence, 500000). Scan filters
--                 on (asset_code, asset_issuer_id) [variant A] or contract_id
--                 [variant B] which are non-leading columns — bloom filter or
--                 full partition scan; acceptable cost for the pilot.
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 + PARTITION BY intDiv.
-- CH Engine:    All Replacing — FINAL required.
-- CH Pattern:   2 variants (classic identity / contract identity), same shape
--               as PG E10. Cursor drops `created_at` (§5.2) — natural keyset
--               becomes (ledger_sequence, transaction_id). operation_types[]
--               via correlated groupUniqArray subquery.
-- ADR 0044 §:   §4.3 (operations_appearances Replacing partitioned),
--               §4.2 (transactions), §4.5 (accounts), §5.2 (no closed_at).
-- Notes:
--   • Same two-variant pattern as PG. The API picks one based on asset_type;
--     SAC has both identities and the API merges/dedupes by transaction_id.
--   • Native (asset_type=0) has no row-level filter on operations_appearances
--     — out of scope, same as PG.
--   • PG uses `DISTINCT ON (created_at, transaction_id)` to dedup multi-op
--     tx; CH uses `GROUP BY` + `argMin/argMax` patterns or `LIMIT 1 BY`.
--     Here we keep it simple with a subquery `LIMIT BY (transaction_id)`
--     that picks one row per tx ordered by (ledger_sequence DESC, id ASC).
--   • Cursor tuple `(ledger_sequence, transaction_id)` drops PG's
--     `created_at` term (§5.2). Same semantic page boundary because
--     `(ledger_sequence, transaction_id)` is monotone-with-time per Stellar.

-- ============================================================================
-- A. Classic identity path: assets with (asset_code, issuer_id).
--    asset_type IN (1 = classic_credit, 2 = sac).
-- ============================================================================
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
    SELECT
        oa.transaction_id,
        oa.ledger_sequence
    FROM operations_appearances oa FINAL
    WHERE oa.asset_code      = (SELECT asset_code FROM assets FINAL WHERE id = $1 LIMIT 1)
      AND oa.asset_issuer_id = (SELECT issuer_id  FROM assets FINAL WHERE id = $1 LIMIT 1)
      AND oa.asset_code IS NOT NULL
      AND ($3 IS NULL OR (oa.ledger_sequence, oa.transaction_id) < ($3, $4))
    ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
    LIMIT 1 BY oa.transaction_id
    LIMIT $2 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $2;

-- @@ split @@

-- ============================================================================
-- B. Contract identity path: assets with contract_id.
--    asset_type IN (2 = sac, 3 = soroban_native).
-- ============================================================================
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
    SELECT
        oa.transaction_id,
        oa.ledger_sequence
    FROM operations_appearances oa FINAL
    WHERE oa.contract_id = (SELECT contract_id FROM assets FINAL WHERE id = $1 LIMIT 1)
      AND oa.contract_id IS NOT NULL
      AND ($3 IS NULL OR (oa.ledger_sequence, oa.transaction_id) < ($3, $4))
    ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
    LIMIT 1 BY oa.transaction_id
    LIMIT $2 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $2;
