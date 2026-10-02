//! USD analytics for the pool endpoints: price identities, the last-close
//! lookup and the TVL / volume / fee arithmetic.

use clickhouse::Row;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};

use crate::common::asset_identity::{ResolvedAsset, resolve_asset_identities};

/// The prices identity of one leg, from the identity already resolved for it.
pub(super) fn price_leg_of(id: i64, identities: &HashMap<i64, ResolvedAsset>) -> PriceLeg {
    match identities.get(&id).filter(|r| r.known) {
        Some(r) => price_leg(r.asset_type, r.asset_code.as_deref(), r.issuer.as_deref()),
        None => price_leg(-1, None, None),
    }
}

// ---------------------------------------------------------------------------
// USD analytics (task 0199, ADR 0053 compute-at-read).
//
// `tvl` / `volume` / `fee_revenue` are computed at read time from on-chain
// inputs (`reserve_a/b`, `gross_volume_a`, `fee_bps` — all indexer-written)
// joined against the prices service's `prices.*` views in the same CH
// cluster. Nothing is materialized back into `liquidity_pool_snapshots`
// (the RMT has no version column; a write-back would race live inserts).
//
// JOIN interop contract (prices views.sql header, pinned 2026-06-16):
// key = (asset_kind, asset_code, issuer_address) with
// asset_kind ∈ ('native','credit','contract'); native XLM is
// ('native','XLM',''); bucket is a grain-floored DateTime. Grains provided:
// 1h + 1d only — the 1w chart interval joins the DAILY series.
//
// Two deliberate traps documented in the task
// (notes/R-prices-freeze-incident-and-current-price-usd-v13.md):
// - never join raw `prices.assets` (153 empty-code rows silently price
//   native legs as an arbitrary asset);
// - never decode a `prices.*` view positionally / via `SELECT *` — the
//   views grow additively (current_price_usd went 6 → 13 columns).
//
// LEFT JOIN misses surface as DEFAULT values, not NULL (`join_use_nulls`
// is rejected for the readonly API user — CH gotcha list), so every read
// wraps the price in `nullIf(price, 0)`.
//
// The views do NOT guarantee `close_usd > 0`: a bucket whose only candles
// carry zero volume can publish `Decimal128::MIN` (≈ -1.7e24) instead of
// omitting the row (prices-side 0171, confirmed by the owner 2026-08-11).
// A negative close would print a -1e24-scale TVL and, through the chart's
// ASOF carry-forward, smear it over every later bucket — so every
// `close_usd` read here filters `close_usd > 0` itself and treats
// non-positive rows as absent.
//
// USD arithmetic is Float64, rounded to cents. The analytics carry a 1%
// verification tolerance by design (task AC); Float64 keeps the SQL free
// of Decimal128×Decimal128 scale-overflow (7+14 fractional digits).
// **Every money value is formatted by [`usd_str`] on the Rust side** — SQL
// returns raw Float64. CH's `toString(round(x, 2))` emits "25" / "0" /
// "1.5" (variable decimals), which would put the chart and the detail
// endpoint on two different wire shapes for the same field.
//
// **Bounded price carry-forward.** A price bucket exists only once the
// asset trades in it, so the in-progress bucket is routinely missing for an
// illiquid leg — an exact bucket-equality join then NULLs the newest chart
// point, the one users read as "current". Reads therefore take the most
// recent close at or before the wanted bucket (CH `ASOF LEFT JOIN`), but
// only within [`MAX_PRICE_CARRY_SECONDS`]. Unbounded carry-forward would be
// worse than a hole: it would paint the 2026-07-21..08-03 provider freeze
// with a 12-day-old price and present it as live (box-checked — the ASOF
// match for 07-28 is a 07-21 candle).
// ---------------------------------------------------------------------------

/// How stale a price candle may be before it stops standing in for a
/// missing one, in seconds (48 h).
///
/// Covers the routine gap — the current bucket has no candle yet because
/// the asset has not traded in it — without masking a real outage. Shared
/// by the chart (carry-forward bound) and the detail endpoint (lookback
/// window) so both surfaces answer "what is this pool worth" from the same
/// staleness rule.
pub(super) const MAX_PRICE_CARRY_SECONDS: i64 = 48 * 3600;

