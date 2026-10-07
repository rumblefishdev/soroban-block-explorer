//! `GET /v1/liquidity-pools/:id/chart` — the time-bucketed USD series.
//!
//! One shape for both pool kinds. The database answers three questions — the
//! pool's last state in each bucket where it changed; each leg's price per
//! price bucket; the traded volume per bucket — and [`assemble_chart`] turns
//! the answers into points.

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

use super::usd_analytics::{
    MAX_PRICE_CARRY_SECONDS, PoolChartContext, PriceLeg, fee_revenue_usd, price_leg, usd_str,
};
use crate::common::ch::millis_to_utc;
use crate::liquidity_pools::dto::ChartDataPoint;

/// A pool state and where it is priced: `(price bucket start in epoch
/// seconds, each leg's reserve)`.
type PricedState = (i64, Vec<Option<f64>>);

/// What the database says about one pool over the window, in display units —
/// the input of [`assemble_chart`].
#[derive(Debug, Default)]
struct ChartInputs {
    /// Each bucket in which the pool's state changed: its last state in each
    /// price bucket (hour or day) it changed in. Keyed by bucket start, epoch
    /// millis, as the next two maps. `None` inside the reserves: that leg's
    /// reserve is not known.
    states: BTreeMap<i64, Vec<PricedState>>,
    /// How many state rows each of those buckets holds.
    samples: BTreeMap<i64, u64>,
    /// Traded volume per bucket, USD; `None` where a trade could not be
    /// priced. A bucket without trades is absent.
    volumes: BTreeMap<i64, Option<f64>>,
    /// Each leg's closes, `(price bucket start in epoch seconds, USD)`,
    /// ascending; the outer index is the leg, in pool order.
    prices: Vec<Vec<(i64, f64)>>,
}

/// `GET /v1/liquidity-pools/:id/chart` — TVL, volume and fee revenue per
/// bucket, USD computed at read (task 0199, ADR 0053).
///
/// - **TVL** is a state: a bucket in which the pool changed gets its last
///   state that prices — each leg's reserve × that leg's close at the
///   state's own price bucket, carried back at most
///   [`MAX_PRICE_CARRY_SECONDS`]. `NULL` unless EVERY leg has a reserve and a
///   price — a partial sum understates the pool while looking real. A weekly
///   bucket prices at the latest day of the week that prices: weekly candles
///   are not provided.
/// - **Volume** is a flow: only buckets with trades carry it. Fee revenue is
///   derived from it.
/// - A bucket appears when the pool changed or traded in it.
pub async fn fetch_pool_chart(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    ctx: &PoolChartContext,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<ChartDataPoint>, clickhouse::error::Error> {
    let legs = &ctx.price.legs;
    let (prices, mut inputs) = match ctx.pool_kind {
        domain::PoolKind::Classic => futures::try_join!(
            fetch_leg_closes(client, legs, interval, from, to),
            fetch_classic_series(client, pool_id_hex, legs, interval, from, to),
        )?,
        domain::PoolKind::Soroban => futures::try_join!(
            fetch_leg_closes(client, legs, interval, from, to),
            fetch_soroban_series(client, pool_id_hex, ctx, interval, from, to),
        )?,
    };
    inputs.prices = prices;
    Ok(assemble_chart(&inputs, ctx.price.fee_bps))
}

/// The chart's points: one per bucket in which the pool changed or traded,
/// oldest first. See [`fetch_pool_chart`] for what each field means.
fn assemble_chart(inputs: &ChartInputs, fee_bps: i32) -> Vec<ChartDataPoint> {
    let buckets: BTreeSet<i64> = inputs
        .states
        .keys()
        .chain(inputs.volumes.keys())
        .copied()
        .collect();
    let mut points = Vec::new();
    for ms in buckets {
        let tvl = match inputs.states.get(&ms) {
            Some(states) => last_priced_tvl(states, &inputs.prices),
            None => None,
        };
        let volume = inputs.volumes.get(&ms).copied().flatten();
        points.push(ChartDataPoint {
            bucket: millis_to_utc(ms),
            tvl: tvl.map(usd_str),
            volume: volume.map(usd_str),
            fee_revenue: volume.map(|v| usd_str(fee_revenue_usd(v, fee_bps))),
            samples_in_bucket: inputs.samples.get(&ms).copied().unwrap_or(0) as i64,
        });
    }
    points
}

/// The TVL of a bucket's newest state that prices. Every state of one price
/// bucket shares its closes, so the newest price bucket that prices holds the
/// bucket's last priceable state.
fn last_priced_tvl(states: &[PricedState], prices: &[Vec<(i64, f64)>]) -> Option<f64> {
    let mut newest_first: Vec<&PricedState> = states.iter().collect();
    newest_first.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, reserves) in newest_first {
        if let Some(usd) = tvl_usd(reserves, prices, *at) {
            return Some(usd);
        }
    }
    None
}

