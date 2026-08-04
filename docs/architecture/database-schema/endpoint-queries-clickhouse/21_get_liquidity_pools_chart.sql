-- Endpoint:     GET /liquidity-pools/:id/chart
-- Purpose:      Time-bucketed series for a pool: TVL, volume, fee revenue —
--               all USD, computed at read (task 0199, ADR 0053).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.14
-- Schema:       ADR 0044 (CH pilot); prices views contract pinned in the
--               prices repo `views.sql` header (2026-06-16).
-- Data sources: DB + in-cluster `prices.*` views (same CH cluster, read-only).
-- Inputs:
--   $1  :pool_id      FixedString(32)  raw 32-byte pool id
--   $2  :from_ms      Int64            inclusive lower bound (epoch millis)
--   $3  :to_ms        Int64            exclusive upper bound (epoch millis)
--   $4  :interval     '1h' | '1d' | '1w' (allowlist; picks bucket fn + grain)
--   Leg identities + fee_bps come from a per-request pre-query on
--   `liquidity_pools` (also the 404 gate): (asset_kind, asset_code,
--   issuer_address) per leg in the prices interop forms —
--   native = ('native','XLM',''), classic = ('credit', code, issuer).
-- Indexes:      liquidity_pool_snapshots ORDER BY (pool_id, ledger_sequence)
--                 — leading-PK seek bounds the scan to this pool.
--               ledgers minmax(closed_at) — window bounds resolve to a
--                 ledger range; both subquery bounds carry `>= from` so the
--                 index can prune (lore-0356: `< to` alone scans history).
-- CH Engine:    liquidity_pool_snapshots — RMT, NO FINAL: the inner
--                 subquery collapses duplicates via `LIMIT 1 BY
--                 ledger_sequence` (0356 / PR #318 determinism).
--               ledgers — RMT, deduped via `LIMIT 1 BY sequence` (lore-0420).
-- USD semantics (task 0199; Float64 arithmetic — the analytics carry a 1%
-- verification tolerance by design):
--   • TVL — state quantity: last PRICEABLE snapshot in the bucket,
--     `reserve_a·close_usd_a + reserve_b·close_usd_b`. NULL unless BOTH
--     legs price (never a one-leg partial).
--   • volume — flow quantity: `sum(gross_volume_a × close_usd_a)`, each
--     ledger priced at its own price bucket. Any unpriceable swap in the
--     bucket → NULL (honest hole, not a partial sum). The veto is
--     per-bucket, so at 1w one unpriced ledger discards the week —
--     deliberate; revisit with a coverage field rather than a silent
--     partial sum.
--   • fee_revenue — `volume × fee_bps / 10000`, derived in Rust from the
--     volume float so chart and detail run identical arithmetic.
--   • MONEY IS FORMATTED IN RUST, not SQL. `toString(round(x, 2))` emits
--     "25" / "1.5" / "0" (variable decimals) and would put the chart on a
--     different wire shape than the detail endpoint. SQL returns raw
--     Nullable(Float64); the API formats with a single helper.
-- Prices join:
--   • Grain follows the interval: '1h' → prices.price_usd_series_1h joined
--     on toStartOfHour(closed_at); '1d'/'1w' → prices.price_usd_series on
--     toStartOfDay(closed_at). No weekly candles exist — a 1w bucket
--     prices at its last snapshot's day.
--   • **ASOF LEFT JOIN, capped at 48 h.** A candle exists only once the
--     asset trades in that bucket, so exact bucket equality NULLed the
--     newest point for every pool with an illiquid leg (reproduced on prod:
--     the current day had a yXLM candle but none for WGUARDIAN). ASOF takes
--     the most recent close at or before the wanted bucket. The 48 h cap is
--     what keeps that honest — uncapped, the 2026-07-21..08-03 price freeze
--     would render as live TVL priced off a 12-day-old candle.
--   • ASOF requires an equi-join column, hence `1 AS k` on both sides — a
--     real column, not `ON 1 = 1` (that form pins join_algorithm to hash).
--   • Subquery lower bounds are widened by the same 48 h so the FIRST
--     bucket of the window can carry forward too.
--   • Identity + bucket-range predicates INSIDE the right-side subqueries:
--     the bucket range is what the prices views push down to the
--     `price_ohlcv_*` scan; without it the view aggregates full history.
--   • A join miss yields DEFAULTs (epoch bucket, 0 close), not NULL
--     (`join_use_nulls` rejected for the readonly user) — the staleness
--     test rejects it, an epoch bucket always exceeding the cap.
--   • NEVER join raw `prices.assets` (empty-code rows silently price
--     native legs as an arbitrary asset) and NEVER decode `prices.*`
--     positionally (views grow additively).
-- Upstream caveat: the IN-PROGRESS candle is unstable. Measured 2026-08-04,
--   the current hour's yXLM close read 1.3085 USD against ~0.170 in every
--   neighbouring hour (7.7×), briefly quadrupling the newest 1h TVL point.
--   That is price-source data, faithfully reported; we do not second-guess
--   it with our own outlier filter (prices owns that, and diverging would
--   make our numbers disagree with theirs).
-- Access:       no grant needed — `api_reader` has no <grants> block in
--               users.d/services.xml, so its read_only profile already
--               reads `prices.*` (verified on the box 2026-08-04).

-- {carry} = 48 h in seconds (MAX_PRICE_CARRY_SECONDS in the API).
SELECT
    bucket_ms,
    argMaxIf(tvl_row, ledger_sequence, isNotNull(tvl_row)) AS tvl,
    if(countIf(unpriced_swap) > 0, NULL, sum(vol_row))     AS volume,
    count()                                                AS samples_in_bucket
FROM (
    SELECT
        toUnixTimestamp64Milli(toDateTime64({bucket_fn}(l.closed_at), 3, 'UTC')) AS bucket_ms,
        lps.ledger_sequence                              AS ledger_sequence,
        if(dateDiff('second', pa.bucket, l.price_bucket) <= {carry},
           nullIf(toFloat64(pa.close_usd), 0), NULL)      AS pa_usd,
        if(dateDiff('second', pb.bucket, l.price_bucket) <= {carry},
           nullIf(toFloat64(pb.close_usd), 0), NULL)      AS pb_usd,
        toFloat64(lps.reserve_a) * pa_usd
            + toFloat64(lps.reserve_b) * pb_usd          AS tvl_row,
        toFloat64(lps.gross_volume_a) * pa_usd           AS vol_row,
        isNotNull(lps.gross_volume_a) AND isNull(pa_usd) AS unpriced_swap
    FROM (
        SELECT ledger_sequence, reserve_a, reserve_b, gross_volume_a
        FROM liquidity_pool_snapshots
        WHERE pool_id = $1
          AND ledger_sequence >= (SELECT min(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli($2))
          AND ledger_sequence <= (SELECT max(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli($2) AND closed_at < fromUnixTimestamp64Milli($3))
        ORDER BY ledger_sequence DESC
        LIMIT 1 BY ledger_sequence
    ) lps
    JOIN (
        SELECT 1 AS k, sequence, closed_at, {price_bucket_fn}(closed_at) AS price_bucket
        FROM ledgers
        WHERE closed_at >= fromUnixTimestamp64Milli($2)
          AND closed_at <  fromUnixTimestamp64Milli($3)
        LIMIT 1 BY sequence
    ) l ON l.sequence = lps.ledger_sequence
    ASOF LEFT JOIN (
        SELECT 1 AS k, bucket, close_usd FROM {series_view}
        WHERE asset_kind = {leg_a_kind} AND asset_code = {leg_a_code} AND issuer_address = {leg_a_issuer}
          AND bucket >= {price_bucket_fn}(fromUnixTimestamp64Milli($2)) - INTERVAL {carry} SECOND
          AND bucket <  fromUnixTimestamp64Milli($3)
    ) pa ON pa.k = l.k AND pa.bucket <= l.price_bucket
    ASOF LEFT JOIN (
        SELECT 1 AS k, bucket, close_usd FROM {series_view}
        WHERE asset_kind = {leg_b_kind} AND asset_code = {leg_b_code} AND issuer_address = {leg_b_issuer}
          AND bucket >= {price_bucket_fn}(fromUnixTimestamp64Milli($2)) - INTERVAL {carry} SECOND
          AND bucket <  fromUnixTimestamp64Milli($3)
    ) pb ON pb.k = l.k AND pb.bucket <= l.price_bucket
)
GROUP BY bucket_ms
ORDER BY bucket_ms ASC;
