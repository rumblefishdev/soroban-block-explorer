-- ============================================================================
-- ⚠️  `contract_ids` REMOVED from the API (task 0386) — the shared helper now
--     returns `operation_types` only; the `contract_ids` array below is dead.
-- ⚠️  CH 26.3 CORRECTION (task 0243) — do NOT implement the
--     `operation_types` array with the correlated scalar
--     subqueries shown below (`… WHERE oa.transaction_id = t.id`): ClickHouse
--     26.3 rejects them with `Code: 48 NOT_IMPLEMENTED`. Use the NON-correlated
--     two-step the shipped modules use — fetch the page of tx keys, then
--     aggregate per `(ledger_sequence, transaction_id) IN (…)` with
--     `GROUP BY transaction_id`. Reuse the shared Rust helper
--     `crates/api/src/common/ch.rs::fetch_tx_list_aggregates`.
-- ⚠️  AMOUNTS (task 0279 / issue #371) — the shipped module runs one more
--     bounded step after the page keys, so each row can carry what it moved
--     through THIS pool. Not expressible in the single statement below:
--
--       SELECT transaction_id AS tid, application_order AS ord, asset_id,
--              any(amount) AS amount
--       FROM lp_operation_amounts
--       WHERE pool_id = $1
--         AND (ledger_sequence, transaction_id) IN (<page keys>)
--         AND intDiv(ledger_sequence, 500000) IN (<page partitions>)
--       GROUP BY tid, ord, asset_id
--
--     One row PER OPERATION, never summed per transaction: 8.2% of (pool, tx)
--     pairs run more than one operation against the same pool (measured on
--     prod 2026-08-12 over 8.49M pairs), and a sum across a bundled deposit +
--     path payment describes neither — it is smaller than the deposit and can
--     flip the sign shape, landing under an Event chip that does not match it.
--     The handler groups the rows per transaction; the frontend renders one
--     line each.
--     The GROUP BY dedups the ReplacingMergeTree by its full ORDER BY key —
--     `any()` is exact because duplicates of a key are byte-identical by
--     construction (one reducer, shared by live ingest and the re-parse).
--     `FINAL` would merge whole parts to answer a bounded seek.
--     `asset_id` maps onto the pool's two legs via `ids::asset_id` over
--     `liquidity_pools.asset_{a,b}_{type,code,issuer_id}` — resolved in Rust,
--     since CH's builtin `cityHash64` is NOT the surrogate's algorithm.
--     A transaction with no rows sends an empty list — the frontend renders
--     blank, never `0` — for history the backfill has not reached.
-- ============================================================================
-- Endpoint:     GET /liquidity-pools/:id/transactions
-- Purpose:      Paginated transactions touching a given pool — deposits,
--               withdrawals, trades. Default: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.14
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :pool_id             FixedString(32)   raw 32-byte pool id
--                            (seeked via operation_pools.pool_id — task 0365)
--   $2  :limit               Int               page size
--   $3  :cursor_ledger       Int64             NULL on first page
--   $4  :cursor_tx_id        Int64             NULL on first page
-- Indexes:      operation_pools ORDER BY (pool_id, ledger_sequence,
--                 transaction_id) + PARTITION BY intDiv (task 0365). `pool_id`
--                 is the leading key → STEP 1 is a direct PK prefix-seek bounded
--                 to the pool's own rows, superseding the 0281-C read-in-order
--                 scan over operations_appearances (a popular pool sat in ~every
--                 granule; the has(pool_ids, X) Array filter could not prune).
--               transactions ORDER BY (ledger_sequence, application_order, id).
-- CH Engine:    All Replacing — FINAL (operation_pools deduped via LIMIT 1 BY).
-- CH Pattern:   same shape as E07 (account seek) / E10 (asset seek): a per-entity
--               presence companion keyed entity-first, then id-IN hydration.
-- ADR 0044 §:   §4.3 (operations_appearances Replacing partitioned),
--                 §4.2 (transactions), §4.5 (accounts), §5.2 (no closed_at).
-- Notes:
--   • Driver is `operation_pools` seeked by `pool_id = $1` (task 0365: the
--     pool-keyed presence twin of transaction_participants, written by the
--     indexer as a per-op fan-out over pool_ids — path payments contribute every
--     pool crossed by their claim atoms). Same tx set as PG E20.
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
    SELECT transaction_id, ledger_sequence
    FROM operation_pools
    WHERE pool_id = $1
      AND ($3 IS NULL OR (ledger_sequence, transaction_id) < ($3, $4))
    ORDER BY ledger_sequence DESC, transaction_id DESC
    LIMIT 1 BY ledger_sequence, transaction_id
    LIMIT $2
) m
JOIN transactions t FINAL ON t.id = m.transaction_id AND t.ledger_sequence = m.ledger_sequence
JOIN accounts src FINAL ON src.id = t.source_id
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $2;