/// Natural price identity of one pool leg in the exact column forms the
/// `prices.*` views expose. A leg that cannot be priced maps to empty
/// strings, which match no prices row → NULL analytics, never a wrong price.
///
/// `Hash` so it can key the [`fetch_last_closes`] result directly — the
/// per-row lookup on the list path then costs no allocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PriceLeg {
    pub kind: &'static str,
    pub code: String,
    pub issuer: String,
}

/// Map an LP leg (XDR `AssetType` + code + issuer G-strkey) to its prices
/// identity. LP legs are classic-only (`LiquidityPoolEntry`), so only
/// native (0) and credit_alphanum4/12 (1|2) occur; anything else — or a
/// credit leg missing its code/issuer — is unpriceable by construction.
pub fn price_leg(asset_type: i16, code: Option<&str>, issuer: Option<&str>) -> PriceLeg {
    match asset_type {
        0 => PriceLeg {
            kind: "native",
            code: "XLM".to_string(),
            issuer: String::new(),
        },
        1 | 2 => match (code, issuer) {
            (Some(c), Some(i)) if !c.is_empty() && !i.is_empty() => PriceLeg {
                kind: "credit",
                code: c.to_string(),
                issuer: i.to_string(),
            },
            _ => PriceLeg {
                kind: "",
                code: String::new(),
                issuer: String::new(),
            },
        },
        _ => PriceLeg {
            kind: "",
            code: String::new(),
            issuer: String::new(),
        },
    }
}

/// Pool inputs the chart's USD computation needs besides the snapshots:
/// both leg identities + the pool fee. Fetched once per chart request
/// (doubles as the 404 existence gate).
#[derive(Debug, Clone)]
pub struct PoolPriceContext {
    /// Every leg, in pool order. A pool prices only when ALL of them do — a
    /// partial sum understates it, which is why this is a vector rather than a
    /// pair even though the two-leg case is the common one.
    pub legs: Vec<PriceLeg>,
    pub fee_bps: i32,
}

/// What the chart needs about the pool: the price context, which chart it
/// gets (a soroban pool's reserves are raw and live in `pool_state_changes`),
/// and each leg's display scale — `None` where the token's scale is unknown.
#[derive(Debug, Clone)]
pub struct PoolChartContext {
    pub price: PoolPriceContext,
    pub pool_kind: domain::PoolKind,
    pub leg_decimals: Vec<Option<u32>>,
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PriceContextChRow {
    legs: Vec<i64>,
    fee_bps: i32,
    pool_kind: i16,
}

/// Resolve the pool's leg identities, `fee_bps`, kind and leg scales. `None`
/// = pool unknown (the chart handler's 404 gate — replaces `pool_exists`
/// there).
///
/// Leg identity comes from the shared resolver, which carries the issuer
/// StrKey with it — so the restricted-`iss` CTE this used to run (never
/// `accounts FINAL`: a 14M-row hash build, box-confirmed Code 241) is gone
/// along with the pair columns it keyed on.
pub async fn fetch_pool_chart_context(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<PoolChartContext>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT legs, fee_bps, toInt16(pool_kind) AS pool_kind FROM liquidity_pools FINAL \
             WHERE pool_id = unhex(?) LIMIT 1",
        )
        .bind(pool_id_hex)
        .fetch_optional::<PriceContextChRow>()
        .await?;

    let Some(r) = row else { return Ok(None) };
    let leg_ids: BTreeSet<i64> = r.legs.iter().copied().collect();
    let identities = resolve_asset_identities(client, &leg_ids).await?;

    Ok(Some(PoolChartContext {
        price: PoolPriceContext {
            legs: r
                .legs
                .iter()
                .map(|id| price_leg_of(*id, &identities))
                .collect(),
            fee_bps: r.fee_bps,
        },
        pool_kind: crate::common::strkey::decode_pool_kind(pool_id_hex, r.pool_kind),
        leg_decimals: r
            .legs
            .iter()
            .map(|id| identities.get(id).and_then(|i| i.decimals))
            .collect(),
    }))
}

