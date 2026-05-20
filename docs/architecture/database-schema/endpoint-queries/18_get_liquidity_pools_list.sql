-- Endpoint:     GET /liquidity-pools
-- Purpose:      Paginated list of liquidity pools with their latest
--               on-chain state (reserves, total shares, TVL). Optional
--               filters: asset pair, minimum TVL.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.13
-- Schema:       ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                    INT            page size
--   $2  :cursor_created_at_ledger BIGINT         NULL on first page
--   $3  :cursor_pool_id           BYTEA(32)      NULL on first page
--   $4  :asset_a_code             VARCHAR        NULL = no filter
--   $5  :asset_a_issuer_strkey    VARCHAR(56)    NULL = no filter
--   $6  :asset_b_code             VARCHAR        NULL = no filter
--   $7  :asset_b_issuer_strkey    VARCHAR(56)    NULL = no filter
--   $8  :min_tvl                  NUMERIC(28,7)  NULL = no filter
--   $9  :asset_code               VARCHAR        NULL = no filter
--                                                (uppercased + trimmed
--                                                 at API boundary; task 0246)
-- Indexes:      idx_pools_asset_a / idx_pools_asset_b (per-leg asset filters),
--               idx_pools_created_at_ledger ON (created_at_ledger DESC, pool_id DESC)
--                  — exact keyset walk; added in task 0132 migration
--                  `20260428000100_add_endpoint_query_indexes`,
--               idx_lps_pool ON (pool_id, created_at DESC) — for the
--                  latest-snapshot lateral lookup,
--               idx_lpp_shares (pool_id, shares DESC) WHERE shares > 0
--                  — partial index covering the per-row participant count
--                  subquery (task 0246).
-- Notes:
--   • Default ordering is `(created_at_ledger DESC, pool_id DESC)`: newest
--     pools first, deterministic on tie. We deliberately do NOT order by
--     latest-snapshot TVL — that field can be NULL (TVL ingestion is a
--     future task) and would force a NULLS-LAST cursor that is hard to
--     keep keyset-stable. TVL is still surfaced and filterable; the caller
--     can sort client-side within a page or the endpoint can be expanded
--     with an explicit `?sort=tvl` once TVL is populated.
--   • Latest snapshot per pool is fetched via a LATERAL with `LIMIT 1`,
--     no time-bound predicate. Pool reserves/total_shares only change on
--     deposit/withdraw/swap events (snapshot triggers are state-change
--     driven — see `xdr_parser::extract_liquidity_pools`), so the latest
--     snapshot is always the actual current on-chain state regardless of
--     age. Clients that care about staleness can read `latest_snapshot_at`
--     in the response. (`tvl`/`volume`/`fee_revenue` are populated by a
--     future TVL-ingestion task; today they are NULL on every snapshot.)
--   • Asset-leg filter accepts native (`code IS NULL` / `issuer IS NULL`)
--     by leaving both code and issuer params NULL, OR explicit classic
--     identity (both non-NULL). Mixed (one NULL one not) is undefined —
--     the API validates inputs upstream.
--   • Single-asset filter (`$9`, task 0246) coexists additively with the
--     per-leg filters. The handler trims + uppercases the caller input
--     before binding; the column side applies `UPPER(...)` symmetrically
--     so mixed-case stored codes still match. The two `idx_pools_asset_*`
--     btree indexes are on the raw column, so this clause does not seek;
--     acceptable because the planner can still use the cursor / per-leg
--     filters when present, and the pool table is small (≈10⁴ rows on
--     Stellar pubnet).
--   • Issuer StrKeys resolve via a CTE with the `accounts.account_id`
--     UNIQUE index, then are surfaced via final joins.
--   • `participant_count` (task 0246) is a correlated subquery hitting
--     the partial index `idx_lpp_shares (pool_id, shares DESC) WHERE
--     shares > 0`. Not snapshot-bound — populated even on stale pools.
--     N×1 index seeks per page (limit + 1); on hot pools with many
--     LP positions this can dominate page latency — benchmark before
--     production scale-out, consider a cached column if it bites.
--   • Sentinel placeholder pools (ADR 0041 / task 0193) are excluded
--     via `lp.created_at_ledger > 0`. The persist layer emits these
--     rows (`created_at_ledger = 0`, NULL/0 asset/fee fields) to
--     satisfy the `lp_positions.pool_id` FK during partial backfills
--     when the parent pool was created in a pre-window ledger and
--     never touched in the current one; they self-heal on the next
--     ledger touch via the 13a UPSERT. The list endpoint hides them
--     until they carry real data. Pubnet genesis seq is 1, so `> 0`
--     excludes every sentinel without rejecting any real pool.

