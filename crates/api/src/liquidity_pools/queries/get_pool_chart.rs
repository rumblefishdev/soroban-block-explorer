//! `GET /v1/liquidity-pools/:id/chart` — the time-bucketed USD series.

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;

use crate::common::ch::millis_to_utc;

use super::usd_analytics::{
    MAX_PRICE_CARRY_SECONDS, PoolPriceContext, fee_revenue_usd, price_leg, priced_pair, usd_str,
};
use crate::liquidity_pools::dto::ChartDataPoint;

/// Money arrives as raw `Nullable(Float64)` and is formatted by [`usd_str`]
/// on the Rust side; `fee_revenue` is derived from `volume` here rather
/// than in SQL, so chart and detail run identical arithmetic.
#[derive(Debug, Row, Deserialize)]
pub(super) struct ChartChRow {
    pub(super) bucket_ms: i64,
    pub(super) tvl: Option<f64>,
    pub(super) volume: Option<f64>,
    pub(super) samples_in_bucket: u64,
}

/// `GET /v1/liquidity-pools/:id/chart` — time-bucketed TVL / volume / fee
/// series, USD computed at read (task 0199, ADR 0053).
///
/// CH translation choices:
/// - **Bucket truncation** maps the `1h | 1d | 1w` allowlist to
///   `toStartOfHour` / `toStartOfDay` / `toMonday`. `toMonday` is the
///   Monday-start week, matching PG's ISO `date_trunc('week', …)` — the
///   contract the endpoint launched with. CH's other spellings both miss
///   it (box-verified 2026-08-05): `toStartOfWeek` defaults to SUNDAY
///   (mode 0, one day off ISO), and the reference SQL's epoch-aligned
///   `toStartOfInterval(…, INTERVAL 604800 SECOND)` buckets on THURSDAYS —
///   1970-01-01 was a Thursday, so 7-day blocks from epoch all are
///   (2026-08-04 → bucket 2026-07-30, toDayOfWeek = 4). An earlier version
///   of this comment claimed "Sunday-aligned"; that was wrong.
/// - **No `created_at` on CH snapshots** — the window is filtered on the
///   joined `ledgers.closed_at` (bijection with `ledger_sequence`), so the
///   `from`/`to` API contract (RFC3339 timestamps) is preserved unchanged
///   rather than switched to the ledger-bound form the reference SQL used.
/// - **`pool_id = unhex(?)`** is a leading-PK seek on
///   `liquidity_pool_snapshots` (`ORDER BY (pool_id, ledger_sequence)`), so
///   the scan is bounded to this pool's snapshots — box-measured 14.5 M rows
///   / 237 MB for the hottest pool (1.84 M snapshots) over a 90-day 1d window.
///
/// USD semantics (per bucket):
/// - **TVL** is a state quantity — the last snapshot in the bucket whose
///   own price bucket is priced: `reserve_a·close_usd_a + reserve_b·close_usd_b`.
///   NULL unless BOTH legs price (a one-leg TVL silently halves the pool).
///   `argMaxIf(…, isNotNull(tvl_row))` deliberately falls back to the last
///   PRICEABLE snapshot in the bucket (≤ one bucket of intra-bucket
///   staleness) instead of NULLing the bucket on a missing tip price.
/// - **volume** is a flow quantity — `sum(gross_volume_a × close_usd_a)`,
///   each ledger priced at its OWN price bucket. If any swap row in the
///   bucket lacks a leg-A price the bucket's volume is NULL (an honest
///   hole), never a silent partial sum. `sum()` over an all-NULL bucket
///   (no swaps) yields NULL (box-confirmed), matching the PG contract.
///   NOTE: the veto is all-or-nothing per bucket, so at `1w` a single
///   unpriced ledger discards the week — deliberate for now (an unmarked
///   partial sum reads as a real number), revisit with a coverage field.
/// - **fee_revenue** is derived in Rust from `volume` ([`fee_revenue_usd`]).
///
/// Prices join (contract: prices views.sql, pinned 2026-06-16):
/// - Grain follows the interval: `1h` → `prices.price_usd_series_1h` on
///   `toStartOfHour(closed_at)`; `1d`/`1w` → `prices.price_usd_series` on
///   `toStartOfDay(closed_at)` (weekly candles are not provided — a 1w
///   bucket's TVL prices at its last snapshot's DAY).
/// - **`ASOF LEFT JOIN` on `price.bucket <= ledger.price_bucket`**, capped
///   at [`MAX_PRICE_CARRY_SECONDS`]: a candle exists only once the asset
///   trades in that bucket, so exact equality left the newest point of
///   every illiquid-leg pool NULL (box-reproduced). ASOF needs an equi-join
///   column, hence the constant `k` on both sides — a real column, not the
///   `ON 1 = 1` form that pins the join algorithm to `hash`. The
///   subqueries' lower bound is widened by the same cap so the FIRST bucket
///   can carry forward too.
/// - The staleness cap is what keeps carry-forward honest: without it the
///   2026-07-21..08-03 provider freeze would render as live TVL priced off
///   a 12-day-old candle.
/// - Identity + bucket-range predicates live INSIDE the right-side
///   subqueries: the bucket range is what bounds the view's scan of
///   `price_ohlcv_*` (their header's pushdown note); one identity per side
///   keeps the hash tables at ≤ one row per bucket.
/// - A join miss yields DEFAULT (epoch `bucket`, `0` close), not NULL
///   (`join_use_nulls` is rejected for the readonly user) — the staleness
///   test rejects it, since an epoch bucket is always further back than the
///   cap. `nullIf(close_usd, 0)` guards the priced-but-zero case; the views
///   already filter `close_usd > 0`.
/// - The **in-progress price bucket is excluded** (`least(to, grain(now))`).
///   It is only partly enriched, so its weighted close can be a dust print —
///   see [`fetch_last_closes`] for the measured case and the prices owner's
///   confirmation. The ASOF carry then prices the newest chart bucket off
///   the last CLOSED price bucket, which is exactly what the carry is for.
pub async fn fetch_pool_chart(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    ctx: &PoolPriceContext,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<ChartDataPoint>, clickhouse::error::Error> {
    // Defensive second gate (the handler validates against the allowlist
    // first) — fail loud on allowlist drift rather than emit a wrong bucket.
    assert!(
        matches!(interval, "1h" | "1d" | "1w"),
        "fetch_pool_chart called with non-allowlisted interval `{interval}` — \
         handler validation drift; expected 1h | 1d | 1w"
    );
    let bucket_fn = match interval {
        "1h" => "toStartOfHour",
        "1d" => "toStartOfDay",
        "1w" => "toMonday",
        _ => unreachable!("interval validated against the 1h|1d|1w allowlist above"),
    };
    let (series_view, price_bucket_fn) = match interval {
        "1h" => ("prices.price_usd_series_1h", "toStartOfHour"),
        "1d" | "1w" => ("prices.price_usd_series", "toStartOfDay"),
        _ => unreachable!("interval validated against the 1h|1d|1w allowlist above"),
    };

    // `bucket_ms`: each truncated bucket is coerced to a UTC `DateTime64(3)`
    // then to epoch millis, so `millis_to_utc` round-trips it on the Rust side
    // (matches the `DateTime<Utc>` shape PG returns from `date_trunc`).
    //
    // **NO `FINAL`** (0356 / PR #318): `sum()`/`count()` over bucketed snapshots
    // must see exactly one row per ledger, so a bare `FROM … FINAL` can't just be
    // dropped — pre-cleanup before/after duplicates would double-count volume /
    // fee_revenue / samples. Instead the inner subquery collapses to one row per
    // ledger (`LIMIT 1 BY ledger_sequence`) with no merge; tvl/volume/fee are
    // identical across a duplicate pair, so which row survives is irrelevant. The
    // outer bucket aggregation is then byte-identical to the old `FINAL` form.
    //
    // The outer `ledgers` read is ALSO deduped — via a `LIMIT 1 BY sequence`
    // subquery, not a bare `JOIN ledgers` (lore-0420). `ledgers` is itself a
    // ReplacingMergeTree with unmerged duplicate rows, so a bare join doubled
    // every snapshot (measured 2806 samples vs 1403 distinct) → doubled
    // `sum(volume)`/`sum(fee_revenue)`/`samples_in_bucket`. The join needs
    // `l.closed_at` for the bucket key, so a pure `IN` semi-join won't do;
    // `LIMIT 1 BY sequence` keeps one row per ledger (closed_at is identical
    // across a dup pair, so which survives is irrelevant) — same idiom as the
    // snapshot dedup above, and it keeps `closed_at` a plain column so the
    // window filter can stay in the subquery WHERE and drive minmax pruning.
    //
    // The inner read is bounded to the window's ledger range, resolved from
    // `[from, to]` against `ledgers.closed_at` (minmax skip index).
    //
    // The upper-bound subquery carries a lower bound too. That reads as
    // redundant — it is not. A minmax index can only skip granules that
    // *cannot* match, and `closed_at < to` alone matches all of history, so
    // that subquery scanned the full 26M-row table to find the last ledger
    // before `to` — 37% of the box's total read work at 50M req/month. The
    // extra `closed_at >= from` is what gives the index something to prune
    // (26M → ~209k). The value is unchanged either way: ledgers close every
    // ~5 s with no gaps, so the last ledger before `to` always falls inside
    // `[from, to)`.
    let sql = format!(
        "SELECT \
            bucket_ms, \
            argMaxIf(tvl_row, ledger_sequence, isNotNull(tvl_row)) AS tvl, \
            if(countIf(unpriced_swap) > 0, NULL, sum(vol_row))     AS volume, \
            count()                                                AS samples_in_bucket \
         FROM ( \
             SELECT \
                toUnixTimestamp64Milli(toDateTime64({bucket_fn}(l.closed_at), 3, 'UTC')) AS bucket_ms, \
                lps.ledger_sequence                              AS ledger_sequence, \
                if(dateDiff('second', pa.bucket, l.price_bucket) <= {carry}, \
                   nullIf(toFloat64(pa.close_usd), 0), NULL)      AS pa_usd, \
                if(dateDiff('second', pb.bucket, l.price_bucket) <= {carry}, \
                   nullIf(toFloat64(pb.close_usd), 0), NULL)      AS pb_usd, \
                toFloat64(lps.reserve_a) * pa_usd \
                    + toFloat64(lps.reserve_b) * pb_usd          AS tvl_row, \
                toFloat64(lps.gross_volume_a) * pa_usd           AS vol_row, \
                isNotNull(lps.gross_volume_a) AND isNull(pa_usd) AS unpriced_swap \
             FROM ( \
                 SELECT ledger_sequence, reserve_a, reserve_b, gross_volume_a \
                 FROM liquidity_pool_snapshots \
                 WHERE pool_id = unhex(?) \
                   AND ledger_sequence >= (SELECT min(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli(?)) \
                   AND ledger_sequence <= (SELECT max(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli(?) AND closed_at < fromUnixTimestamp64Milli(?)) \
                 ORDER BY ledger_sequence DESC \
                 LIMIT 1 BY ledger_sequence \
             ) lps \
             JOIN ( \
                 SELECT 1 AS k, sequence, closed_at, {price_bucket_fn}(closed_at) AS price_bucket \
                 FROM ledgers \
                 WHERE closed_at >= fromUnixTimestamp64Milli(?) \
                   AND closed_at <  fromUnixTimestamp64Milli(?) \
                 LIMIT 1 BY sequence \
             ) l ON l.sequence = lps.ledger_sequence \
             ASOF LEFT JOIN ( \
                 SELECT 1 AS k, bucket, close_usd \
                 FROM {series_view} \
                 WHERE asset_kind = ? AND asset_code = ? AND issuer_address = ? \
                   AND bucket >= {price_bucket_fn}(fromUnixTimestamp64Milli(?)) - INTERVAL {carry} SECOND \
                   AND bucket <  least(fromUnixTimestamp64Milli(?), {price_bucket_fn}(now())) \
                   AND close_usd > 0 \
             ) pa ON pa.k = l.k AND pa.bucket <= l.price_bucket \
             ASOF LEFT JOIN ( \
                 SELECT 1 AS k, bucket, close_usd \
                 FROM {series_view} \
                 WHERE asset_kind = ? AND asset_code = ? AND issuer_address = ? \
                   AND bucket >= {price_bucket_fn}(fromUnixTimestamp64Milli(?)) - INTERVAL {carry} SECOND \
                   AND bucket <  least(fromUnixTimestamp64Milli(?), {price_bucket_fn}(now())) \
                   AND close_usd > 0 \
             ) pb ON pb.k = l.k AND pb.bucket <= l.price_bucket \
         ) \
         GROUP BY bucket_ms \
         ORDER BY bucket_ms ASC",
        bucket_fn = bucket_fn,
        price_bucket_fn = price_bucket_fn,
        series_view = series_view,
        carry = MAX_PRICE_CARRY_SECONDS,
    );

    let (chart_leg_a, chart_leg_b) = match priced_pair(ctx) {
        Some((a, b)) => (a.clone(), b.clone()),
        None => (price_leg(-1, None, None), price_leg(-1, None, None)),
    };
    let rows = client
        .query(&sql)
        .bind(pool_id_hex)
        .bind(from.timestamp_millis()) // min(sequence): closed_at >= from
        .bind(from.timestamp_millis()) // max(sequence): closed_at >= from
        .bind(to.timestamp_millis()) // max(sequence): closed_at <  to
        .bind(from.timestamp_millis()) // ledgers dedup subquery: closed_at >= from
        .bind(to.timestamp_millis()) // ledgers dedup subquery: closed_at <  to
        .bind(chart_leg_a.kind) // pa: identity
        .bind(chart_leg_a.code.as_str())
        .bind(chart_leg_a.issuer.as_str())
        .bind(from.timestamp_millis()) // pa: bucket >= floor(from)
        .bind(to.timestamp_millis()) // pa: bucket < to
        .bind(chart_leg_b.kind) // pb: identity
        .bind(chart_leg_b.code.as_str())
        .bind(chart_leg_b.issuer.as_str())
        .bind(from.timestamp_millis()) // pb: bucket >= floor(from)
        .bind(to.timestamp_millis()) // pb: bucket < to
        .fetch_all::<ChartChRow>()
        .await?;

    Ok(rows
        .into_iter()
        .map(|r| ChartDataPoint {
            bucket: millis_to_utc(r.bucket_ms),
            tvl: r.tvl.map(usd_str),
            volume: r.volume.map(usd_str),
            fee_revenue: r.volume.map(|v| usd_str(fee_revenue_usd(v, ctx.fee_bps))),
            samples_in_bucket: r.samples_in_bucket as i64,
        })
        .collect())
}