/// Detail-endpoint USD analytics (task 0199 semantics, defined here because
/// the snapshot columns were never populated before this task):
/// - `tvl` — latest reserves × each leg's last hourly close
///   (`price_usd_series_1h`, [`MAX_PRICE_CARRY_SECONDS`] lookback); NULL
///   unless BOTH legs price (a one-leg TVL would silently halve the pool —
///   no-misleading-fallbacks rule).
/// - `volume` — last-24h gross volume of each traded leg × that leg's close:
///   a classic pool's `gross_volume_a`, a soroban pool's traded-leg amounts
///   (see [`fetch_pool_volume_24h`]). One price for the whole day, not
///   per-trade (upgrade path: per-ledger join as in the chart, if product
///   needs it).
/// - `fee_revenue` — [`fee_revenue_usd`].
///
/// Why the 1h series and NOT `prices.current_price_usd`: box-measured
/// 2026-08-04, the spot view is live (3,316 assets, updater ticking) but
/// `price_usd = 0` — the "unavailable" sentinel — for native XLM itself,
/// so every XLM-leg pool (the majority) would read NULL TVL. The last 1h
/// close costs the same (112 ms / 1.6M read rows vs 92 ms / 1.2M on the
/// hottest pool) and actually returns data. Revisit spot when the
/// prices-side updater (their 0039) prices native.
///
/// This endpoint and the chart apply the SAME staleness rule
/// ([`MAX_PRICE_CARRY_SECONDS`]) but not the same grain: detail always
/// reads the hourly series, while the chart reads the grain its interval
/// asks for. So the two agree on whether a pool is priceable, and may
/// differ by up to one chart bucket on the value — a 1d bucket closes on
/// its own daily candle, not on the latest hour.
#[derive(Debug, Default)]
pub struct PoolUsdAnalytics {
    pub tvl: Option<String>,
    pub volume: Option<String>,
    pub fee_revenue: Option<String>,
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct Vol24ChRow {
    vol24_a_units: Option<String>,
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct LegVol24ChRow {
    traded_leg: u64,
    raw_units: String,
}

/// Last-24h gross trade volume for one pool, as `(leg, units)`: how much of
/// each leg was traded, in that leg's own units (`leg` is 0-based, in pool
/// order). `None` when it is not knowable — an unparseable sum, or a
/// soroban leg that traded but whose token publishes no decimals.
///
/// A day with no trades is `[(0, 0.0)]`, not `[]`: a zero is reported in
/// leg A, so it still needs leg A's price to read "$0.00", as it always did.
///
/// - **Classic:** `liquidity_pool_snapshots.gross_volume_a`, which the
///   indexer already sums per ledger — always leg A. Deduped with `LIMIT 1
///   BY ledger_sequence` — same idiom as the chart — because RMT duplicate
///   versions of one `(pool, ledger)` row would double the sum.
/// - **Soroban:** `pool_movements`, the absolute amount of every trade's
///   **traded leg** — the lowest-index leg the trade moved — scaled by that
///   leg's decimals. In a two-leg pool every trade moves leg A, so this is
///   the leg the classic snapshot counts. In a three- or four-leg pool a
///   trade moves two of the legs, often not leg A, and is counted on the
///   first of the two. Deduped on the table's full sort key: the live writer
///   and the backfill write the same rows on purpose. Only trade events
///   count, as only claim atoms count in `gross_volume_a`.
///
/// **Both ledger bounds are required, and the upper one is not redundant.**
/// `min()`/`max()` over an empty set return the type DEFAULT (`0`), not
/// NULL — box-verified. With only the `>=` floor, a 24h window containing
/// no ledgers degrades to `ledger_sequence >= 0`, i.e. the pool's ENTIRE
/// history, and the endpoint reports lifetime volume as "24h volume". That
/// is reachable: ingestion has stalled for >16h before (galexie
/// protocol-upgrade stall, 2026-07-08) while the independent prices service
/// kept serving, so the spot price would still resolve and the inflated
/// number would render as real. Pairing the bounds makes the empty window
/// self-cancelling (`>= 0 AND <= 0` matches nothing), which is exactly why
/// the chart's equivalent floor was safe.
pub(super) async fn fetch_pool_volume_24h(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    pool_kind: domain::PoolKind,
    leg_decimals: &[Option<u32>],
) -> Result<Option<Vec<(usize, f64)>>, clickhouse::error::Error> {
    match pool_kind {
        domain::PoolKind::Classic => {
            let raw = client
                .query(
                    "SELECT toString(sum(gross_volume_a)) AS vol24_a_units FROM ( \
                         SELECT ledger_sequence, gross_volume_a \
                         FROM liquidity_pool_snapshots \
                         WHERE pool_id = unhex(?) \
                           AND ledger_sequence >= ( \
                               SELECT min(sequence) FROM ledgers \
                               WHERE closed_at >= now() - INTERVAL 24 HOUR) \
                           AND ledger_sequence <= ( \
                               SELECT max(sequence) FROM ledgers \
                               WHERE closed_at >= now() - INTERVAL 24 HOUR) \
                         ORDER BY ledger_sequence DESC \
                         LIMIT 1 BY ledger_sequence \
                     )",
                )
                .bind(pool_id_hex)
                .fetch_one::<Vol24ChRow>()
                .await?
                .vol24_a_units;
            // SQL NULL (no snapshot rows in the window, or no swaps among
            // them) is a genuine zero-volume day. A row that IS present but
            // unparseable is NOT — it is an unknown, and must not be reported
            // as "$0.00 traded".
            let units = match raw.as_deref() {
                None => Some(0.0),
                Some(raw) => parse_f64(raw),
            };
            Ok(units.map(|units| vec![(0, units)]))
        }
        domain::PoolKind::Soroban => {
            // The registry's `legs` is read without `FINAL`: a pool's legs are
            // fixed at registration, and no pool has two versions that differ
            // (0 on production, 2026-10-01). A trade's rows carry one row per
            // leg it moved; `min(leg)` picks the traded leg and `argMin` its
            // amount.
            let rows = client
                .query(
                    "SELECT traded_leg, toString(sum(abs(amount))) AS raw_units FROM ( \
                         SELECT min(leg) AS traded_leg, argMin(amount, leg) AS amount FROM ( \
                             SELECT ledger_sequence, application_order, operation_index, event_index, \
                                    indexOf((SELECT legs FROM liquidity_pools \
                                             WHERE pool_id = unhex(?) LIMIT 1), asset_id) AS leg, \
                                    amount \
                             FROM pool_movements \
                             WHERE pool_id = toFixedString(unhex(?), 32) \
                               AND event_kind = ? \
                               AND ledger_sequence >= ( \
                                   SELECT min(sequence) FROM ledgers \
                                   WHERE closed_at >= now() - INTERVAL 24 HOUR) \
                               AND ledger_sequence <= ( \
                                   SELECT max(sequence) FROM ledgers \
                                   WHERE closed_at >= now() - INTERVAL 24 HOUR) \
                             LIMIT 1 BY ledger_sequence, application_order, operation_index, \
                                        event_index, asset_id \
                         ) \
                         WHERE leg > 0 \
                         GROUP BY ledger_sequence, application_order, operation_index, event_index \
                     ) \
                     GROUP BY traded_leg \
                     ORDER BY traded_leg",
                )
                .bind(pool_id_hex)
                .bind(pool_id_hex)
                .bind(domain::PoolEvent::Trade as u8)
                .fetch_all::<LegVol24ChRow>()
                .await?;
            if rows.is_empty() {
                return Ok(Some(vec![(0, 0.0)]));
            }
            let mut per_leg = Vec::with_capacity(rows.len());
            for row in rows {
                let leg = row.traded_leg as usize - 1;
                let Some(Some(decimals)) = leg_decimals.get(leg) else {
                    return Ok(None);
                };
                let Some(raw) = parse_f64(&row.raw_units) else {
                    return Ok(None);
                };
                per_leg.push((leg, raw / 10f64.powi(*decimals as i32)));
            }
            Ok(Some(per_leg))
        }
    }
}

/// Traded units per leg × that leg's close, summed. `None` unless every leg
/// that traded has a price — a partial sum understates the volume while
/// looking like a real number.
fn volume_usd(
    per_leg: &[(usize, f64)],
    legs: &[PriceLeg],
    closes: &HashMap<PriceLeg, f64>,
) -> Option<f64> {
    let mut usd = 0.0;
    for (leg, units) in per_leg {
        let close = closes.get(legs.get(*leg)?)?;
        usd += units * close;
    }
    Some(usd)
}

/// Fetch last hourly closes + 24h gross volume, compute the detail USD
/// analytics in Rust (Float64 tolerance documented on the module block
/// above).
///
/// Prices come from the SAME [`fetch_last_closes`] primitive the list uses,
/// so the two surfaces cannot answer "is this pool priceable" differently —
/// and leg B stops re-scanning the window leg A already scanned. The two
/// queries are independent, so they overlap rather than run serially.
pub async fn fetch_pool_usd_analytics(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    pool_kind: domain::PoolKind,
    ctx: &PoolPriceContext,
    reserves: &[Option<&str>],
    leg_decimals: &[Option<u32>],
) -> Result<PoolUsdAnalytics, clickhouse::error::Error> {
    let legs = priceable_legs(ctx);
    let (closes, vol24) = tokio::join!(
        fetch_last_closes(client, &legs),
        fetch_pool_volume_24h(client, pool_id_hex, pool_kind, leg_decimals),
    );
    let closes = closes?;
    let vol24 = vol24?;

    let tvl = tvl_usd(reserves, &ctx.legs, &closes);
    let volume = match vol24 {
        Some(per_leg) => volume_usd(&per_leg, &ctx.legs, &closes),
        None => None,
    };
    let fee_revenue = volume.map(|v| fee_revenue_usd(v, ctx.fee_bps));

    Ok(PoolUsdAnalytics {
        tvl: tvl.map(usd_str),
        volume: volume.map(usd_str),
        fee_revenue: fee_revenue.map(usd_str),
    })
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct LastCloseChRow {
    asset_kind: String,
    asset_code: String,
    issuer_address: String,
    close_usd: Option<String>,
}

/// Batched last-hourly-close lookup for a page of pools (Phase A2, list-side
/// TVL — issue #367's literal ask). ONE query per page, never per row: the
/// prices views cannot prune by identity anyway (computed columns), so the
/// cost is one bounded [`MAX_PRICE_CARRY_SECONDS`] window scan regardless of
/// how many identities the page carries; the OR-chain only trims the result
/// set. Unpriceable legs (empty `kind`) are filtered out by the caller.
///
/// **The in-progress hour is excluded on purpose.** The prices service
/// bakes `close_usd` in a pass that trails candle ingestion, so a bucket
/// still being formed is only partly enriched and its volume-weighted
/// close is taken over whichever rows happen to be done — on 2026-08-05
/// that made a 0.764-unit dust print the entire price of yXLM's 13:00
/// hour (1.3085 against a true ~0.170) and quadrupled the pool's TVL on
/// the page. The prices owner confirmed the mechanism, that only the
/// forming bucket is affected, and that it repairs once the bucket
/// closes; a coverage gate is coming, and this guard should be revisited
/// then. Cost of the guard is up to one hour of freshness against a
/// [`MAX_PRICE_CARRY_SECONDS`] budget — nothing.
///
/// Returns `(kind, code, issuer) → close_usd`; identities with no priced
/// candle in the window are simply absent.
pub(super) async fn fetch_last_closes(
    client: &clickhouse::Client,
    legs: &[&PriceLeg],
) -> Result<std::collections::HashMap<PriceLeg, f64>, clickhouse::error::Error> {
    if legs.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let identity_or = std::iter::repeat_n(
        "(asset_kind = ? AND asset_code = ? AND issuer_address = ?)",
        legs.len(),
    )
    .collect::<Vec<_>>()
    .join(" OR ");
    let sql = format!(
        "SELECT asset_kind, asset_code, issuer_address, \
                toString(nullIf(argMaxIf(close_usd, bucket, close_usd > 0), 0)) AS close_usd \
         FROM prices.price_usd_series_1h \
         WHERE ({identity_or}) \
           AND bucket >= now() - INTERVAL {carry} SECOND \
           AND bucket <  toStartOfHour(now()) \
         GROUP BY asset_kind, asset_code, issuer_address",
        carry = MAX_PRICE_CARRY_SECONDS,
    );
    let mut query = client.query(&sql);
    for leg in legs {
        query = query
            .bind(leg.kind)
            .bind(leg.code.as_str())
            .bind(leg.issuer.as_str());
    }
    let rows = query.fetch_all::<LastCloseChRow>().await?;
    // Key by the CALLER's `PriceLeg`, not by the returned strings: the leg
    // owns the `&'static str` kind the caller will look up with, so callers
    // get an allocation-free `closes.get(leg)`. `legs` is at most two per
    // pool (≤ 2 × page), so the linear match back is trivial.
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let close = r.close_usd.as_deref().and_then(parse_f64)?;
            let leg = legs.iter().find(|l| {
                l.kind == r.asset_kind && l.code == r.asset_code && l.issuer == r.issuer_address
            })?;
            Some(((*leg).clone(), close))
        })
        .collect())
}