WITH issuer_a AS (
    SELECT id FROM accounts WHERE $5::varchar IS NOT NULL AND account_id = $5
),
issuer_b AS (
    SELECT id FROM accounts WHERE $7::varchar IS NOT NULL AND account_id = $7
)
SELECT
    encode(lp.pool_id, 'hex')           AS pool_id_hex,
    asset_type_name(lp.asset_a_type)    AS asset_a_type_name,
    lp.asset_a_type                     AS asset_a_type,
    lp.asset_a_code,
    iss_a.account_id                    AS asset_a_issuer,
    asset_type_name(lp.asset_b_type)    AS asset_b_type_name,
    lp.asset_b_type                     AS asset_b_type,
    lp.asset_b_code,
    iss_b.account_id                    AS asset_b_issuer,
    lp.fee_bps,
    -- Frontend §6.13 shows "fee percentage" (e.g. 0.30 %).
    -- DB stores basis points; conversion is here, not on the client.
    (lp.fee_bps::numeric / 100)         AS fee_percent,
    lp.created_at_ledger,
    -- Task 0246: active LP count per pool. Correlated subquery on
    -- `idx_lpp_shares` partial index. Not snapshot-bound.
    (SELECT COUNT(*) FROM lp_positions lpp
      WHERE lpp.pool_id = lp.pool_id AND lpp.shares > 0)
                                        AS participant_count,
    s.ledger_sequence                   AS latest_snapshot_ledger,
    s.reserve_a,
    s.reserve_b,
    s.total_shares,
    s.tvl,
    s.volume,
    s.fee_revenue,
    s.created_at                        AS latest_snapshot_at
FROM liquidity_pools lp
LEFT JOIN accounts iss_a ON iss_a.id = lp.asset_a_issuer_id
LEFT JOIN accounts iss_b ON iss_b.id = lp.asset_b_issuer_id
LEFT JOIN LATERAL (
    SELECT
        lps.ledger_sequence,
        lps.reserve_a,
        lps.reserve_b,
        lps.total_shares,
        lps.tvl,
        lps.volume,
        lps.fee_revenue,
        lps.created_at
    FROM liquidity_pool_snapshots lps
    WHERE lps.pool_id = lp.pool_id
    ORDER BY lps.created_at DESC, lps.ledger_sequence DESC
    LIMIT 1
) s ON TRUE
WHERE
    -- Sentinel filter (ADR 0041 / task 0193).
    lp.created_at_ledger > 0
    AND ($2::bigint IS NULL OR (lp.created_at_ledger, lp.pool_id) < ($2, $3))
    AND ($4::varchar IS NULL OR lp.asset_a_code = $4)
    AND ($5::varchar IS NULL OR lp.asset_a_issuer_id = (SELECT id FROM issuer_a))
    AND ($6::varchar IS NULL OR lp.asset_b_code = $6)
    AND ($7::varchar IS NULL OR lp.asset_b_issuer_id = (SELECT id FROM issuer_b))
    AND ($8::numeric IS NULL OR s.tvl >= $8)
    -- Single-asset filter (task 0246). `$9` is uppercased + trimmed at
    -- the API boundary; `UPPER(...)` on the column side covers
    -- mixed-case stored codes.
    AND ($9::varchar IS NULL
         OR UPPER(lp.asset_a_code) = $9
         OR UPPER(lp.asset_b_code) = $9)
ORDER BY lp.created_at_ledger DESC, lp.pool_id DESC
LIMIT $1;
