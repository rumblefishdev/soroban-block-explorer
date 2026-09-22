-- Endpoint:     GET /liquidity-pools/:id/participants
-- Purpose:      Paginated list of liquidity providers in a pool, ordered by
--               share size descending. Powers the "Pool participants" table
--               on the LP detail page (frontend §6.14).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.14
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037 §16
-- Data sources: DB-only.
-- Inputs:
--   $1  :pool_id              FixedString(32)  raw 32-byte pool id
--   $2  :limit                Int              page size
--   $3  :cursor_shares        Decimal          NULL on first page
--   $4  :cursor_account_id    Int64            NULL on first page
-- Indexes:      lp_positions ORDER BY (pool_id, account_id) + FINAL. Pool-
--                 leading equality + FINAL deduplication; the (shares DESC)
--                 sort happens at scan time post-dedup.
--               liquidity_pool_snapshots ORDER BY (pool_id, ledger_sequence, id)
--                 + intDiv partition — latest-snapshot lookup for total_shares.
--               accounts ORDER BY (id) — StrKey join.
-- CH Engine:    lp_positions — Replacing(last_updated_ledger) (FINAL).
--                 liquidity_pool_snapshots — Replacing partitioned (FINAL).
--                 accounts — Replacing (FINAL).
-- CH Pattern:   FINAL on all reads; latest-snapshot total_shares via scalar
--                 subquery (one row, pool-bound, argMax over ledger_sequence);
--                 keyset cursor on (shares DESC, account_id DESC).
-- ADR 0044 §:   §4.5 (Replacing state tables), §5.2 (no created_at — the
--                 freshness window in PG E23 is replaced by ledger-window
--                 if needed; here we just take the latest snapshot
--                 unconditionally because pool state is event-driven, not
--                 wall-clock-driven).
-- Notes:
--   • PG E23 has a 7-day freshness window on `liquidity_pool_snapshots` —
--     "if no snapshot in last 7 days, share_percentage = NULL". CH-side
--     we drop the window because (a) snapshots are state-change-driven so
--     "latest" is always correct, (b) closed_at is on ledgers not snapshots
--     so the JOIN cost would be wasted, (c) NULL behaviour falls out
--     naturally when no snapshots exist for the pool.
--   • `share_percentage = shares * 100 / latest.total_shares`, NULL when
--     `total_shares` is NULL or 0. Same semantic as PG.
--   • Cursor on (shares DESC, account_id DESC); shares can repeat across
--     accounts so account_id is the tiebreaker. Same shape as PG.

WITH latest_snap AS (
    SELECT
        argMax(total_shares, ledger_sequence)   AS total_shares
    FROM liquidity_pool_snapshots FINAL
    WHERE pool_id = $1
)
SELECT
    acc.account_id                                                                 AS account,
    lpp.shares,
    if(snap.total_shares IS NULL OR snap.total_shares = 0,
       NULL,
       toFloat64(lpp.shares) * 100.0 / toFloat64(snap.total_shares))               AS share_percentage,
    lpp.first_deposit_ledger,
    lpp.last_updated_ledger
FROM lp_positions lpp FINAL
JOIN accounts acc FINAL ON acc.id = lpp.account_id
CROSS JOIN latest_snap snap
WHERE lpp.pool_id = $1
  AND lpp.shares  > 0
  AND ($3 IS NULL
       OR (lpp.shares, lpp.account_id) < ($3, $4))
ORDER BY lpp.shares DESC, lpp.account_id DESC
LIMIT $2;

-- SOROBAN pool (task 0374). Its providers hold the pool's share TOKEN, not a
-- pool-share trustline, so they are that token's holders in `balances`.
-- Inputs:
--   $1  :share_token_id        Int64   pool_instance_state.share_token_id
--   $2  :pool_contract_id      Int64   the pool's own contract surrogate
--   $3  :limit                 Int
--   $4  :cursor_amount         Int128  raw balance, NULL on first page
--   $5  :cursor_holder_id      Int64   NULL on first page
-- Indexes:      balances ORDER BY (holder_id, asset_id) — the one asset-first
--                 read of this table; `idx_bal_asset_id` (bloom_filter) skips
--                 the granules that do not hold the token: 1.16M rows /
--                 28 MiB for a 31-holder token, 17.9M / 519 MiB for the
--                 busiest (2026-09-22). Without it: 117M rows / 1.17 GiB.
-- Notes:
--   • The pool's own contract is left out: a pair locks its minimum
--     liquidity by holding its own share token (participant_count
--     subtracts it too). The percentage still divides by every positive
--     balance, that holding included.
--   • `sum(amt) OVER ()` runs before the keyset and the exclusion narrow the
--     set; a CTE referenced twice would scan `balances` twice.
--   • The holder is an account or a contract (share tokens get staked),
--     resolved through both dimensions afterwards. first_deposit_ledger is
--     the holder's first `mint` or incoming `transfer` of the token in
--     `soroban_events` (topic 3), a seek on the token's contract_id; it
--     dates 4,152 of 4,152 current holders (2026-09-22).

SELECT holder_id, amt AS amount,
       round(amt * 100 / total, 4) AS share_percentage,
       lul AS last_updated_ledger
FROM (
    SELECT holder_id, amt, lul, sum(amt) OVER () AS total
    FROM (
        SELECT holder_id,
               argMax(amount, last_updated_ledger) AS amt,
               max(last_updated_ledger)            AS lul
        FROM balances
        WHERE asset_id = $1
        GROUP BY holder_id
        HAVING amt > 0
    )
)
WHERE holder_id != $2
  AND ($4 IS NULL
       OR amt < $4
       OR (amt = $4 AND holder_id < $5))
ORDER BY amt DESC, holder_id DESC
LIMIT $3;