/// The pool's two legs, or `None` for a pool that does not have exactly two.
///
/// The price join is CLASSIC-shaped: two reserves, two closes, summed. A pool
/// with three or four legs is soroban, and pricing its first two would report a
/// TVL that understates the pool while looking like a real number — the
/// misleading-fallback class. `None` degrades the analytics to NULL, the same
/// answer an untracked asset gets.
///
/// This is the property [`PoolPriceContext`] promises ("a pool prices only when
/// ALL of them do"), so it lives on the context rather than at its caller, the
/// classic chart. It used to be a per-index accessor whose own doc claimed
/// this guarantee while `get(0)` / `get(1)` quietly provided the opposite.
pub(super) fn priced_pair(ctx: &PoolPriceContext) -> Option<(&PriceLeg, &PriceLeg)> {
    match ctx.legs.as_slice() {
        [a, b] => Some((a, b)),
        _ => None,
    }
}

/// The two legs of a pool as a `fetch_last_closes` input, with unpriceable
/// legs (empty `kind`) dropped — they match no prices row by construction,
/// so asking for them is pure waste.
fn priceable_legs(ctx: &PoolPriceContext) -> Vec<&PriceLeg> {
    ctx.legs.iter().filter(|l| !l.kind.is_empty()).collect()
}

/// A pool's TVL: every leg's reserve times its price, summed. `None` unless
/// EVERY leg has both — a partial sum understates the pool while looking like
/// a real number. `reserves[i]` and `legs[i]` describe the same leg: both
/// callers build the two from one leg list.
pub(super) fn tvl_usd(
    reserves: &[Option<&str>],
    legs: &[PriceLeg],
    closes: &HashMap<PriceLeg, f64>,
) -> Option<f64> {
    reserves
        .iter()
        .zip(legs)
        .map(|(r, leg)| Some(r.and_then(parse_f64)? * closes.get(leg).copied()?))
        .sum()
}

