-- Endpoint:     GET /liquidity-pools
-- Purpose:      Paginated list of liquidity pools with their latest
--               on-chain state. Optional filters: asset pair, minimum TVL.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.13
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                     Int      page size
--   $2  :cursor_created_at_ledger  Int64    NULL on first page
--   $3  :cursor_pool_id            String   NULL on first page (hex, optional)
--   $4  :asset_a_code              String   NULL = no filter
--   $5  :asset_a_issuer_strkey     String   NULL = no filter
--   $6  :asset_b_code              String   NULL = no filter
--   $7  :asset_b_issuer_strkey     String   NULL = no filter
--   $8  :min_tvl                   Decimal  NULL = no filter
-- Indexes:      liquidity_pools ORDER BY (pool_id) — full scan + sort here;
--                 the table is small relative to fact tables (one row per
--                 pool created). Acceptable pilot cost.
--               accounts ORDER BY (id) — issuer joins.
--               liquidity_pool_snapshots ORDER BY (pool_id, ledger_sequence, id)
--                 + PARTITION BY intDiv — latest-snapshot lookup is a scalar
--                 subquery with single sparse-PK seek per pool row.
-- CH Engine:    liquidity_pools — MergeTree (no FINAL).
--               liquidity_pool_snapshots — Replacing partitioned (FINAL).
--               accounts — Replacing (FINAL).
-- CH Pattern:   latest-snapshot via scalar subquery per row (CH-idiomatic
--                 alternative to PG's LATERAL); FINAL on Replacing reads;
--                 closed_at via JOIN ledgers for the latest_snapshot_at
--                 display column.
-- ADR 0044 §:   §4.7 (liquidity_pools MergeTree immutable lookup),
--               §4.5 (state Replacing), §4.1 (ledgers), §5.2 (no
--               closed_at on snapshots — fetched via JOIN ledgers).
-- Notes:
--   • PG uses LATERAL for the latest-snapshot lookup. CH-side LATERAL
--     works on 23.x+ but is brittle for correlated state; the scalar-
--     subquery form is more idiomatic and the planner handles it as a
--     per-row sparse-PK seek.
--   • Latest snapshot scalar uses `argMax` on (ledger_sequence) to pick
--     the row with the highest ledger and project all 6 snapshot fields
--     in one pass. Equivalent to PG's `ORDER BY ... DESC LIMIT 1`, just
--     CH-idiomatic.
--   • `latest_snapshot_at` requires `closed_at` (§5.2 — snapshots have no
--     timestamp column). We JOIN `ledgers` ON `latest_snapshot_ledger` to
--     project it.
--   • `asset_type_name` helper unavailable — project raw Int16 and decode
--     in API.
--   • Issuer StrKey filters use scalar subqueries on `accounts FINAL` to
--     resolve (PG used CTE; functionally identical).

SELECT
    lower(hex(lp.pool_id))                                                          AS pool_id_hex,
    lp.asset_a_type                                                                 AS asset_a_type,
    lp.asset_a_code,
    iss_a.account_id                                                                AS asset_a_issuer,
    lp.asset_b_type                                                                 AS asset_b_type,
    lp.asset_b_code,
    iss_b.account_id                                                                AS asset_b_issuer,
    lp.fee_bps,
    toDecimal64(lp.fee_bps, 2) / 100                                                AS fee_percent,
    lp.created_at_ledger,
    s.latest_ledger_sequence                                                        AS latest_snapshot_ledger,
    s.reserve_a,
    s.reserve_b,
    s.total_shares,
    s.tvl,
    s.volume,
    s.fee_revenue,
    l_snap.closed_at                                                                AS latest_snapshot_at
FROM liquidity_pools lp
LEFT JOIN accounts iss_a FINAL ON iss_a.id = lp.asset_a_issuer_id
LEFT JOIN accounts iss_b FINAL ON iss_b.id = lp.asset_b_issuer_id
-- Latest snapshot per pool via per-pool group + argMax. Column alias is
-- `latest_ledger_sequence` (not `ledger_sequence`) to avoid CH's aggregate-
-- inside-aggregate alias collision (the `ledger_sequence` used inside the
-- other argMax calls would otherwise be ambiguous with the outer alias).
LEFT JOIN (
    SELECT
        pool_id,
        max(ledger_sequence)                      AS latest_ledger_sequence,
        argMax(reserve_a,        ledger_sequence) AS reserve_a,
        argMax(reserve_b,        ledger_sequence) AS reserve_b,
        argMax(total_shares,     ledger_sequence) AS total_shares,
        argMax(tvl,              ledger_sequence) AS tvl,
        argMax(volume,           ledger_sequence) AS volume,
        argMax(fee_revenue,      ledger_sequence) AS fee_revenue
    FROM liquidity_pool_snapshots FINAL
    GROUP BY pool_id
) s ON s.pool_id = lp.pool_id
LEFT JOIN ledgers l_snap ON l_snap.sequence = s.latest_ledger_sequence
WHERE
    ($2 IS NULL OR (lp.created_at_ledger, lower(hex(lp.pool_id))) < ($2, $3))
    AND ($4 IS NULL OR lp.asset_a_code = $4)
    AND ($5 IS NULL OR lp.asset_a_issuer_id = (SELECT id FROM accounts FINAL WHERE account_id = $5 LIMIT 1))
    AND ($6 IS NULL OR lp.asset_b_code = $6)
    AND ($7 IS NULL OR lp.asset_b_issuer_id = (SELECT id FROM accounts FINAL WHERE account_id = $7 LIMIT 1))
    AND ($8 IS NULL OR s.tvl >= $8)
ORDER BY lp.created_at_ledger DESC, lp.pool_id DESC
LIMIT $1;
