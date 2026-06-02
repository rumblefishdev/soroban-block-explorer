-- Endpoint:     GET /liquidity-pools
-- Purpose:      Paginated list of liquidity pools with their latest
--               on-chain state. Optional filters: asset pair, minimum TVL.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.13
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment.
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                          Int     page size
--   $2  :cursor_last_updated_ledger     Int64   NULL on first page
--   $3  :cursor_pool_id                 String  NULL on first page (hex, optional)
--   $4  :asset_a_code                   String  NULL = no filter (empty for native)
--   $5  :asset_a_issuer_strkey          String  NULL = no filter (resolved to issuer_id)
--   $6  :asset_b_code                   String  NULL = no filter
--   $7  :asset_b_issuer_strkey          String  NULL = no filter
--   $8  :min_tvl                        Decimal NULL = no filter
-- Indexes:      liquidity_pools ORDER BY (pool_id) — full scan + sort here;
--                 the table is small relative to fact tables.
--               accounts ORDER BY (account_id) — issuer joins by id.
--               liquidity_pool_snapshots ORDER BY (pool_id, ledger_sequence)
--                 + intDiv partition — latest-snapshot lookup via argMax GROUP BY.
--               ledgers ORDER BY (sequence) — closed_at JOIN for display.
-- CH Engine:    liquidity_pools — Replacing(last_updated_ledger) (FINAL).
--               liquidity_pool_snapshots — Replacing partitioned (FINAL).
--               accounts — Replacing (FINAL).
-- CH Pattern:   FINAL on Replacing reads; latest-snapshot via argMax + GROUP BY
--               subquery joined to lp on pool_id; closed_at via JOIN ledgers.
-- ADR 0044 §:   §4.7 (liquidity_pools state Replacing — engine swap per PR
--                 #175 from plain MergeTree to Replacing(last_updated_ledger)),
--                 §4.5 (state Replacing), §4.1 (ledgers MergeTree).
--   **PR #175 amendment:** `liquidity_pools.created_at_ledger` dropped;
--   ordering now uses `last_updated_ledger` (RMT version slot). Pool's
--   true creation ledger is derivable as
--   `MIN(ledger_sequence) FROM liquidity_pool_snapshots GROUP BY pool_id`
--   if needed for display — currently not projected (frontend defaults
--   to "most recently active" ordering, which `last_updated_ledger`
--   naturally provides).
-- Notes:
--   • Cursor ordering switched from `created_at_ledger DESC` to
--     `last_updated_ledger DESC`. UI label changes from "newest pools
--     first" to "most recently active first" — different semantic but
--     a more useful default for users browsing active LPs.
--   • argMax over GROUP BY rather than correlated scalar — CH 26.x
--     rejects correlated subqueries with ORDER BY/LIMIT in JOIN.
--   • Sentinel `issuer_id=0` for native: LEFT JOIN gated by `!= 0`.

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
    lp.last_updated_ledger                                                          AS last_updated_ledger,
    s.latest_ledger_sequence                                                        AS latest_snapshot_ledger,
    s.reserve_a,
    s.reserve_b,
    s.total_shares,
    s.tvl,
    s.volume,
    s.fee_revenue,
    l_snap.closed_at                                                                AS latest_snapshot_at
FROM liquidity_pools lp FINAL
LEFT JOIN accounts iss_a FINAL ON iss_a.id = lp.asset_a_issuer_id AND lp.asset_a_issuer_id != 0
LEFT JOIN accounts iss_b FINAL ON iss_b.id = lp.asset_b_issuer_id AND lp.asset_b_issuer_id != 0
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
    ($2 IS NULL OR (lp.last_updated_ledger, lower(hex(lp.pool_id))) < ($2, $3))
    AND ($4 IS NULL OR lp.asset_a_code = $4)
    AND ($5 IS NULL OR lp.asset_a_issuer_id = (SELECT id FROM accounts FINAL WHERE account_id = $5 LIMIT 1))
    AND ($6 IS NULL OR lp.asset_b_code = $6)
    AND ($7 IS NULL OR lp.asset_b_issuer_id = (SELECT id FROM accounts FINAL WHERE account_id = $7 LIMIT 1))
    AND ($8 IS NULL OR s.tvl >= $8)
ORDER BY lp.last_updated_ledger DESC, lp.pool_id DESC
LIMIT $1;