/// Strict decimal-string → f64 (the wire strings come from CH `toString`
/// over Decimal columns; anything non-parseable degrades to None, never 500).
fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// USD amount → wire string. The single formatter for every money field on
/// both LP surfaces — see the module note on why this is not done in SQL.
///
/// Cents for anything a cent or larger, and **significant digits below
/// that**, because a flat `{:.2}` reports a real value as `"0.00"` — a
/// number the client cannot tell from a genuine zero and cannot recover.
/// It is not a corner case: `fee_revenue` is 0.30% of the traded volume,
/// so any pool trading less than a few dollars a bucket serialises its
/// entire fee series as zeros (observed on prod — a pool with real volume
/// rendered every chart bucket and every axis tick as `$0`).
pub(super) fn usd_str(v: f64) -> String {
    let abs = v.abs();
    if abs > 0.0 && abs < 0.01 {
        // Two significant digits: 0.003 → "0.0030", 0.00009 → "0.000090".
        // Capped so a denormal cannot produce an absurdly long string.
        let places = ((-abs.log10()).ceil() as usize + 1).min(12);
        format!("{v:.places$}")
    } else {
        format!("{v:.2}")
    }
}

/// `volume × fee_bps / 10000` — the pool's cut of the traded volume.
/// `fee_bps` is basis points (30 = 0.30%), so the divisor is 10 000, not
/// 100. Shared by chart and detail so the two cannot drift.
pub(super) fn fee_revenue_usd(volume_usd: f64, fee_bps: i32) -> f64 {
    volume_usd * f64::from(fee_bps) / 10_000.0
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ch_tests;
