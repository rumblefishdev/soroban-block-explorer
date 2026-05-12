-- Endpoint:     GET /assets/:id/transactions
-- Purpose:      Paginated transactions involving a given asset. Driver is
--               `operations_appearances`, filtered by either
--               `(asset_code, asset_issuer_id)` for classic-form references
--               or by `contract_id` for SAC/Soroban-form references.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.9
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment.
-- Data sources: DB-only.
-- Inputs (variant A — classic identity):
--   $1  :asset_code        String   non-empty
--   $2  :asset_issuer_id   Int64    cityhash64(issuer strkey), non-zero
--   $3  :limit             Int      page size
--   $4  :cursor_ledger     Int64    NULL on first page
--   $5  :cursor_tx_id      Int64    NULL on first page
-- Inputs (variant B — contract identity):
--   $1  :contract_id       Int64    cityhash64(C-StrKey), non-zero
--   $2  :limit             Int      page size
--   $3  :cursor_ledger     Int64    NULL on first page
--   $4  :cursor_tx_id      Int64    NULL on first page
-- Indexes:      operations_appearances ORDER BY (ledger_sequence,
--                 transaction_id, application_order). asset_code /
--                 asset_issuer_id / contract_id are non-leading filter
--                 columns — partition prune via intDiv on cursor's ledger.
--               transactions ORDER BY (ledger_sequence, application_order)
--                 + PARTITION BY intDiv.
--               accounts ORDER BY (account_id) — source join via id.
-- CH Engine:    All Replacing — FINAL required.
-- CH Pattern:   2 variants. PR #175 dropped `assets.id` surrogate, so the
--               API resolves the asset identity at request boundary (via PG
--               or via decomposition of the synthetic cityHash64 surrogate)
--               and passes the matching natural key directly to one variant.
-- ADR 0044 §:   §4.3 (operations_appearances Replacing partitioned),
--               §4.2 (transactions), §4.5 (accounts), §5.2 (no closed_at —
--               partition prune via intDiv).
--   **PR #175 amendment:** removed correlated subquery
--   `(SELECT ... FROM assets FINAL WHERE id = $1)` — the surrogate is gone;
--   the API caller pre-resolves the identity tuple.
-- Notes:
--   • Same two-variant pattern as PG E10. API picks variant from asset_type;
--     SAC has both identities and the API may merge/dedupe by transaction_id.
--   • Native (asset_type=0) has no row-level filter on operations_appearances
--     — out of scope, same as PG.
--   • Cursor tuple `(ledger_sequence, transaction_id)` matches the natural
--     index ordering of `operations_appearances` PK.

-- ============================================================================
-- A. Classic identity path: (asset_code, asset_issuer_id) tuple.
--    Used for asset_type IN (1 = classic_credit, 2 = sac classic-side).
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
        SELECT groupUniqArray(oa2.type)
        FROM operations_appearances oa2 FINAL
        WHERE oa2.transaction_id = t.id
          AND oa2.ledger_sequence = t.ledger_sequence
          AND intDiv(oa2.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                                                                   AS operation_types,
    t.id                                                                                AS cursor_tx_id
FROM (
    SELECT oa.transaction_id, oa.ledger_sequence
    FROM operations_appearances oa FINAL
    WHERE oa.asset_code      = $1
      AND oa.asset_issuer_id = $2
      AND ($4 IS NULL OR (oa.ledger_sequence, oa.transaction_id) < ($4, $5))
    ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
    LIMIT 1 BY oa.transaction_id
    LIMIT $3 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $3;

-- @@ split @@

-- ============================================================================
-- B. Contract identity path: contract_id (Int64 cityhash64 surrogate).
--    Used for asset_type IN (2 = sac contract-side, 3 = soroban-native).
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
        SELECT groupUniqArray(oa2.type)
        FROM operations_appearances oa2 FINAL
        WHERE oa2.transaction_id = t.id
          AND oa2.ledger_sequence = t.ledger_sequence
          AND intDiv(oa2.ledger_sequence, 500000) = intDiv(t.ledger_sequence, 500000)
    )                                                                                   AS operation_types,
    t.id                                                                                AS cursor_tx_id
FROM (
    SELECT oa.transaction_id, oa.ledger_sequence
    FROM operations_appearances oa FINAL
    WHERE oa.contract_id = $1
      AND ($3 IS NULL OR (oa.ledger_sequence, oa.transaction_id) < ($3, $4))
    ORDER BY oa.ledger_sequence DESC, oa.transaction_id DESC
    LIMIT 1 BY oa.transaction_id
    LIMIT $2 * 4
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $2;