/// Σ reserve × close over the legs, or `None` when any leg lacks either — or
/// when the pool has no legs to value.
fn tvl_usd(reserves: &[Option<f64>], prices: &[Vec<(i64, f64)>], at: i64) -> Option<f64> {
    if prices.is_empty() {
        return None;
    }
    let mut usd = 0.0;
    for (leg, closes) in prices.iter().enumerate() {
        let reserve = reserves.get(leg).copied().flatten()?;
        let close = close_at(closes, at)?;
        usd += reserve * close;
    }
    Some(usd)
}

/// The newest close at or before `at` (epoch seconds), if it is at most
/// [`MAX_PRICE_CARRY_SECONDS`] old. A price bucket exists only once the asset
/// trades in it, so an exact match would leave quiet assets unpriced; the cap
/// keeps a stalled price feed from rendering as live TVL.
fn close_at(closes: &[(i64, f64)], at: i64) -> Option<f64> {
    let newer = closes.partition_point(|(bucket, _)| *bucket <= at);
    if newer == 0 {
        return None;
    }
    let (bucket, close) = closes[newer - 1];
    if at - bucket > MAX_PRICE_CARRY_SECONDS {
        return None;
    }
    Some(close)
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct LegCloseChRow {
    leg: u64,
    bucket_s: i64,
    close: f64,
}

/// Each leg's closes over the window, from the price series the interval
/// reads (`1h` the hourly series, `1d` / `1w` the daily one), widened back by
/// [`MAX_PRICE_CARRY_SECONDS`] so the first bucket can carry a price too.
///
/// - Contract (prices views.sql, pinned 2026-06-16): key
///   `(asset_kind, asset_code, issuer_address)`; the rows carry the leg they
///   price (`indexOf` of their identity in the pool's leg list), so one read
///   prices every leg, however many. An unpriceable leg (empty kind) matches
///   no row.
/// - The views do not guarantee `close_usd > 0` (a zero-volume bucket can
///   publish a huge negative), so non-positive rows are dropped here.
/// - The **in-progress price bucket is excluded** (`least(to, grain(now))`):
///   it is only partly enriched, so its weighted close can be a dust print —
///   see `fetch_last_closes` for the measured case. The newest chart bucket
///   then prices off the last CLOSED price bucket, which is what the carry
///   is for.
async fn fetch_leg_closes(
    client: &clickhouse::Client,
    legs: &[PriceLeg],
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Vec<(i64, f64)>>, clickhouse::error::Error> {
    let (_, series_view, price_bucket_fn) = chart_grain(interval);
    let sql = format!(
        "SELECT toUInt64(indexOf(arrayZip(?, ?, ?), (asset_kind, asset_code, issuer_address))) AS leg, \
                toInt64(toUnixTimestamp(bucket)) AS bucket_s, \
                toFloat64(close_usd) AS close \
         FROM {series_view} \
         WHERE leg > 0 \
           AND bucket >= {price_bucket_fn}(fromUnixTimestamp64Milli(?)) - INTERVAL {carry} SECOND \
           AND bucket <  least(fromUnixTimestamp64Milli(?), {price_bucket_fn}(now())) \
           AND close_usd > 0 \
         ORDER BY leg, bucket",
        carry = MAX_PRICE_CARRY_SECONDS,
    );
    let rows = client
        .query(&sql)
        .bind(legs.iter().map(|l| l.kind).collect::<Vec<_>>()) // leg identities
        .bind(legs.iter().map(|l| l.code.as_str()).collect::<Vec<_>>())
        .bind(legs.iter().map(|l| l.issuer.as_str()).collect::<Vec<_>>())
        .bind(from.timestamp_millis()) // bucket >= floor(from) - carry
        .bind(to.timestamp_millis()) // bucket < to
        .fetch_all::<LegCloseChRow>()
        .await?;
    let mut closes = vec![Vec::new(); legs.len()];
    for row in rows {
        closes[row.leg as usize - 1].push((row.bucket_s, row.close));
    }
    Ok(closes)
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
pub(super) struct ClassicBucketChRow {
    pub(super) bucket_ms: i64,
    pub(super) states: Vec<PricedState>,
    pub(super) samples_in_bucket: u64,
    pub(super) volume: Option<f64>,
}

/// A classic pool's chart inputs from its snapshots — one row per change,
/// reserves already in units — in one read: per bucket, the last snapshot of
/// each price bucket it holds.
///
/// - **Volume** is `gross_volume_a` (the snapshot's sum of claim atoms on leg
///   A), each ledger priced at its OWN price bucket; a bucket with any swap
///   row lacking a leg-A price is `NULL` — an honest hole, never a partial
///   sum. A bucket of snapshots without swaps sums all-NULL to `NULL`.
/// - **NO `FINAL`** (0356): `liquidity_pool_snapshots` and `ledgers` are
///   ReplacingMergeTrees with unmerged duplicates, so each is read one row per
///   key (`LIMIT 1 BY`) — a bare join doubled every snapshot (lore-0420).
/// - The snapshots are bounded to the window's ledger range, resolved from
///   `[from, to)` against `ledgers.closed_at`. The upper-bound subquery carries
///   a lower bound too, which is not redundant: `closed_at < to` alone matches
///   all of history and gives the minmax index nothing to skip (26M → ~209k
///   rows).
async fn fetch_classic_series(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    legs: &[PriceLeg],
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<ChartInputs, clickhouse::error::Error> {
    let (bucket_fn, series_view, price_bucket_fn) = chart_grain(interval);
    let sql = format!(
        "SELECT \
            bucket_ms, \
            groupArray((price_bucket_s, reserves))   AS states, \
            sum(samples)                             AS samples_in_bucket, \
            if(sum(unpriced) > 0, NULL, sum(volume)) AS volume \
         FROM ( \
         SELECT \
            bucket_ms, \
            price_bucket_s, \
            argMax(reserves, ledger_sequence) AS reserves, \
            count()                           AS samples, \
            countIf(unpriced_swap)            AS unpriced, \
            sum(vol_row)                      AS volume \
         FROM ( \
             SELECT \
                toUnixTimestamp64Milli(toDateTime64({bucket_fn}(l.closed_at), 3, 'UTC')) AS bucket_ms, \
                lps.ledger_sequence                              AS ledger_sequence, \
                [toNullable(toFloat64(lps.reserve_a)), toNullable(toFloat64(lps.reserve_b))] AS reserves, \
                toInt64(toUnixTimestamp(l.price_bucket))         AS price_bucket_s, \
                if(dateDiff('second', pa.bucket, l.price_bucket) <= {carry}, \
                   nullIf(toFloat64(pa.close_usd), 0), NULL)      AS pa_usd, \
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
         ) \
         GROUP BY bucket_ms, price_bucket_s \
         ) \
         GROUP BY bucket_ms \
         ORDER BY bucket_ms ASC",
        carry = MAX_PRICE_CARRY_SECONDS,
    );
    let unpriceable = price_leg(-1, None, None);
    let leg_a = legs.first().unwrap_or(&unpriceable);
    let buckets = client
        .query(&sql)
        .bind(pool_id_hex)
        .bind(from.timestamp_millis()) // min(sequence): closed_at >= from
        .bind(from.timestamp_millis()) // max(sequence): closed_at >= from
        .bind(to.timestamp_millis()) // max(sequence): closed_at <  to
        .bind(from.timestamp_millis()) // ledgers: closed_at >= from
        .bind(to.timestamp_millis()) // ledgers: closed_at <  to
        .bind(leg_a.kind) // leg-A price identity
        .bind(leg_a.code.as_str())
        .bind(leg_a.issuer.as_str())
        .bind(from.timestamp_millis()) // prices: bucket >= floor(from)
        .bind(to.timestamp_millis()) // prices: bucket < to
        .fetch_all::<ClassicBucketChRow>()
        .await?;

    let mut inputs = ChartInputs::default();
    for row in buckets {
        inputs.samples.insert(row.bucket_ms, row.samples_in_bucket);
        inputs.states.insert(row.bucket_ms, row.states);
        inputs.volumes.insert(row.bucket_ms, row.volume);
    }
    Ok(inputs)
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct SorobanBucketChRow {
    bucket_ms: i64,
    states: Vec<PricedState>,
    samples_in_bucket: u64,
}

/// A soroban pool's chart inputs: its reserve history from
/// `pool_state_changes` (one row per pool and ledger, raw token units, one
/// entry per leg in pool order — a concentrated pool's vector carries per-tick
/// state after its legs, so only the first `legs` entries are read) and the
/// traded volume ([`fetch_soroban_volume_series`]).
/// Reserves are scaled by each leg's decimals; a leg with no known decimals
/// has no reserve, so no TVL. Window bounds and dedup as in
/// [`fetch_classic_series`].
async fn fetch_soroban_series(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    ctx: &PoolChartContext,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<ChartInputs, clickhouse::error::Error> {
    let (bucket_fn, _, price_bucket_fn) = chart_grain(interval);
    let leg_count = ctx.price.legs.len() as u64;
    let sql = format!(
        "SELECT \
            bucket_ms, \
            groupArray((price_bucket_s, reserves)) AS states, \
            sum(samples)                           AS samples_in_bucket \
         FROM ( \
         SELECT \
            l.bucket_ms      AS bucket_ms, \
            l.price_bucket_s AS price_bucket_s, \
            argMax(arrayMap(x -> toNullable(toFloat64(x)), arraySlice(s.reserves, 1, ?)), s.ledger_sequence) AS reserves, \
            count()          AS samples \
         FROM ( \
             SELECT ledger_sequence, reserves \
             FROM pool_state_changes \
             WHERE pool_id = unhex(?) \
               AND ledger_sequence >= (SELECT min(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli(?)) \
               AND ledger_sequence <= (SELECT max(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli(?) AND closed_at < fromUnixTimestamp64Milli(?)) \
             ORDER BY ledger_sequence DESC \
             LIMIT 1 BY ledger_sequence \
         ) s \
         JOIN ( \
             SELECT sequence, \
                    toInt64(toUnixTimestamp({price_bucket_fn}(closed_at))) AS price_bucket_s, \
                    toUnixTimestamp64Milli(toDateTime64({bucket_fn}(closed_at), 3, 'UTC')) AS bucket_ms \
             FROM ledgers \
             WHERE closed_at >= fromUnixTimestamp64Milli(?) AND closed_at < fromUnixTimestamp64Milli(?) \
             LIMIT 1 BY sequence \
         ) l ON l.sequence = s.ledger_sequence \
         GROUP BY bucket_ms, price_bucket_s \
         ) \
         GROUP BY bucket_ms \
         ORDER BY bucket_ms ASC"
    );
    let buckets = client
        .query(&sql)
        .bind(leg_count) // the legs' entries of the reserve vector
        .bind(pool_id_hex)
        .bind(from.timestamp_millis()) // min(sequence): closed_at >= from
        .bind(from.timestamp_millis()) // max(sequence): closed_at >= from
        .bind(to.timestamp_millis()) // max(sequence): closed_at <  to
        .bind(from.timestamp_millis()) // ledgers: closed_at >= from
        .bind(to.timestamp_millis()) // ledgers: closed_at <  to
        .fetch_all::<SorobanBucketChRow>();
    let volumes = fetch_soroban_volume_series(client, pool_id_hex, ctx, interval, from, to);
    let (buckets, volumes) = futures::try_join!(buckets, volumes)?;

    let mut inputs = ChartInputs::default();
    for row in buckets {
        let mut states = Vec::new();
        for (at, raw) in row.states {
            states.push((at, scale_reserves(&raw, &ctx.leg_decimals)));
        }
        inputs.samples.insert(row.bucket_ms, row.samples_in_bucket);
        inputs.states.insert(row.bucket_ms, states);
    }
    for row in volumes {
        inputs.volumes.insert(row.bucket_ms, row.volume);
    }
    Ok(inputs)
}

/// Raw reserves → display units, one leg at a time; a leg whose token
/// publishes no decimals has no reserve.
fn scale_reserves(raw: &[Option<f64>], decimals: &[Option<u32>]) -> Vec<Option<f64>> {
    let mut scaled = Vec::with_capacity(raw.len());
    for (leg, reserve) in raw.iter().enumerate() {
        let value = match (reserve, decimals.get(leg)) {
            (Some(r), Some(Some(d))) => Some(r / 10f64.powi(*d as i32)),
            _ => None,
        };
        scaled.push(value);
    }
    scaled
}

/// One bucket of a soroban pool's traded volume, USD.
#[derive(Debug, Row, Deserialize)]
struct SorobanVolumeChRow {
    bucket_ms: i64,
    volume: Option<f64>,
}

/// A soroban pool's volume per chart bucket: the absolute amount of every
/// trade's traded leg, scaled by that leg's decimals and priced at the
/// trade's own ledger, as the classic chart prices `gross_volume_a`.
///
/// - The **traded leg** is the lowest-index leg the trade wrote a row for
///   (legs in the registry row's order). A two-leg trade writes both legs, so
///   it is leg A, the leg the classic snapshot counts. A three- or four-leg
///   trade writes only the two legs it moved, often not leg A, and is counted
///   on the first of the two. The detail endpoint's 24h volume reads the same
///   rule, in `fetch_pool_volume_24h` (`usd_analytics.rs`).
/// - A bucket with any trade whose traded leg has no price or no published
///   decimals is `NULL` — an honest hole, never a partial sum, the classic
///   chart's rule. An unpriceable leg (empty kind) matches no price row; an
///   unknown scale is bound as -1.
/// - Rows are deduped on the table's full sort key: the live writer and the
///   backfill write the same rows on purpose.
async fn fetch_soroban_volume_series(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    ctx: &PoolChartContext,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<SorobanVolumeChRow>, clickhouse::error::Error> {
    let (bucket_fn, series_view, price_bucket_fn) = chart_grain(interval);
    let sql = format!(
        "WITH t AS ( \
             SELECT ledger_sequence, min(leg) AS traded_leg, argMin(amount, leg) AS amount \
             FROM ( \
                 SELECT ledger_sequence, application_order, operation_index, event_index, \
                        indexOf((SELECT legs FROM liquidity_pools \
                                 WHERE pool_id = unhex(?) LIMIT 1), asset_id) AS leg, \
                        amount \
                 FROM pool_movements \
                 WHERE pool_id = toFixedString(unhex(?), 32) \
                   AND event_kind = ? \
                   AND ledger_sequence >= (SELECT min(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli(?)) \
                   AND ledger_sequence <= (SELECT max(sequence) FROM ledgers WHERE closed_at >= fromUnixTimestamp64Milli(?) AND closed_at < fromUnixTimestamp64Milli(?)) \
                 LIMIT 1 BY ledger_sequence, application_order, operation_index, event_index, asset_id \
             ) \
             WHERE leg > 0 \
             GROUP BY ledger_sequence, application_order, operation_index, event_index \
         ) \
         SELECT \
            bucket_ms, \
            if(countIf(isNull(usd)) > 0, NULL, sum(usd)) AS volume \
         FROM ( \
             SELECT \
                l.bucket_ms AS bucket_ms, \
                arrayElement(?, t.traded_leg) AS scale, \
                if(dateDiff('second', p.bucket, l.price_bucket) <= {carry} AND scale >= 0, \
                   abs(toFloat64(t.amount)) / pow(10, scale) \
                       * nullIf(toFloat64(p.close_usd), 0), NULL) AS usd \
             FROM t \
             JOIN ( \
                 SELECT sequence, {price_bucket_fn}(closed_at) AS price_bucket, \
                        toUnixTimestamp64Milli(toDateTime64({bucket_fn}(closed_at), 3, 'UTC')) AS bucket_ms \
                 FROM ledgers \
                 WHERE sequence IN (SELECT ledger_sequence FROM t) \
                 LIMIT 1 BY sequence \
             ) l ON l.sequence = t.ledger_sequence \
             ASOF LEFT JOIN ( \
                 SELECT toUInt64(indexOf(arrayZip(?, ?, ?), (asset_kind, asset_code, issuer_address))) AS leg, \
                        bucket, close_usd \
                 FROM {series_view} \
                 WHERE leg > 0 \
                   AND bucket >= {price_bucket_fn}(fromUnixTimestamp64Milli(?)) - INTERVAL {carry} SECOND \
                   AND bucket <  least(fromUnixTimestamp64Milli(?), {price_bucket_fn}(now())) \
                   AND close_usd > 0 \
             ) p ON p.leg = t.traded_leg AND p.bucket <= l.price_bucket \
         ) \
         GROUP BY bucket_ms",
        carry = MAX_PRICE_CARRY_SECONDS,
    );
    let legs = &ctx.price.legs;
    let scales: Vec<i32> = ctx
        .leg_decimals
        .iter()
        .map(|d| d.map_or(-1, |d| d as i32))
        .collect();
    // The window's ledgers are read only where the pool traded: the trades
    // already carry the window, and a whole-window `ledgers` read was most
    // of this query's cost (1Y: 6.1M rows of 31.4M, 2026-10-01).
    client
        .query(&sql)
        .bind(pool_id_hex) // registry legs
        .bind(pool_id_hex)
        .bind(domain::PoolEvent::Trade as u8)
        .bind(from.timestamp_millis()) // min(sequence): closed_at >= from
        .bind(from.timestamp_millis()) // max(sequence): closed_at >= from
        .bind(to.timestamp_millis()) // max(sequence): closed_at <  to
        .bind(scales) // each leg's scale, -1 where unknown
        .bind(legs.iter().map(|l| l.kind).collect::<Vec<_>>()) // leg identities
        .bind(legs.iter().map(|l| l.code.as_str()).collect::<Vec<_>>())
        .bind(legs.iter().map(|l| l.issuer.as_str()).collect::<Vec<_>>())
        .bind(from.timestamp_millis()) // prices: bucket >= floor(from)
        .bind(to.timestamp_millis()) // prices: bucket < to
        .fetch_all::<SorobanVolumeChRow>()
        .await
}

/// The `1h | 1d | 1w` allowlist → the chart's bucket function, the price
/// series view, and the price bucket a ledger is priced at. A 1w bucket prices
/// at a DAY: weekly candles are not provided. Fails loud on allowlist drift
/// (the handler validates first) rather than emit a wrong bucket.
fn chart_grain(interval: &str) -> (&'static str, &'static str, &'static str) {
    match interval {
        "1h" => (
            "toStartOfHour",
            "prices.price_usd_series_1h",
            "toStartOfHour",
        ),
        "1d" => ("toStartOfDay", "prices.price_usd_series", "toStartOfDay"),
        "1w" => ("toMonday", "prices.price_usd_series", "toStartOfDay"),
        _ => panic!(
            "chart called with non-allowlisted interval `{interval}` — handler \
             validation drift; expected 1h | 1d | 1w"
        ),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
