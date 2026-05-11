-- Endpoint:     GET /liquidity-pools/:id
-- Purpose:      Pool detail: identity (asset pair, fee, created_at_ledger) +
--               latest on-chain state (reserves, total shares, TVL).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.14
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :pool_id  FixedString(32)  raw 32-byte pool id (or hex via unhex())
-- Indexes:      liquidity_pools ORDER BY (pool_id) — direct PK seek.
--               liquidity_pool_snapshots ORDER BY (pool_id, ledger_sequence, id)
--                 + PARTITION BY intDiv.
--               accounts ORDER BY (id) — issuer joins.
--               ledgers ORDER BY (sequence) — closed_at JOIN.
-- CH Engine:    liquidity_pools — MergeTree (no FINAL).
--               liquidity_pool_snapshots — Replacing partitioned (FINAL).
--               accounts — Replacing (FINAL).
--               ledgers — MergeTree (no FINAL).
-- CH Pattern:   single-row variant of E18; latest snapshot via argMax over
--                 a pool-pinned scan; closed_at via JOIN ledgers.
-- ADR 0044 §:   §4.7 (liquidity_pools MergeTree), §4.5 (state Replacing),
--                 §4.1 (ledgers), §5.2 (closed_at via JOIN ledgers).
-- Notes:
--   • Single pool → latest snapshot is a scalar argMax over a pool-pinned
--     scan, exactly one sparse-PK granule per partition.
--   • No freshness window: state is correct regardless of age (same
--     reasoning as PG E19).
--   • `latest_snapshot_at` requires JOIN to `ledgers` for `closed_at`
--     (§5.2 dropped this column from snapshots).
--   • `asset_type_name` PG helper not available — project raw Int16.

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
LEFT JOIN (
    SELECT
        max(ledger_sequence)                      AS latest_ledger_sequence,
        argMax(reserve_a,        ledger_sequence) AS reserve_a,
        argMax(reserve_b,        ledger_sequence) AS reserve_b,
        argMax(total_shares,     ledger_sequence) AS total_shares,
        argMax(tvl,              ledger_sequence) AS tvl,
        argMax(volume,           ledger_sequence) AS volume,
        argMax(fee_revenue,      ledger_sequence) AS fee_revenue
    FROM liquidity_pool_snapshots FINAL
    WHERE pool_id = $1
) s ON 1=1
LEFT JOIN ledgers l_snap ON l_snap.sequence = s.latest_ledger_sequence
WHERE lp.pool_id = $1;
