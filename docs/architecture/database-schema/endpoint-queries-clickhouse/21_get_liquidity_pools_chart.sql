-- Endpoint:     GET /liquidity-pools/:id/chart
-- Purpose:      Time-bucketed series for a pool: TVL, volume, fee revenue.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.14
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :pool_id           FixedString(32)  raw 32-byte pool id
--   $2  :from_ledger       Int64            inclusive lower bound (ledger)
--   $3  :to_ledger         Int64            exclusive upper bound (ledger)
--   $4  :interval_seconds  Int              bucket size in seconds
--                                            (3600 = '1h', 86400 = '1d',
--                                             604800 = '1w'). The API maps
--                                            its '1h'|'1d'|'1w' allowlist
--                                            to this integer.
-- Indexes:      liquidity_pool_snapshots ORDER BY (pool_id, ledger_sequence, id)
--                 + PARTITION BY intDiv. The ledger range $2..$3 reduces to
--                 a partition predicate.
--               ledgers ORDER BY (sequence) — JOIN for closed_at bucket math.
-- CH Engine:    liquidity_pool_snapshots — Replacing partitioned (FINAL).
--                 ledgers — MergeTree (no FINAL).
-- CH Pattern:   GROUP BY toStartOfInterval(closed_at, INTERVAL $4 SECOND);
--                 partition prune via intDiv on ledger range;
--                 argMax over (created_at, ledger_sequence) for TVL state.
-- ADR 0044 §:   §4.5 (Replacing partitioned), §4.1 (ledgers), §5.2 (no
--                 created_at — bucket from JOIN ledgers, range parameter
--                 is ledger-based not timestamp-based for clean partition
--                 prune).
-- Notes:
--   • **Range parameter change:** PG E21 takes `from`/`to` as TIMESTAMPTZ.
--     CH-side, partition is by `intDiv(ledger_sequence, 500000)`, so we
--     pass the range as ledger sequences. The API resolves user-supplied
--     timestamps to ledger bounds via a `ledgers` lookup (cheap; one
--     `closed_at >= ?` seek per bound). This trade-off keeps the chart
--     query partition-pruned cleanly; the user-facing API contract is
--     unchanged.
--   • TVL is a state quantity: use `argMax(tvl, ledger_sequence)` per
--     bucket to project the latest within-bucket state. Volume + fee_revenue
--     are flow quantities: `sum()` over bucket.
--   • `toStartOfInterval(t, INTERVAL N SECOND)` is CH's bucket truncation
--     primitive; works for any second-multiple (3600 = 1h, 86400 = 1d).
--     For week-aligned buckets the API can also use `toStartOfWeek(t)` —
--     but a 604800-second interval starting from epoch is week-aligned
--     by construction (1970-01-04 was a Sunday; close enough for chart UX).
--   • `samples_in_bucket` = `count()` of snapshot rows that landed in the
--     bucket.

SELECT
    toStartOfInterval(l.closed_at, INTERVAL $4 SECOND)              AS bucket,
    argMax(lps.tvl,         lps.ledger_sequence)                    AS tvl,
    sum(lps.volume)                                                 AS volume,
    sum(lps.fee_revenue)                                            AS fee_revenue,
    count()                                                         AS samples_in_bucket
FROM liquidity_pool_snapshots lps FINAL
JOIN ledgers l ON l.sequence = lps.ledger_sequence
WHERE lps.pool_id = $1
  AND lps.ledger_sequence >= $2
  AND lps.ledger_sequence <  $3
  AND intDiv(lps.ledger_sequence, 500000) BETWEEN intDiv($2, 500000) AND intDiv($3, 500000)
GROUP BY bucket
ORDER BY bucket ASC;
