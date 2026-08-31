//! ClickHouse queries for the liquidity-pool endpoints (task 0243).
//!
//! Returns the `PoolRow` / `PoolTxRow` /
//! `ParticipantRow` / `ChartDataPoint` shapes, so the handlers reuse
//! `map_pool_item` / cursor builders unchanged after the fetch.
//!
//! CH-specific translation choices (see task 0243 handoff note):
//! - **Decimal128(7)** columns are read via `toString(...)` in SQL → wire
//!   decimal strings, sidestepping the clickhouse-rs Decimal decode gotcha.
//! - **`pool_id`** is a `FixedString(32)`; the wire/hex form is the 64-char
//!   lowercase hex. SQL compares with `pool_id = unhex(?)` and reads back
//!   `lower(hex(pool_id))`.
//! - **`created_at_ledger`** does NOT exist on CH `liquidity_pools` (dropped,
//!   see schema header) — derived as `min(ledger_sequence)` over the pool's
//!   snapshots, falling back to `last_updated_ledger` for a pool that somehow
//!   has no snapshot yet.
//! - **snapshot `created_at`** does NOT exist on CH `liquidity_pool_snapshots`
//!   (only `ledger_sequence`) — the latest-snapshot timestamp is derived from
//!   the joined `ledgers.closed_at`.
//! - The freshness window (PG: `snapshots.created_at >= NOW() - 7d`) is NOT
//!   applied on the detail/list latest-snapshot pick yet — detail takes the
//!   single latest snapshot regardless of age (matches the "latest known
//!   state" intent); a staleness cutoff is a follow-up if parity needs it.

use chrono::{DateTime, Utc};
use clickhouse::Row;
use db_clickhouse::persist::ids;
use serde::Deserialize;
use std::collections::HashMap;

use crate::common::ch::{millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};
use crate::common::pool_asset_codes::asset_codes_predicate;

use super::dto::{ChartDataPoint, PoolActivityCursor, PoolEvent, PoolListCursor, SharesCursor};

// ---------------------------------------------------------------------------
// Internal query-result rows + resolved params (not serialized; the handler
// maps these into the public response DTOs).
// ---------------------------------------------------------------------------

/// Canonical pool column projection shared between list and detail.
#[derive(Debug, Clone)]
pub struct PoolRow {
    pub pool_id_hex: String,
    pub asset_a_type: i16,
    pub asset_a_type_name: Option<String>,
    pub asset_a_code: Option<String>,
    pub asset_a_issuer: Option<String>,
    /// C-strkey of the SAC mirror for the asset-A leg. `None` otherwise (task 0263).
    pub asset_a_contract_id: Option<String>,
    /// `icon_url` for the asset-A leg, from `asset_enrichment` (ADR 0050).
    /// NOT from `assets` — task 0310 dropped that dead column from prod.
    pub asset_a_icon_url: Option<String>,
    pub asset_b_type: i16,
    pub asset_b_type_name: Option<String>,
    pub asset_b_code: Option<String>,
    pub asset_b_issuer: Option<String>,
    /// C-strkey of the SAC mirror for the asset-B leg. See `asset_a_contract_id`.
    pub asset_b_contract_id: Option<String>,
    /// `icon_url` for the asset-B leg. See `asset_a_icon_url`.
    pub asset_b_icon_url: Option<String>,
    pub fee_bps: i32,
    pub fee_percent: String,
    pub created_at_ledger: i64,
    /// Ledger value the list keyset orders + paginates on. CH keys on the
    /// native `last_updated_ledger` ("most recently active"), carried here.
    /// The wire `PoolListCursor.created_at_ledger` slot stays opaque (ADR
    /// 0008); only this field feeds the cursor builder. Unused by detail.
    pub cursor_ledger: i64,
    /// `COUNT(*) FROM lp_positions WHERE pool_id = lp.pool_id AND shares > 0`.
    /// Task 0246 — see DTO doc for surfacing rules. `None` for soroban pools
    /// (their participants live in `balances` by share token, not
    /// `lp_positions` — see the DTO doc; `None` ≠ 0).
    pub participant_count: Option<i64>,
    /// `liquidity_pools.pool_kind`: 0 = classic (CAP-38), 1 = soroban AMM
    /// contract (task 0374). Decides which leg representation the handler
    /// builds — the pair columns on a soroban row are storage defaults, not
    /// legs, and must not be surfaced.
    pub pool_kind: u8,
    /// `soroban_contracts.id` surrogate of the registering router; 0 on
    /// classic rows. Protocol labels resolve from it at read time.
    pub deployment_id: i64,
    /// Verbatim `pool_type` sym from `add_pool`; empty on classic rows.
    pub pool_type_raw: String,
    /// Token-contract surrogates per leg in emission order; empty on
    /// classic rows.
    pub legs: Vec<i64>,
    pub latest_snapshot_ledger: Option<i64>,
    pub reserve_a: Option<String>,
    pub reserve_b: Option<String>,
    pub total_shares: Option<String>,
    pub tvl: Option<String>,
    pub volume: Option<String>,
    pub fee_revenue: Option<String>,
    pub latest_snapshot_at: Option<DateTime<Utc>>,
}

/// One current LP participant (a positive-shares position). Handler strips the
/// surrogate before building the API response.
#[derive(Debug)]
pub struct ParticipantRow {
    /// StrKey resolved by surrogate: `G...` via `accounts`; for a soroban
    /// pool's share-token holder possibly `C...` via `soroban_contracts`.
    pub account: String,
    /// `accounts.id` / holder surrogate BIGINT — used only to encode the
    /// next cursor; not exposed in the response DTO.
    pub account_id_surrogate: i64,
    /// DISPLAY value. Classic: decimal text preserving `NUMERIC(28,7)`
    /// (pre-scaled DB-side). Soroban: the share-token balance scaled by the
    /// token's on-chain metadata decimals; `None` when the token never
    /// published decimals — an unknown scale must surface as absent, never
    /// as a raw integer masquerading as a scaled amount.
    pub shares: Option<String>,
    /// Keyset value the next cursor carries — ALWAYS the database-side form
    /// (classic decimal text / soroban raw integer), independent of how
    /// `shares` displays. The cursor must round-trip into the SQL keyset,
    /// which compares against the stored column, not the display scale.
    pub cursor_shares: String,
    /// `100 * shares / total_pool_shares`, NULL when the pool has no snapshot
    /// in the 7-day freshness window. Already a decimal string.
    pub share_percentage: Option<String>,
    /// `None` on the soroban path (`balances` records state, not first
    /// sighting).
    pub first_deposit_ledger: Option<i64>,
    pub last_updated_ledger: i64,
}

/// Resolved, validated `GET /v1/liquidity-pools` list params.
pub struct ResolvedPoolListParams {
    pub limit: i64,
    pub cursor: Option<PoolListCursor>,
    pub asset_a_code: Option<String>,
    pub asset_a_issuer: Option<String>,
    pub asset_b_code: Option<String>,
    pub asset_b_issuer: Option<String>,
    /// Free-text asset filter (task 0246, widened in 0440) — trimmed and
    /// uppercased at the handler boundary, then split on `/` into at most two
    /// needles. Every needle must appear on *some* leg, which makes a pair
    /// query order-insensitive without anyone knowing Stellar's canonical leg
    /// ordering. Empty = no filter.
    pub asset_codes: Vec<String>,
    /// The same free-text box, when it held a pool identifier instead
    /// (`L…` SEP-23 StrKey, the one canonical form per task 0264, resolved to
    /// the stored hex — task 0470).
    /// Mutually exclusive with `asset_codes`: an identifier names exactly one
    /// pool, so there is nothing left for a code match to narrow.
    pub pool_id_hex: Option<String>,
    /// `filter[pool_kind]` resolved to the stored discriminant: 0 classic,
    /// 1 soroban. Handler-validated; `None` = both worlds (task 0374).
    pub pool_kind: Option<u8>,
}

/// One activity row after enrichment — the handler maps this straight into
/// `PoolActivityItem` (task 0491).
#[derive(Debug, Clone)]
pub struct PoolActivityRow {
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    /// Surrogate `transactions.id`. Not on the wire — it is the cursor's
    /// middle component, the same tie-break the sort key uses.
    pub transaction_id: i64,
    /// `None` on the soroban feed — contract events have no per-op anchor.
    pub application_order: Option<i16>,
    pub event: Option<PoolEvent>,
    pub amount_a: Option<String>,
    pub amount_b: Option<String>,
    pub source_account: String,
    /// How many pools the whole operation crossed (`length(pool_ids)` off the
    /// same appearance seek that resolves the op source). `None` = unknowable
    /// (no appearance row), never guessed to `1`.
    pub pools_crossed: Option<i64>,
    /// Soroban feed only: per-leg movements `(leg_index, signed raw amount)`.
    pub leg_amounts: Option<Vec<(u32, String)>>,
    pub created_at: DateTime<Utc>,
    /// Soroban feed only: the event's index within its transaction — the
    /// cursor's final keyset component there. 0 on classic rows.
    pub event_index: i64,
}

/// 7-day freshness window expressed in ledgers (~17280 ledgers/day at the
/// ~5 s mainnet cadence). The PG path uses `snapshots.created_at >= NOW() - 7d`;
/// CH `liquidity_pool_snapshots` carries no `created_at`, so the window is
/// approximated by a `ledger_sequence` floor relative to chain head. Exact
/// wall-clock parity is a documented tolerance (freshness is a stale/fresh
/// heuristic, not an exact cutoff).
const FRESHNESS_WINDOW_LEDGERS: i64 = 7 * 17_280;

/// `true` if `s` is a plain decimal string (digits, at most one `.`, optional
/// leading `-`). Cursor `shares` is decoded from an opaque payload and inlined
/// into the keyset SQL (to dodge the clickhouse-rs None-into-tuple bind defect,
/// same as accounts/contracts); validating it first keeps that inline safe.
fn is_decimal_str(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && body.bytes().filter(|&b| b == b'.').count() <= 1
}

/// `true` if `s` is a 64-char lowercase-hex `pool_id` (the wire form, decoded
/// from the opaque list cursor). Guards the inlined keyset bound: a `pool_id`
/// from a tampered cursor that is not clean hex degrades to "no keyset" (first
/// page) rather than reaching the SQL string. Same rationale as
/// [`is_decimal_str`] for the participants cursor.
fn is_hex_pool_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `asset_type` SMALLINT → label, matching the PG `asset_type_name()` SQL
/// function (migration `20260422000000_enum_label_functions`) — the XDR
/// `AssetType`, which is what LP legs carry. NOT `asset_family_name`
/// (native/classic_credit/sac/soroban); the `PoolAssetLeg` doc-comment quotes
/// that sibling function and is misleading for pool legs. Box-confirmed: a
/// 9-char code (`WGUARDIAN`) is `asset_type = 2` = credit_alphanum12, not sac.
/// Out-of-range → `None` (PG `CASE` returns NULL with no `ELSE`).
fn asset_type_name(asset_type: i16) -> Option<String> {
    match asset_type {
        0 => Some("native".to_string()),
        1 => Some("credit_alphanum4".to_string()),
        2 => Some("credit_alphanum12".to_string()),
        3 => Some("pool_share".to_string()),
        _ => None,
    }
}

/// `fee_bps / 100` as a decimal string (e.g. 30 → "0.3", 25 → "0.25",
/// 100 → "1"). Computed in Rust to avoid CH integer-division / decimal-scale
/// quirks; trailing zeros are trimmed.
///
/// NOTE: PG emits `(fee_bps::numeric / 100)::text`; exact trailing-zero
/// parity is a documented box-smoke check (cosmetic field, FE re-renders).
fn fee_percent_str(fee_bps: i32) -> String {
    let whole = fee_bps / 100;
    let frac = (fee_bps % 100).abs();
    if frac == 0 {
        whole.to_string()
    } else if frac % 10 == 0 {
        format!("{whole}.{}", frac / 10)
    } else {
        format!("{whole}.{frac:02}")
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
const MAX_PRICE_CARRY_SECONDS: i64 = 48 * 3600;

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
    pub leg_a: PriceLeg,
    pub leg_b: PriceLeg,
    pub fee_bps: i32,
    /// 0 = classic, 1 = soroban. The chart handler branches on it — a
    /// soroban pool's history lives in `pool_state_changes`, and its legs
    /// price through [`soroban_chart_legs`], not the pair identities above.
    pub pool_kind: u8,
    /// Soroban leg surrogates (`legs` column); empty for classic rows. Read
    /// here so the soroban chart branch needs no second dimension seek.
    pub legs: Vec<i64>,
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PriceContextChRow {
    asset_a_type: i16,
    asset_a_code: String,
    asset_a_issuer: String,
    asset_b_type: i16,
    asset_b_code: String,
    asset_b_issuer: String,
    fee_bps: i32,
    pool_kind: u8,
    legs: Vec<i64>,
}

/// Resolve the pool's leg identities + `fee_bps`. `None` = pool unknown
/// (the chart handler's 404 gate — replaces `pool_exists` there).
///
/// Issuer resolution reuses the detail query's restricted-`iss` idiom:
/// never `accounts FINAL` joins (14M-row hash build, box-confirmed
/// Code 241) — restrict to the pool's ≤2 issuer ids, `GROUP BY id` +
/// `any()` (account_id is stable across RMT versions).
pub async fn fetch_pool_price_context(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<PoolPriceContext>, clickhouse::error::Error> {
    let row = client
        .query(
            "WITH legs AS ( \
                 SELECT asset_a_type, asset_a_code, asset_a_issuer_id, \
                        asset_b_type, asset_b_code, asset_b_issuer_id, fee_bps, pool_kind, legs \
                 FROM liquidity_pools FINAL WHERE pool_id = unhex(?) \
             ), \
             iss AS ( \
                 SELECT id, any(account_id) AS account_id FROM accounts \
                 WHERE id IN (SELECT asset_a_issuer_id FROM legs WHERE asset_a_issuer_id != 0 \
                              UNION ALL SELECT asset_b_issuer_id FROM legs WHERE asset_b_issuer_id != 0) \
                 GROUP BY id \
             ) \
             SELECT \
                legs.asset_a_type            AS asset_a_type, \
                legs.asset_a_code            AS asset_a_code, \
                iss_a.account_id             AS asset_a_issuer, \
                legs.asset_b_type            AS asset_b_type, \
                legs.asset_b_code            AS asset_b_code, \
                iss_b.account_id             AS asset_b_issuer, \
                legs.fee_bps                 AS fee_bps, \
                legs.pool_kind               AS pool_kind, \
                legs.legs                    AS legs \
             FROM legs \
             LEFT JOIN iss iss_a ON iss_a.id = legs.asset_a_issuer_id \
             LEFT JOIN iss iss_b ON iss_b.id = legs.asset_b_issuer_id \
             LIMIT 1",
        )
        .bind(pool_id_hex)
        .fetch_optional::<PriceContextChRow>()
        .await?;

    Ok(row.map(|r| PoolPriceContext {
        leg_a: price_leg(
            r.asset_a_type,
            Some(&r.asset_a_code),
            Some(&r.asset_a_issuer),
        ),
        leg_b: price_leg(
            r.asset_b_type,
            Some(&r.asset_b_code),
            Some(&r.asset_b_issuer),
        ),
        fee_bps: r.fee_bps,
        pool_kind: r.pool_kind,
        legs: r.legs,
    }))
}

/// Detail-endpoint USD analytics (task 0199 semantics, defined here because
/// the snapshot columns were never populated before this task):
/// - `tvl` — latest reserves × each leg's last hourly close
///   (`price_usd_series_1h`, [`MAX_PRICE_CARRY_SECONDS`] lookback); NULL
///   unless BOTH legs price (a one-leg TVL would silently halve the pool —
///   no-misleading-fallbacks rule).
/// - `volume` — last-24h `gross_volume_a` × the same leg-A close. One
///   price for the whole day, not per-trade (upgrade path: per-ledger join
///   as in the chart, if product needs it).
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

/// Last-24h gross trade volume for one pool, in asset-A units.
///
/// Deduped with `LIMIT 1 BY ledger_sequence` — same idiom as the chart —
/// because RMT duplicate versions of one `(pool, ledger)` row would double
/// the sum.
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
async fn fetch_pool_volume_24h(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<String>, clickhouse::error::Error> {
    let row = client
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
        .await?;
    Ok(row.vol24_a_units)
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
    ctx: &PoolPriceContext,
    reserve_a: Option<&str>,
    reserve_b: Option<&str>,
) -> Result<PoolUsdAnalytics, clickhouse::error::Error> {
    let legs = priceable_legs(ctx);
    let (closes, vol24_raw) = tokio::join!(
        fetch_last_closes(client, &legs),
        fetch_pool_volume_24h(client, pool_id_hex),
    );
    let closes = closes?;
    let spot_a = closes.get(&ctx.leg_a).copied();
    let spot_b = closes.get(&ctx.leg_b).copied();
    // SQL NULL (no snapshot rows in the window, or no swaps among them) is a
    // genuine zero-volume day. A row that IS present but unparseable is NOT —
    // it is an unknown, and must not be reported as "$0.00 traded".
    let vol24_units = match vol24_raw?.as_deref() {
        None => Some(0.0),
        Some(raw) => parse_f64(raw),
    };

    let tvl = match (
        reserve_a.and_then(parse_f64),
        reserve_b.and_then(parse_f64),
        spot_a,
        spot_b,
    ) {
        (Some(ra), Some(rb), Some(pa), Some(pb)) => Some(ra * pa + rb * pb),
        _ => None,
    };
    let volume = match (spot_a, vol24_units) {
        (Some(pa), Some(units)) => Some(units * pa),
        _ => None,
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
async fn fetch_last_closes(
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

/// The two legs of a pool as a `fetch_last_closes` input, with unpriceable
/// legs (empty `kind`) dropped — they match no prices row by construction,
/// so asking for them is pure waste.
fn priceable_legs(ctx: &PoolPriceContext) -> Vec<&PriceLeg> {
    [&ctx.leg_a, &ctx.leg_b]
        .into_iter()
        .filter(|l| !l.kind.is_empty())
        .collect()
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
fn usd_str(v: f64) -> String {
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
fn fee_revenue_usd(volume_usd: f64, fee_bps: i32) -> f64 {
    volume_usd * f64::from(fee_bps) / 10_000.0
}

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PoolDetailChRow {
    pool_id_hex: String,
    asset_a_type: i16,
    asset_a_code: Option<String>,
    asset_a_issuer: Option<String>,
    asset_a_contract_id: Option<String>,
    asset_a_icon_url: Option<String>,
    asset_b_type: i16,
    asset_b_code: Option<String>,
    asset_b_issuer: Option<String>,
    asset_b_contract_id: Option<String>,
    asset_b_icon_url: Option<String>,
    fee_bps: i32,
    created_at_ledger: i64,
    participant_count: i64,
    latest_snapshot_ledger: Option<i64>,
    reserve_a: Option<String>,
    reserve_b: Option<String>,
    total_shares: Option<String>,
    latest_snapshot_at_ms: Option<i64>,
    pool_kind: u8,
    deployment_id: i64,
    pool_type_raw: String,
    legs: Vec<i64>,
}

/// `GET /v1/liquidity-pools/:id` — single-pool detail. Mirrors the PG
/// `fetch_pool_by_id` projection. `tvl`/`volume`/`fee_revenue` are NOT read
/// here — the snapshot columns were never populated (pre-0199 design); the
/// handler fills them from [`fetch_pool_usd_analytics`] (compute-at-read).
pub async fn fetch_pool_by_id(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<PoolRow>, clickhouse::error::Error> {
    // `unhex(?)` appears 5×: the `legs` CTE, the created_at-ledger subquery, the
    // participant-count subquery, the latest-snapshot subquery, and the outer
    // WHERE. All scoped to the literal pool id (NOT correlated to `lp`) since
    // detail is single-pool and CH dislikes correlated subqueries. Each `?`
    // consumes one positional bind; all are the same value, so order is moot.
    //
    // `legs` resolves the pool's two `(code, issuer_id)` pairs once; `iss` and
    // `sac` both fan out from it.
    //
    // **Issuer resolution is a restricted `iss` CTE, NOT `accounts FINAL`
    // joins.** `accounts` is `ORDER BY (account_id)`, so the surrogate `id` is a
    // non-PK reverse lookup; a plain `LEFT JOIN accounts FINAL` builds the whole
    // 14M-row table into the hash — and detail does it for BOTH legs, blowing
    // the 3.73 GiB per-query cap (box-confirmed `Code 241`). Restricting to the
    // pool's ≤2 issuer ids + `GROUP BY id` (no FINAL — account_id is stable
    // across RMT versions, `any()` is safe) scans the id column but builds a
    // ≤2-row hash. Same shape as `fetch_pool_list`'s `iss` CTE.
    //
    // SAC mirror + icon_url (task 0263 + 0274 gap #5 → ADR 0051): the `sac` CTE
    // resolves `(asset_code, issuer_id)` → `(contract_id, icon_url)` once per leg,
    // deduped by GROUP BY so a leg cannot fan the result out (the inline-join
    // form did, masked only by the outer LIMIT 1). Post-ADR 0051 the SAC handle
    // is a FACET in the `asset_sac` side table (not a column on `assets`, and not
    // a separate `asset_type = 2`) — so the deployed SAC's `C…` StrKey resolves by
    // two hops: leg `(code, issuer)` → `asset_sac.sac_contract_id` (surrogate) →
    // `soroban_contracts.contract_id` (un-deployed SACs have no contract row →
    // NULL, as before). The classic carrier is `asset_type IN (0, 1)`.
    //
    // **Native legs are IN the join** (task 0470). They used to be excluded by an
    // `asset_code != ''` guard on every arm, on the assumption that an empty code
    // could match nothing — but native XLM has a deployed SAC like any other
    // classic asset, so the guard was hiding a real answer: the leg reported a
    // NULL `contract_id` for an asset that has one. `('', 0)` is a safe join key
    // here, measured: it is exactly one asset across `asset_type IN (0, 1)`,
    // since a classic credit code is 1–12 characters by protocol. The icon stays
    // NULL, but because `asset_enrichment` holds no native row at all — not
    // because a guard forbids the lookup.
    //
    // **Latest snapshot subquery — NO `FINAL`** (0356 / PR #318). The indexer now
    // writes exactly one deterministic row per `(pool_id, ledger_sequence)`, so
    // `FINAL` is redundant for dedup; dropping it turns the read into a bounded
    // reverse-PK seek (`ORDER BY ledger_sequence DESC LIMIT 1`) instead of a
    // whole-table merge. It stays a whole-row `LIMIT 1` (not per-column
    // `argMax`), so `reserve_a`/`reserve_b` can never tear across a stale
    // before/after pair in the pre-cleanup window. `created_at_ledger` already
    // reads without `FINAL` (`min(ledger_sequence)` is dup-invariant).
    //
    // **`ledgers` is SEEKED, never joined whole.** `LEFT JOIN ledgers l ON
    // l.sequence = s.ledger_sequence` hash-built the entire 26M-row table to
    // resolve ONE `closed_at` — 27.2M read_rows / 1.82 GiB / ~724 ms of CH per
    // request under load (96% of this endpoint's cost, measured 2026-07-17 at
    // 50M/mo). Restricting the right side to the single sequence the snapshot
    // subquery points at makes it a PK point read.
    //
    // The `s` join is an EQUI-join on `pool_id` (not `ON 1 = 1`): a constant ON
    // condition is only supported by `join_algorithm = 'hash'`, so the old form
    // 500'd (Code 48) the moment the server profile carried anything else.
    let row = client
        .query(
            "WITH legs AS ( \
                 SELECT asset_a_code, asset_a_issuer_id, asset_b_code, asset_b_issuer_id \
                 FROM liquidity_pools FINAL WHERE pool_id = unhex(?) \
             ), \
             iss AS ( \
                 SELECT id, any(account_id) AS account_id FROM accounts \
                 WHERE id IN (SELECT asset_a_issuer_id FROM legs WHERE asset_a_issuer_id != 0 \
                              UNION ALL SELECT asset_b_issuer_id FROM legs WHERE asset_b_issuer_id != 0) \
                 GROUP BY id \
             ), \
             sac AS ( \
                 SELECT a.asset_code AS asset_code, a.issuer_id AS issuer_id, \
                        max(sc.contract_id)      AS contract_id, \
                        nullIf(max(ae.icon_url), '') AS icon_url \
                 FROM assets a \
                 LEFT JOIN ( \
                     SELECT asset_type, asset_code, issuer_id, contract_id, \
                            max(sac_contract_id) AS sac_contract_id \
                     FROM asset_sac GROUP BY asset_type, asset_code, issuer_id, contract_id \
                 ) asac ON asac.asset_type = a.asset_type AND asac.asset_code = a.asset_code \
                       AND asac.issuer_id = a.issuer_id AND asac.contract_id = a.contract_id \
                 LEFT JOIN soroban_contracts sc ON sc.id = asac.sac_contract_id AND asac.sac_contract_id != 0 \
                 LEFT JOIN ( \
                     SELECT asset_type, asset_code, issuer_id, contract_id, \
                            argMax(icon_url, version) AS icon_url \
                     FROM asset_enrichment \
                     WHERE asset_type IN (0, 1) AND asset_code IN ( \
                         SELECT asset_a_code FROM legs \
                         UNION ALL SELECT asset_b_code FROM legs) \
                     GROUP BY asset_type, asset_code, issuer_id, contract_id \
                 ) ae ON ae.asset_type = a.asset_type AND ae.asset_code = a.asset_code \
                     AND ae.issuer_id = a.issuer_id AND ae.contract_id = a.contract_id \
                 WHERE a.asset_type IN (0, 1) \
                   AND (a.asset_code, a.issuer_id) IN ( \
                       SELECT asset_a_code, asset_a_issuer_id FROM legs \
                       UNION ALL \
                       SELECT asset_b_code, asset_b_issuer_id FROM legs) \
                 GROUP BY a.asset_code, a.issuer_id \
             ) \
             SELECT \
                lower(hex(lp.pool_id))               AS pool_id_hex, \
                lp.asset_a_type                      AS asset_a_type, \
                nullIf(lp.asset_a_code, '')          AS asset_a_code, \
                nullIf(iss_a.account_id, '')         AS asset_a_issuer, \
                nullIf(sac_a.contract_id, '')        AS asset_a_contract_id, \
                sac_a.icon_url                       AS asset_a_icon_url, \
                lp.asset_b_type                      AS asset_b_type, \
                nullIf(lp.asset_b_code, '')          AS asset_b_code, \
                nullIf(iss_b.account_id, '')         AS asset_b_issuer, \
                nullIf(sac_b.contract_id, '')        AS asset_b_contract_id, \
                sac_b.icon_url                       AS asset_b_icon_url, \
                lp.fee_bps                           AS fee_bps, \
                ifNull( \
                    (SELECT min(ledger_sequence) FROM liquidity_pool_snapshots \
                      WHERE pool_id = unhex(?)), \
                    lp.last_updated_ledger)          AS created_at_ledger, \
                toInt64(ifNull( \
                    (SELECT count() FROM lp_positions FINAL \
                      WHERE pool_id = unhex(?) AND shares > 0), 0)) AS participant_count, \
                s.ledger_sequence                    AS latest_snapshot_ledger, \
                toString(s.reserve_a)                AS reserve_a, \
                toString(s.reserve_b)                AS reserve_b, \
                toString(s.total_shares)             AS total_shares, \
                nullIf(toUnixTimestamp64Milli(l.closed_at), 0) AS latest_snapshot_at_ms, \
                lp.pool_kind                         AS pool_kind, \
                lp.deployment_id                     AS deployment_id, \
                lp.pool_type_raw                     AS pool_type_raw, \
                lp.legs                              AS legs \
             FROM liquidity_pools lp FINAL \
             LEFT JOIN iss iss_a ON iss_a.id = lp.asset_a_issuer_id \
             LEFT JOIN iss iss_b ON iss_b.id = lp.asset_b_issuer_id \
             LEFT JOIN sac sac_a ON sac_a.asset_code = lp.asset_a_code \
                                AND sac_a.issuer_id = lp.asset_a_issuer_id \
             LEFT JOIN sac sac_b ON sac_b.asset_code = lp.asset_b_code \
                                AND sac_b.issuer_id = lp.asset_b_issuer_id \
             LEFT JOIN ( \
                 SELECT pool_id, \
                        toNullable(ledger_sequence) AS ledger_sequence, \
                        toNullable(reserve_a)       AS reserve_a, \
                        toNullable(reserve_b)       AS reserve_b, \
                        toNullable(total_shares)    AS total_shares \
                 FROM liquidity_pool_snapshots \
                 WHERE pool_id = unhex(?) \
                 ORDER BY ledger_sequence DESC \
                 LIMIT 1 \
             ) s ON s.pool_id = lp.pool_id \
             LEFT JOIN ( \
                 SELECT sequence, closed_at FROM ledgers \
                 WHERE sequence = (SELECT max(ledger_sequence) FROM liquidity_pool_snapshots \
                                    WHERE pool_id = unhex(?)) \
             ) l ON l.sequence = s.ledger_sequence \
             WHERE lp.pool_id = unhex(?) \
             LIMIT 1",
        )
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .fetch_optional::<PoolDetailChRow>()
        .await?;

    Ok(row.map(|r| PoolRow {
        pool_id_hex: r.pool_id_hex,
        asset_a_type: r.asset_a_type,
        asset_a_type_name: asset_type_name(r.asset_a_type),
        asset_a_code: r.asset_a_code,
        asset_a_issuer: r.asset_a_issuer,
        asset_a_contract_id: r.asset_a_contract_id,
        asset_a_icon_url: r.asset_a_icon_url,
        asset_b_type: r.asset_b_type,
        asset_b_type_name: asset_type_name(r.asset_b_type),
        asset_b_code: r.asset_b_code,
        asset_b_issuer: r.asset_b_issuer,
        asset_b_contract_id: r.asset_b_contract_id,
        asset_b_icon_url: r.asset_b_icon_url,
        fee_bps: r.fee_bps,
        fee_percent: fee_percent_str(r.fee_bps),
        created_at_ledger: r.created_at_ledger,
        // Detail does not paginate; the field is set for struct completeness.
        cursor_ledger: r.created_at_ledger,
        // Soroban pools: lp_positions holds nothing for them, so the SQL's
        // count is a true 0 of the WRONG population — mask to None (≠ 0).
        participant_count: (r.pool_kind == 0).then_some(r.participant_count),
        pool_kind: r.pool_kind,
        deployment_id: r.deployment_id,
        pool_type_raw: r.pool_type_raw,
        legs: r.legs,
        latest_snapshot_ledger: r.latest_snapshot_ledger,
        reserve_a: r.reserve_a,
        reserve_b: r.reserve_b,
        total_shares: r.total_shares,
        // Filled by the handler from `fetch_pool_usd_analytics` (0199
        // compute-at-read); the snapshot columns are not read.
        tvl: None,
        volume: None,
        fee_revenue: None,
        latest_snapshot_at: r.latest_snapshot_at_ms.map(millis_to_utc),
    }))
}

// `pool_exists` retired with task 0374: every gate that used it now needs
// the pool's KIND too, so existence rides `fetch_pool_kind_share` /
// `fetch_pool_asset_ids` / `fetch_pool_price_context` instead.

#[derive(Debug, Row, Deserialize)]
struct PoolLegsChRow {
    asset_a_type: i16,
    asset_a_code: String,
    asset_a_issuer_id: i64,
    asset_b_type: i16,
    asset_b_code: String,
    asset_b_issuer_id: i64,
    pool_kind: u8,
    legs: Vec<i64>,
}

/// The pool's two legs as `ids::asset_id` surrogates — the key
/// `lp_operation_amounts.asset_id` is written with (task 0279), so a row's
/// asset maps onto the A/B legs the page already renders. `None` when the pool
/// does not exist, which is also this seek's existence check (it replaces a
/// separate [`pool_exists`] round-trip on that path).
///
/// Resolved in Rust, not SQL: the surrogate is `cityhash_102_128`'s lower half
/// and CH's builtin `cityHash64` is a DIFFERENT algorithm (see the schema
/// header), so the writer's helper is the only way to reproduce the key.
///
/// Via [`ids::pool_leg_asset_id`], NOT `ids::asset_id` — `liquidity_pools`
/// stores the XDR asset type, where `2` is `credit_alphanum12`, while
/// `asset_id` reads `2` as the retired SAC facet and returns `0` for it.
/// One resolved Soroban pool leg (task 0374, step 13).
///
/// `legs` on a `pool_kind = 1` row stores contract SURROGATES in emission
/// order. A surrogate resolves along exactly one of two paths, measured on
/// the full registry at 1005/1005 leg occurrences:
///
/// * **SAC hop** (962) — the leg is a Stellar Asset Contract; per ADR 0051 a
///   SAC has NO `assets` row of its own, so identity comes from the
///   `asset_sac` facet: `sac_contract_id → (asset_type, code, issuer)`.
///   Covers native too (XLM's SAC maps to `(0, '', 0)`).
/// * **Direct token** (43) — a bespoke Soroban token; its `assets` row is
///   keyed by the same surrogate (`asset_type = 3`).
///
/// There is no third case BY CONSTRUCTION: `add_pool` registers a vector of
/// token CONTRACT addresses, so a classic asset can only ever appear as a leg
/// through its deployed SAC — "classic without a SAC" is not representable as
/// a Soroban pool leg, and native is just the SAC case with identity
/// `(0, '', 0)`.
///
/// A surrogate resolving through NEITHER path is representable (`None` in the
/// map) and must render as an explicit unresolved marker — never as a
/// plausible empty code (house rule: no misleading fallbacks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedLeg {
    /// `AssetFamily` discriminant of the resolved asset row: 0 native,
    /// 1 classic credit, 3 soroban. NOT the XDR `AssetType` (task 0496).
    pub family: i16,
    /// Classic asset code; empty for native and for soroban tokens.
    pub asset_code: String,
    /// `accounts.id` surrogate of the classic issuer; 0 for native/soroban.
    pub issuer_id: i64,
    /// The contract surrogate the leg came in as (SAC or token).
    pub contract_id: i64,
    /// Display scale (task 0374, step 14). `Some(7)` for every classic-family
    /// leg — the protocol fixes classic amounts at 7 decimal places, so no
    /// lookup is involved. For a soroban token it comes from on-chain
    /// `soroban_contract_metadata`; `None` means the token never published
    /// metadata — an 18-decimal token with NO metadata row exists on mainnet
    /// in a live stable pool, so this case is real, and it must render as an
    /// explicit "unresolved" marker, never as a plausible default scale
    /// (amounts shown 10^11 off look like data, not like a bug).
    pub decimals: Option<u32>,
    /// On-chain SEP-41 symbol — the display handle for a bespoke token,
    /// exactly as the assets page resolves it (metadata, task 0297/0304).
    /// `None` for classic legs (their handle is `asset_code`) and for tokens
    /// that never published metadata.
    pub symbol: Option<String>,
    /// On-chain SEP-41 name; same sourcing and caveats as `symbol`.
    pub name: Option<String>,
    /// C-strkey of the leg's token contract (`soroban_contracts` hop) — the
    /// address `add_pool` registered, for linking. `None` when the contract
    /// dimension has no row for the surrogate.
    pub contract_strkey: Option<String>,
}

/// Both arms GROUP BY their key: `asset_sac` is an AggregatingMergeTree read
/// through `max()` per house idiom, and `assets` is an unmerged RMT whose
/// duplicates would otherwise fan the map out. No `FINAL` anywhere (0356).
///
/// Ids are FORMATTED into the `{ids}` placeholder, not bound — the house
/// idiom for id-IN resolvers (0344/0345): they are `i64`s, injection-free by
/// type, and the driver does not bind sequences into `IN`. Every subquery on
/// a dimension is bounded by the same list — an unbounded
/// `SELECT DISTINCT … FROM soroban_contracts` would scan the whole contracts
/// dimension per request.
const RESOLVE_LEGS_SQL_TEMPLATE: &str = "\
    SELECT s.sac_contract_id AS leg, \
           max(s.asset_type)  AS family, \
           max(s.asset_code)  AS asset_code, \
           max(s.issuer_id)   AS issuer_id, \
           toNullable(toUInt32(7)) AS decimals, \
           CAST(NULL, 'Nullable(String)') AS symbol, \
           CAST(NULL, 'Nullable(String)') AS name, \
           nullIf(max(sc.contract_id), '') AS contract_strkey \
    FROM asset_sac s \
    LEFT JOIN (SELECT id, contract_id FROM soroban_contracts \
               WHERE id IN ({ids}) LIMIT 1 BY id) sc \
           ON sc.id = s.sac_contract_id \
    WHERE s.sac_contract_id IN ({ids}) \
    GROUP BY s.sac_contract_id \
    UNION ALL \
    SELECT a.contract_id AS leg, \
           3             AS family, \
           ''            AS asset_code, \
           0             AS issuer_id, \
           m.decimals    AS decimals, \
           nullIf(m.symbol, '') AS symbol, \
           nullIf(m.name, '')   AS name, \
           nullIf(sc.contract_id, '') AS contract_strkey \
    FROM (SELECT contract_id FROM assets \
          WHERE asset_type = 3 AND contract_id IN ({ids}) \
          GROUP BY contract_id) a \
    LEFT JOIN (SELECT id, contract_id FROM soroban_contracts \
               WHERE id IN ({ids}) LIMIT 1 BY id) sc \
           ON sc.id = a.contract_id \
    LEFT JOIN (SELECT contract_id, \
                      argMax(decimals, version) AS decimals, \
                      argMax(symbol, version)   AS symbol, \
                      argMax(name, version)     AS name \
               FROM soroban_contract_metadata GROUP BY contract_id) m \
           ON m.contract_id = sc.contract_id";

#[derive(Debug, Row, Deserialize)]
struct ResolvedLegChRow {
    leg: i64,
    family: i16,
    asset_code: String,
    issuer_id: i64,
    decimals: Option<u32>,
    symbol: Option<String>,
    name: Option<String>,
    contract_strkey: Option<String>,
}

/// Resolve pool-leg surrogates to asset identities, keyed by the surrogate.
pub(crate) async fn resolve_leg_assets(
    client: &clickhouse::Client,
    leg_ids: &[i64],
) -> Result<std::collections::HashMap<i64, ResolvedLeg>, clickhouse::error::Error> {
    // Canon shape from `common/ch.rs` (0344/0345): sort + dedup before
    // formatting — one union list's XLM leg repeats across nearly every pool.
    let mut leg_ids = leg_ids.to_vec();
    leg_ids.sort_unstable();
    leg_ids.dedup();
    if leg_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let ids = leg_ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let rows = client
        .query(&RESOLVE_LEGS_SQL_TEMPLATE.replace("{ids}", &ids))
        .fetch_all::<ResolvedLegChRow>()
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.leg,
                ResolvedLeg {
                    family: r.family,
                    asset_code: r.asset_code,
                    issuer_id: r.issuer_id,
                    contract_id: r.leg,
                    decimals: r.decimals,
                    symbol: r.symbol,
                    name: r.name,
                    contract_strkey: r.contract_strkey,
                },
            )
        })
        .collect())
}

/// Which per-pool feed serves a pool — the two id worlds of ADR 0058. A
/// soroban row's pair columns are storage defaults, and hashing them would
/// yield the NATIVE asset id, silently answering with XLM traffic that has
/// nothing to do with the pool — so the kind decides here, once, and the
/// impossible mixed states are unrepresentable.
pub enum PoolFeed {
    Classic { asset_a_id: i64, asset_b_id: i64 },
    Soroban { legs: Vec<i64> },
}

pub async fn fetch_pool_feed(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<PoolFeed>, clickhouse::error::Error> {
    let mut rows = client
        .query(
            "SELECT asset_a_type, asset_a_code, asset_a_issuer_id, \
                    asset_b_type, asset_b_code, asset_b_issuer_id, pool_kind, legs \
             FROM liquidity_pools WHERE pool_id = unhex(?) \
             ORDER BY last_updated_ledger DESC LIMIT 1",
        )
        .bind(pool_id_hex)
        .fetch_all::<PoolLegsChRow>()
        .await?;
    Ok(rows.pop().map(|r| {
        if r.pool_kind == 1 {
            PoolFeed::Soroban { legs: r.legs }
        } else {
            PoolFeed::Classic {
                asset_a_id: ids::pool_leg_asset_id(
                    r.asset_a_type,
                    &r.asset_a_code,
                    r.asset_a_issuer_id,
                ),
                asset_b_id: ids::pool_leg_asset_id(
                    r.asset_b_type,
                    &r.asset_b_code,
                    r.asset_b_issuer_id,
                ),
            }
        }
    }))
}

// ---------------------------------------------------------------------------
// Soroban-pool reads (task 0374, steps 16-17).
// ---------------------------------------------------------------------------

/// Router deployments whose OPERATOR identity is verified against vendor
/// documentation (task 0374 T1). Resolved at read time from
/// `liquidity_pools.deployment_id` → `soroban_contracts.contract_id` → this
/// list, so a new pool is labelled the moment it registers and a label fix
/// is a code change, not an UPDATE.
///
/// Deliberately NOT "every deployment of Aquarius's WASM": a second live
/// router shares the code byte-for-byte with all seven admin roles disjoint
/// (measured), so code identity does not establish operator identity. Its
/// pools stay indexed and unlabelled.
const ROUTER_PROTOCOL_LABELS: &[(&str, &str)] = &[(
    "CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK",
    "aquarius",
)];

/// `deployment_id` surrogates → protocol labels, via the contract dimension.
/// Ids absent from [`ROUTER_PROTOCOL_LABELS`] simply don't appear in the map
/// (unlabelled ≠ error).
pub(crate) async fn resolve_protocol_labels(
    client: &clickhouse::Client,
    deployment_ids: Vec<i64>,
) -> Result<HashMap<i64, &'static str>, clickhouse::error::Error> {
    let ids: Vec<i64> = deployment_ids.into_iter().filter(|&id| id != 0).collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let strkeys = crate::common::ch::resolve_contracts(client, ids).await?;
    Ok(strkeys
        .into_iter()
        .filter_map(|(id, strkey)| {
            ROUTER_PROTOCOL_LABELS
                .iter()
                .find(|(router, _)| *router == strkey)
                .map(|(_, label)| (id, *label))
        })
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct PoolReservesChRow {
    pool_id_hex: String,
    reserves: Vec<i128>,
}

/// Latest per-pool reserves from `pool_state_changes`, keyed by pool hex.
///
/// One deterministic row per (pool, ledger) — parse-time collapse in ledger
/// apply order, same grain as the classic snapshots — so `argMax` by
/// `ledger_sequence` alone picks the latest state and cannot tear. The
/// vector is VERBATIM plane/instance state: a concentrated pool's per-tick
/// tail may follow the leg reserves, so callers slice by the pool's leg
/// count, never by vector length.
///
/// Values return as decimal strings — an i128 reserve exceeds JSON number
/// precision, same rule as every amount on this API.
pub(crate) async fn fetch_latest_soroban_reserves(
    client: &clickhouse::Client,
    pool_ids_hex: &[&str],
) -> Result<HashMap<String, Vec<String>>, clickhouse::error::Error> {
    let mut hexes: Vec<&str> = pool_ids_hex
        .iter()
        .copied()
        .filter(|h| is_hex_pool_id(h))
        .collect();
    hexes.sort_unstable();
    hexes.dedup();
    if hexes.is_empty() {
        return Ok(HashMap::new());
    }
    // Validated-hex ids formatted into the IN list (the driver does not bind
    // sequences) — same inlining discipline as the list keyset.
    let in_list = hexes
        .iter()
        .map(|h| format!("unhex('{h}')"))
        .collect::<Vec<_>>()
        .join(",");
    let rows = client
        .query(&format!(
            "SELECT lower(hex(pool_id)) AS pool_id_hex, \
                    argMax(reserves, ledger_sequence) AS reserves \
             FROM pool_state_changes \
             WHERE pool_id IN ({in_list}) \
             GROUP BY pool_id"
        ))
        .fetch_all::<PoolReservesChRow>()
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.pool_id_hex,
                r.reserves.iter().map(i128::to_string).collect(),
            )
        })
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct PoolKindShareChRow {
    pool_kind: u8,
    share_token_id: i64,
}

/// Existence + kind + share-token gate for the participants endpoint:
/// `None` = pool unknown (404). `share_token_id = 0` = no share token known
/// (relation not yet derived, or a concentrated pool that never mints one).
///
/// The relation lives in the `pool_share_tokens` SIDE table (asset_sac
/// pattern — a partial row in the RMT registry would clobber the full
/// registration); versioned by sighting ledger so a share-token migration
/// converges on the newest, matching on-chain `share_id()`.
pub async fn fetch_pool_kind_share(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<(u8, i64)>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT lp.pool_kind AS pool_kind, \
                    toInt64(ifNull(st.tok, 0)) AS share_token_id \
             FROM liquidity_pools lp FINAL \
             LEFT JOIN ( \
                 SELECT pool_id, toNullable(argMax(share_token_id, derived_at_ledger)) AS tok \
                 FROM pool_share_tokens WHERE pool_id = unhex(?) GROUP BY pool_id \
             ) st ON st.pool_id = lp.pool_id \
             WHERE lp.pool_id = unhex(?) LIMIT 1",
        )
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .fetch_optional::<PoolKindShareChRow>()
        .await?;
    Ok(row.map(|r| (r.pool_kind, r.share_token_id)))
}

/// The soroban pool-flow signatures and their wire classification. The
/// router family emits `trade` / `deposit_liquidity` / `withdraw_liquidity`
/// from the POOL contract itself (measured; e.g. 17k trades on one hot
/// pool), with fully decoded topics/data already in `soroban_events`.
const SOROBAN_FLOW: [(&str, PoolEvent); 3] = [
    ("trade", PoolEvent::Trade),
    ("deposit_liquidity", PoolEvent::Deposit),
    ("withdraw_liquidity", PoolEvent::Withdrawal),
];

#[derive(Debug, Row, Deserialize)]
struct SorobanFlowEventChRow {
    ledger_sequence: i64,
    transaction_id: i64,
    event_index: i64,
    signature: String,
    topics_xdr: String,
    data_xdr: String,
}

/// One decoded address from house-dialect typed JSON.
fn json_address(v: &serde_json::Value) -> Option<String> {
    (v.get("type")?.as_str()? == "address")
        .then(|| v.get("value")?.as_str().map(str::to_string))
        .flatten()
}

/// One decoded i128 amount (as its decimal string) from typed JSON.
fn json_amount(v: &serde_json::Value) -> Option<String> {
    matches!(v.get("type")?.as_str()?, "i128" | "u128")
        .then(|| v.get("value")?.as_str().map(str::to_string))
        .flatten()
}

/// `GET /v1/liquidity-pools/{id}/activity`, SOROBAN branch (task 0374): the
/// pool's own flow events out of `soroban_events` — no new table, the data
/// was always there. The events table is keyed
/// `(contract_id, ledger, transaction_id, event_index)`, so the per-pool
/// read is a leading-PK seek and the keyset rides the table's own order
/// (like the classic feed, the intra-ledger component is a hash surrogate:
/// a STABLE pagination order, not a temporal one — the same accepted
/// compromise, called out here rather than hidden).
///
/// Amount semantics per signature (shapes pinned on prod):
/// - `trade`: topics `[sym, token_in, token_out, trader]`,
///   data `[amount_in, amount_out, fee]` — in enters the pool (+), out
///   leaves (−); the trader address is the actor.
/// - `deposit_liquidity` / `withdraw_liquidity`: topics `[sym, tokens…]`,
///   data `[amount per token…, shares]` — all legs +/− respectively; the
///   actor is the transaction's source.
///
/// A leg address maps to its index in Rust (`ids::contract_id(addr) ==
/// legs[i]`) — no lookups. An event whose shape does not decode is SKIPPED
/// WITH A LOG, never rendered half-right.
pub async fn fetch_soroban_pool_activity(
    client: &clickhouse::Client,
    pool_strkey: &str,
    legs: &[i64],
    limit: i64,
    cursor: Option<&PoolActivityCursor>,
    direction: Direction,
    event_filter: Option<PoolEvent>,
) -> Result<Vec<PoolActivityRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);
    let contract_surrogate = ids::contract_id(pool_strkey);
    let sig_list = SOROBAN_FLOW
        .iter()
        .filter(|(_, e)| event_filter.is_none() || event_filter == Some(*e))
        .map(|(s, _)| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(",");
    // Keyset bounds inlined (all i64 — injection-free by type; the same
    // tuple-comparison idiom as every other keyset here).
    let keyset = match cursor {
        Some(c) => format!(
            "AND (ledger_sequence, transaction_id, event_index) {op} ({ls},{tx},{ei})",
            ls = c.ledger_sequence,
            tx = c.transaction_id,
            ei = c.event_index,
        ),
        None => String::new(),
    };
    let sql = format!(
        "SELECT ledger_sequence, transaction_id, toInt64(event_index) AS event_index,                 ifNull(signature, '') AS signature, topics_xdr, data_xdr          FROM soroban_events         WHERE contract_id = {contract_surrogate}            AND signature IN ({sig_list})            {keyset}          ORDER BY ledger_sequence {order}, transaction_id {order}, event_index {order}          LIMIT 1 BY ledger_sequence, transaction_id, event_index          LIMIT {limit}"
    );
    let events = client
        .query(&sql)
        .fetch_all::<SorobanFlowEventChRow>()
        .await?;
    if events.is_empty() {
        return Ok(Vec::new());
    }

    let tx_keys: std::collections::BTreeSet<(i64, i64)> = events
        .iter()
        .map(|e| (e.ledger_sequence, e.transaction_id))
        .collect();
    let txs = fetch_activity_txs(client, &tx_keys).await?;
    let by_tx: HashMap<i64, &ActivityTxRow> = txs.iter().map(|t| (t.id, t)).collect();
    let sources = resolve_accounts(client, txs.iter().map(|t| t.source_id).collect()).await?;

    let mut out = Vec::with_capacity(events.len());
    for e in events {
        let Some(tx) = by_tx.get(&e.transaction_id) else {
            tracing::error!(
                ledger = e.ledger_sequence,
                tx = e.transaction_id,
                "soroban activity: event without its transaction row — dropped"
            );
            continue;
        };
        let (Ok(topics), Ok(data)) = (
            serde_json::from_str::<serde_json::Value>(&e.topics_xdr),
            serde_json::from_str::<serde_json::Value>(&e.data_xdr),
        ) else {
            tracing::error!(
                ledger = e.ledger_sequence,
                "soroban activity: undecodable event json"
            );
            continue;
        };
        let topic_addrs: Vec<String> = topics
            .as_array()
            .map(|a| a.iter().skip(1).filter_map(json_address).collect())
            .unwrap_or_default();
        let amounts: Vec<String> = data
            .get("value")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(json_amount).collect())
            .unwrap_or_default();
        let leg_index = |addr: &str| -> Option<u32> {
            let id = ids::contract_id(addr);
            legs.iter().position(|&l| l == id).map(|i| i as u32)
        };
        let (event, leg_amounts) = match e.signature.as_str() {
            // trade: topics [token_in, token_out, trader]; data [in, out, fee].
            "trade" => {
                let decoded = (|| {
                    let (tin, tout) = (topic_addrs.first()?, topic_addrs.get(1)?);
                    let (li, lo) = (leg_index(tin)?, leg_index(tout)?);
                    let (ain, aout) = (amounts.first()?, amounts.get(1)?);
                    Some(vec![(li, ain.clone()), (lo, format!("-{aout}"))])
                })();
                let Some(la) = decoded else {
                    tracing::error!(ledger = e.ledger_sequence, pool = %pool_strkey,
                        "trade event does not decode against this pool's legs — dropped");
                    continue;
                };
                (PoolEvent::Trade, la)
            }
            // deposit/withdraw: topics list the tokens, data pairs amounts
            // positionally (the trailing value is minted/burned shares).
            sig @ ("deposit_liquidity" | "withdraw_liquidity") => {
                let sign = if sig == "deposit_liquidity" { "" } else { "-" };
                let la = topic_addrs
                    .iter()
                    .zip(&amounts)
                    .map(|(addr, amt)| Some((leg_index(addr)?, format!("{sign}{amt}"))))
                    .collect::<Option<Vec<_>>>()
                    .filter(|la| la.len() == topic_addrs.len());
                let Some(la) = la else {
                    tracing::error!(ledger = e.ledger_sequence, pool = %pool_strkey,
                        "liquidity event legs/amounts do not line up — dropped");
                    continue;
                };
                let event = SOROBAN_FLOW
                    .iter()
                    .find(|(s, _)| *s == sig)
                    .map(|(_, e)| *e);
                (event.expect("signature came from SOROBAN_FLOW"), la)
            }
            other => {
                tracing::error!(signature = other, "unexpected signature passed the filter");
                continue;
            }
        };
        // The actor is the TRANSACTION's source, never the event's trader
        // topic: for routed trades that topic names the ROUTER contract
        // (measured), while "who did this" on this feed means the same thing
        // it means on the classic feed.
        let Some(source_account) = sources.get(&tx.source_id).cloned() else {
            tracing::error!(tx = e.transaction_id, "unresolvable tx source — dropped");
            continue;
        };
        out.push(PoolActivityRow {
            transaction_hash: tx.hash.clone(),
            ledger_sequence: e.ledger_sequence,
            transaction_id: e.transaction_id,
            application_order: None,
            event: Some(event),
            amount_a: None,
            amount_b: None,
            source_account,
            pools_crossed: None,
            leg_amounts: Some(leg_amounts),
            created_at: millis_to_utc(tx.created_at_ms),
            event_index: e.event_index,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Soroban pool chart (task 0374): TVL/volume/fees for AMM-contract pools.
// Sources: `pool_state_changes` (reserves), the pool's own `trade` events
// (volume), and the SAME prices series the classic chart joins — 96% of
// soroban legs are SACs whose classic identities are priced, and bespoke
// tokens price under `asset_kind = 'contract'` keyed by `contract_address`
// (the interop contract carried that kind from day one). Folded in Rust:
// per-pool row counts are bounded (reserve rows per bucket cap at the chart
// bucket cap; trades per pool per window measured in the tens of thousands),
// and Rust keeps the N-leg × bucket carry logic testable instead of a
// generated SQL wall.
// ---------------------------------------------------------------------------

/// One priceable soroban leg: its prices-series identity + display scale.
#[derive(Debug, Clone)]
pub struct SorobanChartLeg {
    pub price: ChartPriceId,
    pub decimals: u32,
}

/// How a leg keys into the `prices.*` views: SAC legs by their classic
/// identity (reusing [`PriceLeg`]), bespoke tokens under
/// `asset_kind = 'contract'` keyed by the token's C-strkey.
#[derive(Debug, Clone)]
pub enum ChartPriceId {
    Classic(PriceLeg),
    Contract(String),
}

/// Resolve a soroban pool's legs into `(surrogate, chart identity)` pairs —
/// paired so the trade→leg match and the price lookup cannot drift apart.
/// `None` per leg that cannot be identified or scaled — the chart nulls
/// what touches it.
pub async fn soroban_chart_legs(
    client: &clickhouse::Client,
    legs: &[i64],
) -> Result<Vec<(i64, Option<SorobanChartLeg>)>, clickhouse::error::Error> {
    let resolved = resolve_leg_assets(client, legs).await?;
    let issuer_ids: Vec<i64> = resolved
        .values()
        .filter(|l| l.issuer_id != 0)
        .map(|l| l.issuer_id)
        .collect();
    let issuers = resolve_accounts(client, issuer_ids).await?;
    Ok(legs
        .iter()
        .map(|&id| {
            let leg = (|| {
                let l = resolved.get(&id)?;
                let decimals = l.decimals?;
                let price = match l.family {
                    0 => ChartPriceId::Classic(price_leg(0, None, None)),
                    1 => ChartPriceId::Classic(price_leg(
                        1,
                        Some(&l.asset_code),
                        Some(issuers.get(&l.issuer_id)?),
                    )),
                    3 => ChartPriceId::Contract(l.contract_strkey.clone()?),
                    _ => return None,
                };
                Some(SorobanChartLeg { price, decimals })
            })();
            (id, leg)
        })
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct LegSeriesChRow {
    bucket_ts: i64,
    close: String,
}

/// One leg's USD close series at the given grain, `close_usd > 0` only (the
/// zero-volume Decimal::MIN sentinel is treated as absent) and the
/// in-progress price bucket excluded — the same two guards
/// [`fetch_last_closes`] documents as load-bearing.
async fn fetch_leg_close_series(
    client: &clickhouse::Client,
    leg: &SorobanChartLeg,
    interval: &str,
    from_ts: i64,
    to_ts: i64,
) -> Result<Vec<(i64, f64)>, clickhouse::error::Error> {
    let (series_view, price_bucket_fn) = chart_price_series(interval);
    let (identity, binds): (&str, Vec<&str>) = match &leg.price {
        ChartPriceId::Contract(addr) => (
            "asset_kind = 'contract' AND contract_address = ?",
            vec![addr],
        ),
        ChartPriceId::Classic(p) => (
            "asset_kind = ? AND asset_code = ? AND issuer_address = ?",
            vec![p.kind, &p.code, &p.issuer],
        ),
    };
    let sql = format!(
        "SELECT toInt64(toUnixTimestamp(bucket)) AS bucket_ts, \
                toString(argMax(close_usd, bucket)) AS close \
         FROM {series_view} \
         WHERE ({identity}) AND close_usd > 0 \
           AND bucket >= toDateTime({from_ts}) AND bucket < toDateTime({to_ts}) \
           AND bucket < {price_bucket_fn}(now()) \
         GROUP BY bucket ORDER BY bucket"
    );
    let mut q = client.query(&sql);
    for b in binds {
        q = q.bind(b);
    }
    let rows = q.fetch_all::<LegSeriesChRow>().await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| Some((r.bucket_ts, parse_f64(&r.close)?)))
        .collect())
}

/// The prices series view + its grain function for a chart interval — the
/// classic chart's mapping, shared so the soroban branch cannot name a view
/// that does not exist (there is no `_1d` view; daily is the base series).
fn chart_price_series(interval: &str) -> (&'static str, &'static str) {
    match interval {
        "1h" => ("prices.price_usd_series_1h", "toStartOfHour"),
        _ => ("prices.price_usd_series", "toStartOfDay"),
    }
}

#[derive(Debug, Row, Deserialize)]
struct ReserveBucketChRow {
    bucket: i64,
    reserves: Vec<i128>,
    samples: i64,
}

#[derive(Debug, Row, Deserialize)]
struct TradeBucketChRow {
    bucket: i64,
    token_in: String,
    amount_sum: f64,
    bad: i64,
}

/// `GET /v1/liquidity-pools/{id}/chart`, SOROBAN branch. Same wire contract
/// as the classic chart: SPARSE buckets (only buckets where something
/// happened), calendar-aligned by the same `toStartOfHour`/`toStartOfDay`/
/// `toMonday` grain, bucket-close pricing for volume, bounded price
/// carry-forward, `null` over a partial value — TVL requires EVERY leg
/// priced and scaled, or the point is null. `samples_in_bucket` counts
/// reserve-change points (the soroban analogue of snapshot rows).
///
/// ClickHouse pre-aggregates both inputs to ≤ one row per (bucket[, token]);
/// Rust only does what SQL cannot express here: the per-leg price carry and
/// the all-legs-or-null TVL rule.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_soroban_pool_chart(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    pool_strkey: &str,
    legs: &[(i64, Option<SorobanChartLeg>)],
    fee_bps: i32,
    interval: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<ChartDataPoint>, clickhouse::error::Error> {
    let bucket_fn = match interval {
        "1h" => "toStartOfHour",
        "1d" => "toStartOfDay",
        _ => "toMonday",
    };
    let (from_ts, to_ts) = (from.timestamp(), to.timestamp());
    let emitter = ids::contract_id(pool_strkey);

    // Reserve state per bucket (argMax = last write wins), plus one SEED row
    // (bucket 0) carrying the last state before `from` so the leading buckets
    // still price. Ledger→time via a bounded ledgers seek; the pre-window
    // history is aggregated in CH, never shipped.
    let reserve_sql = format!(
        "WITH pts AS ( \
             SELECT ledger_sequence, argMax(reserves, ledger_sequence) AS reserves \
             FROM pool_state_changes WHERE pool_id = unhex('{pool_id_hex}') \
             GROUP BY ledger_sequence \
         ), tpts AS ( \
             SELECT toInt64(toUnixTimestamp(l.closed_at)) AS ts, pts.reserves AS reserves \
             FROM pts \
             INNER JOIN (SELECT sequence, any(closed_at) AS closed_at FROM ledgers \
                         WHERE sequence IN (SELECT ledger_sequence FROM pts) GROUP BY sequence) l \
                    ON l.sequence = pts.ledger_sequence \
             WHERE toUnixTimestamp(l.closed_at) < {to_ts} \
         ) \
         SELECT toInt64(toUnixTimestamp(toDateTime64({bucket_fn}(toDateTime(ts)), 0, 'UTC'))) AS bucket, \
                argMax(reserves, ts) AS reserves, toInt64(count()) AS samples \
         FROM tpts WHERE ts >= {from_ts} GROUP BY bucket \
         UNION ALL \
         SELECT toInt64(0) AS bucket, argMax(reserves, ts) AS reserves, toInt64(0) AS samples \
         FROM tpts WHERE ts < {from_ts} HAVING count() > 0 \
         ORDER BY bucket"
    );

    // Trade volume inputs per (bucket, in-token), summed in CH; `bad` counts
    // amounts that failed to parse so the poison rule survives aggregation.
    let trades_sql = format!(
        "WITH ev AS ( \
             SELECT ledger_sequence, \
                    JSONExtractString(topics_xdr, 2, 'value') AS token_in, \
                    toFloat64OrNull(JSONExtractString(data_xdr, 'value', 1, 'value')) AS amount \
             FROM soroban_events \
             WHERE contract_id = {emitter} AND signature = 'trade' \
             LIMIT 1 BY ledger_sequence, transaction_id, event_index \
         ) \
         SELECT toInt64(toUnixTimestamp(toDateTime64({bucket_fn}(l.closed_at), 0, 'UTC'))) AS bucket, \
                ev.token_in AS token_in, \
                sum(coalesce(ev.amount, 0)) AS amount_sum, \
                toInt64(countIf(ev.amount IS NULL)) AS bad \
         FROM ev \
         INNER JOIN (SELECT sequence, any(closed_at) AS closed_at FROM ledgers \
                     WHERE sequence IN (SELECT ledger_sequence FROM ev) GROUP BY sequence) l \
                ON l.sequence = ev.ledger_sequence \
         WHERE toUnixTimestamp(l.closed_at) >= {from_ts} \
           AND toUnixTimestamp(l.closed_at) < {to_ts} \
         GROUP BY bucket, token_in"
    );

    // The three inputs are independent — one wall-clock round trip.
    let (reserve_rows, trade_rows, series) = futures::try_join!(
        client.query(&reserve_sql).fetch_all::<ReserveBucketChRow>(),
        client.query(&trades_sql).fetch_all::<TradeBucketChRow>(),
        futures::future::try_join_all(legs.iter().map(|(_, leg)| async move {
            match leg {
                Some(l) => fetch_leg_close_series(
                    client,
                    l,
                    interval,
                    from_ts - MAX_PRICE_CARRY_SECONDS,
                    to_ts,
                )
                .await
                .map(Some),
                None => Ok(None),
            }
        })),
    )?;

    // Fold sparse buckets: union of reserve + trade buckets, walked in order
    // with three carries — reserves (unbounded: state is state), one price
    // cursor per leg (bounded by MAX_PRICE_CARRY_SECONDS), and the seed.
    #[derive(Default)]
    struct Bucket {
        reserves: Option<Vec<i128>>,
        samples: i64,
        trades: Vec<(String, f64, i64)>,
    }
    let mut buckets: std::collections::BTreeMap<i64, Bucket> = std::collections::BTreeMap::new();
    let mut last_reserves: Option<Vec<i128>> = None;
    for r in reserve_rows {
        if r.bucket == 0 {
            last_reserves = Some(r.reserves); // the seed
        } else {
            let b = buckets.entry(r.bucket).or_default();
            b.reserves = Some(r.reserves);
            b.samples = r.samples;
        }
    }
    for t in trade_rows {
        buckets
            .entry(t.bucket)
            .or_default()
            .trades
            .push((t.token_in, t.amount_sum, t.bad));
    }

    let bucket_secs: i64 = match interval {
        "1h" => 3_600,
        "1d" => 86_400,
        _ => 604_800,
    };
    // Per-leg advancing cursors into the (sorted) close series — O(series)
    // total across all buckets.
    let mut cursors = vec![(0usize, None::<(i64, f64)>); legs.len()];
    let mut points = Vec::with_capacity(buckets.len());
    for (bucket_start, b) in buckets {
        let bucket_end = bucket_start + bucket_secs;
        if let Some(r) = b.reserves {
            last_reserves = Some(r);
        }
        // USD value of one raw unit of each leg in this bucket, or None.
        let unit_usd: Vec<Option<f64>> = legs
            .iter()
            .enumerate()
            .map(|(i, (_, leg))| {
                let s = series[i].as_ref()?;
                let (pos, last) = &mut cursors[i];
                while *pos < s.len() && s[*pos].0 < bucket_end {
                    *last = Some(s[*pos]);
                    *pos += 1;
                }
                let (ts, close) = (*last)?;
                (ts >= bucket_end - MAX_PRICE_CARRY_SECONDS)
                    .then_some(close * 10f64.powi(-(leg.as_ref()?.decimals as i32)))
            })
            .collect();
        let tvl = last_reserves.as_ref().and_then(|res| {
            legs.iter()
                .enumerate()
                .map(|(i, _)| Some(*res.get(i)? as f64 * unit_usd[i]?))
                .sum::<Option<f64>>()
        });
        // A trade we cannot price poisons the bucket to null — never a
        // partial sum (classic rule). Token→leg by SURROGATE against the
        // registry's own legs, never via the contracts dimension.
        let vol = (!b.trades.is_empty())
            .then(|| {
                b.trades
                    .iter()
                    .map(|(token, sum, bad)| {
                        if *bad > 0 {
                            return None;
                        }
                        let id = ids::contract_id(token);
                        let i = legs.iter().position(|(l, _)| *l == id)?;
                        Some(sum * unit_usd[i]?)
                    })
                    .sum::<Option<f64>>()
            })
            .flatten();
        points.push(ChartDataPoint {
            bucket: DateTime::<Utc>::from_timestamp(bucket_start, 0).unwrap_or(from),
            tvl: tvl.map(usd_str),
            volume: vol.map(usd_str),
            fee_revenue: vol.map(|v| usd_str(fee_revenue_usd(v, fee_bps))),
            samples_in_bucket: b.samples,
        });
    }
    Ok(points)
}

#[derive(Debug, Row, Deserialize)]
struct SorobanParticipantChRow {
    holder_id: i64,
    shares: String,
    share_percentage: Option<String>,
    last_updated_ledger: i64,
}

/// Insert the decimal point into a raw non-negative integer amount string.
/// Exact string surgery — no float, no i128 parse (the value already IS the
/// canonical decimal digits). Trailing fractional zeros are trimmed, matching
/// how CH renders Decimal columns via `toString`.
fn scale_raw_amount(raw: &str, decimals: u32) -> String {
    if decimals == 0 {
        return raw.to_string();
    }
    let d = decimals as usize;
    let padded = if raw.len() <= d {
        format!("{raw:0>width$}", width = d + 1)
    } else {
        raw.to_string()
    };
    let (int_part, frac_part) = padded.split_at(padded.len() - d);
    let frac_part = frac_part.trim_end_matches('0');
    if frac_part.is_empty() {
        int_part.to_string()
    } else {
        format!("{int_part}.{frac_part}")
    }
}

#[derive(Debug, Row, Deserialize)]
struct DecimalsChRow {
    decimals: Option<u32>,
}

/// The share token's on-chain metadata `decimals` (newest version), through
/// the surrogate→strkey hop. `None` = token never published metadata — the
/// caller must then surface amounts as UNSCALED-therefore-absent, never
/// guess a scale.
async fn fetch_token_decimals(
    client: &clickhouse::Client,
    token_id: i64,
) -> Result<Option<u32>, clickhouse::error::Error> {
    let row = client
        .query(
            "SELECT argMax(decimals, version) AS decimals \
             FROM soroban_contract_metadata \
             WHERE contract_id = ( \
                 SELECT contract_id FROM soroban_contracts WHERE id = ? LIMIT 1 BY id \
             )",
        )
        .bind(token_id)
        .fetch_optional::<DecimalsChRow>()
        .await?;
    Ok(row.and_then(|r| r.decimals))
}

/// Participants of a SOROBAN pool = current holders of its share token,
/// from `balances` (task 0374 step 16). Ordered `(amount DESC, holder DESC)`
/// to mirror the classic participants sort.
///
/// `balances` is keyed `(holder_id, asset_id)`, so the asset-direction read
/// is a full scan — measured 121.5M rows / 974 MB / 71 ms, acceptable behind
/// the endpoint's short cache.
// ponytail: full-scan by asset_id; hand ClickHouse a skip index
// (`ALTER TABLE balances ADD INDEX ... asset_id TYPE bloom_filter`) in the
// deploy window if this endpoint gets hot.
///
/// `argMax(amount, last_updated_ledger)` dedups the unmerged RMT (house
/// rule: every read dedups). Percentage = share of the sum of POSITIVE
/// balances — the live circulating supply; total-supply state is not
/// separately indexed and burnt/zeroed rows must not dilute it.
pub async fn fetch_soroban_participants(
    client: &clickhouse::Client,
    share_token_id: i64,
    cursor: Option<&SharesCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<ParticipantRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);
    // Same inline-not-bind discipline as the classic keyset (clickhouse-rs
    // None-into-tuple defect); a non-numeric cursor degrades to page one.
    // Share-token balances are integers — reject a fractional cursor value.
    let keyset = match cursor {
        Some(c) if is_decimal_str(&c.shares) && !c.shares.contains('.') => format!(
            "AND ((amt {op} toInt128('{s}')) \
                  OR (amt = toInt128('{s}') AND holder_id {op} {a}))",
            op = op,
            s = c.shares,
            a = c.account_id,
        ),
        _ => String::new(),
    };
    let sql = format!(
        "WITH pos AS ( \
             SELECT holder_id, argMax(amount, last_updated_ledger) AS amt, \
                    max(last_updated_ledger) AS lul \
             FROM balances WHERE asset_id = ? \
             GROUP BY holder_id HAVING amt > 0 \
         ), \
         total AS (SELECT sum(amt) AS t FROM pos) \
         SELECT holder_id, \
                toString(amt) AS shares, \
                toNullable(toString(round(toFloat64(amt) * 100 / (SELECT toFloat64(t) FROM total), 4))) \
                    AS share_percentage, \
                lul AS last_updated_ledger \
         FROM pos \
         WHERE 1 = 1 {keyset} \
         ORDER BY amt {order}, holder_id {order} \
         LIMIT ?",
        keyset = keyset,
        order = order,
    );
    let rows = client
        .query(&sql)
        .bind(share_token_id)
        .bind(limit)
        .fetch_all::<SorobanParticipantChRow>()
        .await?;

    // A holder is an account (G...) or a contract (C...) — Aquarius LPs
    // routinely stake into a locker contract, so contract holders are the
    // NORMAL case, not an edge. Resolve through both dimensions; a surrogate
    // resolving through neither is dropped with the same error contract as
    // the classic path (pagination may terminate early — operator signal).
    let ids: Vec<i64> = rows.iter().map(|r| r.holder_id).collect();
    let (accounts, contracts, decimals) = tokio::join!(
        crate::common::ch::resolve_accounts(client, ids.clone()),
        crate::common::ch::resolve_contracts(client, ids),
        fetch_token_decimals(client, share_token_id),
    );
    let (accounts, contracts, decimals) = (accounts?, contracts?, decimals?);
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let Some(strkey) = accounts
                .get(&r.holder_id)
                .or_else(|| contracts.get(&r.holder_id))
                .cloned()
            else {
                tracing::error!(
                    holder_id = r.holder_id,
                    share_token_id,
                    "share-token balance resolves to no account/contract row: \
                     participant dropped, pagination may terminate early"
                );
                return None;
            };
            Some(ParticipantRow {
                account: strkey,
                account_id_surrogate: r.holder_id,
                // Display scaled by the share token's metadata decimals;
                // decimals unknown → absent, never a raw integer posing as
                // a scaled amount. The percentage is scale-free and stays.
                shares: decimals.map(|d| scale_raw_amount(&r.shares, d)),
                cursor_shares: r.shares,
                share_percentage: r.share_percentage,
                first_deposit_ledger: None,
                last_updated_ledger: r.last_updated_ledger,
            })
        })
        .collect())
}

#[derive(Debug, Row, Deserialize)]
struct ParticipantChRow {
    account_id_surrogate: i64,
    shares: String,
    share_percentage: Option<String>,
    first_deposit_ledger: i64,
    last_updated_ledger: i64,
}

/// `GET /v1/liquidity-pools/:id/participants` — active providers ordered by
/// `(shares DESC, account_id DESC)`. Mirrors the PG `fetch_participants`.
pub async fn fetch_participants(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    cursor: Option<&SharesCursor>,
    limit: i64,
    direction: Direction,
) -> Result<Vec<ParticipantRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Keyset expanded out of the natural `(shares, account_id) <op> (?, ?)`
    // tuple form on purpose: a Decimal128 inside a CH tuple comparison is the
    // documented "Decimal-tuple-compare" trap. The scalar `shares <op>
    // toDecimal128(...)` is proven safe. The bounds are inlined (not bound) for
    // the same reason accounts/contracts inline theirs — a `None` bound into a
    // keyset returns an empty page on clickhouse-rs 0.15. `shares` is validated
    // as a decimal string before inlining; a tampered cursor degrades to "no
    // keyset" (first page) rather than injecting.
    let keyset = match cursor {
        Some(c) if is_decimal_str(&c.shares) => format!(
            "AND ((lpp.shares {op} toDecimal128('{s}', 7)) \
                  OR (lpp.shares = toDecimal128('{s}', 7) AND lpp.account_id {op} {a}))",
            op = op,
            s = c.shares,
            a = c.account_id,
        ),
        _ => String::new(),
    };

    // `snap.ts` = total_shares of the latest snapshot within the freshness
    // window (NULL → stale pool → share_percentage NULL). The scalar subquery
    // is scoped to the literal pool (not correlated). CROSS JOIN broadcasts the
    // single value to every position row (PG `LEFT JOIN latest_snap ON TRUE`).
    let sql = format!(
        "SELECT \
            lpp.account_id                       AS account_id_surrogate, \
            toString(lpp.shares)                 AS shares, \
            if(snap.ts IS NULL OR snap.ts = toDecimal128(0, 7), NULL, \
               toString(lpp.shares * 100 / snap.ts)) AS share_percentage, \
            lpp.first_deposit_ledger             AS first_deposit_ledger, \
            lpp.last_updated_ledger              AS last_updated_ledger \
         FROM lp_positions lpp FINAL \
         CROSS JOIN ( \
            SELECT (SELECT total_shares FROM liquidity_pool_snapshots \
                     WHERE pool_id = unhex(?) \
                       AND ledger_sequence >= (SELECT max(sequence) FROM ledgers) - {fresh} \
                     ORDER BY ledger_sequence DESC LIMIT 1) AS ts \
         ) snap \
         WHERE lpp.pool_id = unhex(?) AND lpp.shares > 0 \
           {keyset} \
         ORDER BY lpp.shares {order}, lpp.account_id {order} \
         LIMIT ?",
        fresh = FRESHNESS_WINDOW_LEDGERS,
        keyset = keyset,
        order = order,
    );

    let rows = client
        .query(&sql)
        .bind(pool_id_hex)
        .bind(pool_id_hex)
        .bind(limit)
        .fetch_all::<ParticipantChRow>()
        .await?;

    // Resolve the provider StrKey by surrogate id (bloom seek) instead of a
    // whole-`accounts` `JOIN accounts acc FINAL` (task 0354). INNER-JOIN drop
    // semantics preserved via filter_map.
    //
    // "A position always has its account" holds today but is NOT guaranteed by
    // construction — it is maintained by operators. Measured on prod
    // (2026-08-04): all 6010 distinct `shares > 0` participants resolved, 0
    // missing. No non-test Rust path deletes an `accounts` row and prod's
    // retained mutation history has none, but two operator-driven paths can:
    //
    //   * `docs/runbooks/0225_backfill_crash_recovery.md` rolls back `accounts`
    //     on `last_seen_ledger` while rolling back `lp_positions` on
    //     `last_updated_ledger` — DIFFERENT watermarks, so an account touched
    //     inside the crashed range can lose its row while an older position
    //     survives. That is exactly the dangling surrogate below;
    //   * `repair_tier1::rebuild_accounts` replaces the whole table via
    //     `EXCHANGE TABLES` (`ch_staging::finalize`), where rows can disappear
    //     with no DELETE at all.
    //
    // So the log stays, and the failure mode is not proportional to the cause:
    // the drop happens BEFORE `finalize_page` reads the `limit + 1` sentinel,
    // so losing the sentinel row reports "no next page" and hides the REST of
    // the list, not one participant. Only 82 of the 26_489 pools with a live
    // participant hold more than one page, but the largest holds 684. `error!`
    // (not `debug!`) because the Lambda runs at `RUST_LOG=info` (0377 F3).
    let accounts = resolve_accounts(
        client,
        rows.iter().map(|r| r.account_id_surrogate).collect(),
    )
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let Some(account) = accounts.get(&r.account_id_surrogate).cloned() else {
                tracing::error!(
                    account_id_surrogate = r.account_id_surrogate,
                    pool_id = pool_id_hex,
                    "lp_positions row resolves to no accounts row: participant \
                     dropped, so participant_count disagrees with the list and \
                     pagination may terminate early"
                );
                return None;
            };
            Some(ParticipantRow {
                account,
                account_id_surrogate: r.account_id_surrogate,
                shares: Some(r.shares.clone()),
                cursor_shares: r.shares,
                share_percentage: r.share_percentage,
                first_deposit_ledger: Some(r.first_deposit_ledger),
                last_updated_ledger: r.last_updated_ledger,
            })
        })
        .collect())
}

/// One raw leg from `lp_operation_amounts` — the table's own grain, read in
/// sort-key order and paired in Rust. No `GROUP BY`: see
/// [`fetch_pool_activity`] for the measurement that removed it.
#[derive(Debug, Row, Deserialize)]
struct PoolLegChRow {
    ls: i64,
    tid: i64,
    ao: i16,
    asset_id: i64,
    amount: i64,
}

/// Transaction-level enrichment for the activity page's DISTINCT tx keys.
#[derive(Debug, Row, Deserialize)]
struct ActivityTxRow {
    id: i64,
    hash: String,
    source_id: i64,
    created_at_ms: i64,
}

/// Enrich a page's DISTINCT transactions (hash, source, close time) — the
/// bounded `(ledger_sequence, id)` PK seek with the partition prune, shared
/// by both activity feeds so the SQL cannot drift between them. Keys inlined
/// (all i64 — injection-free by type), same shape as
/// `common::ch::fetch_tx_list_aggregates`.
async fn fetch_activity_txs(
    client: &clickhouse::Client,
    tx_keys: &std::collections::BTreeSet<(i64, i64)>,
) -> Result<Vec<ActivityTxRow>, clickhouse::error::Error> {
    let in_tuples = tx_keys
        .iter()
        .map(|(ls, tid)| format!("({ls},{tid})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = tx_keys
        .iter()
        .map(|(ls, _)| ls / 500_000)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let detail_sql = format!(
        "SELECT \
            t.id                                 AS id, \
            lower(hex(t.hash))                   AS hash, \
            t.source_id                          AS source_id, \
            toUnixTimestamp64Milli(l.closed_at)  AS created_at_ms \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions}) \
         LIMIT 1 BY t.id"
    );
    client.query(&detail_sql).fetch_all::<ActivityTxRow>().await
}

/// The operation's own source account, `None` when it declares none (the XDR
/// default: the transaction's source).
#[derive(Debug, Row, Deserialize)]
struct OpSourceChRow {
    ls: i64,
    tid: i64,
    ao: i16,
    source_id: Option<i64>,
    /// `length(pool_ids)` — how many pools the whole operation crossed.
    pools_crossed: u64,
}

/// One operation's two legs, paired out of the key-ordered leg stream.
struct PairedOp {
    ls: i64,
    tid: i64,
    ao: i16,
    amount_a: Option<i64>,
    amount_b: Option<i64>,
}

impl PairedOp {
    /// `None` unless BOTH legs landed — the read stays total rather than
    /// classifying a half-row. `anyIf`-style defaulting would have made a
    /// missing leg read as `0` and turn a half-row into a "trade".
    fn event(&self) -> Option<PoolEvent> {
        match (self.amount_a, self.amount_b) {
            (Some(a), Some(b)) => Some(PoolEvent::from_signs(a, b)),
            _ => None,
        }
    }
}

/// Fold the key-ordered leg stream into operations.
///
/// The two legs of one operation are ADJACENT by construction: `asset_id` is
/// the last component of the sort key, so rows sharing
/// `(ledger_sequence, transaction_id, application_order)` are neighbours. That
/// is the whole reason this can be a fold instead of an aggregation.
///
/// `truncated` means the read hit its row cap, so the final group may be
/// missing a leg that simply did not fit — it is dropped and re-read from the
/// previous complete key on the next window.
fn pair_legs(rows: Vec<PoolLegChRow>, legs: (i64, i64), truncated: bool) -> Vec<PairedOp> {
    let (asset_a, asset_b) = legs;
    let mut out: Vec<PairedOp> = Vec::new();
    for r in rows {
        match out.last_mut() {
            Some(last) if last.ls == r.ls && last.tid == r.tid && last.ao == r.ao => {
                if r.asset_id == asset_a {
                    last.amount_a = Some(r.amount);
                } else if r.asset_id == asset_b {
                    last.amount_b = Some(r.amount);
                }
            }
            _ => out.push(PairedOp {
                ls: r.ls,
                tid: r.tid,
                ao: r.ao,
                amount_a: (r.asset_id == asset_a).then_some(r.amount),
                amount_b: (r.asset_id == asset_b).then_some(r.amount),
            }),
        }
    }
    if truncated {
        out.pop();
    }
    out
}

/// `GET /v1/liquidity-pools/:id/activity` — one row per (operation, pool),
/// task 0491.
///
/// **The driver table is the design.** `operation_pools` is keyed
/// `(pool_id, ledger_sequence, transaction_id)` with no `application_order`,
/// so it cannot page per operation. `lp_operation_amounts` is keyed
/// `(pool_id, ledger_sequence, transaction_id, application_order, asset_id)`
/// — the page's exact grain, reached by one PK-prefix seek.
///
/// **No `GROUP BY`, and that is measured, not stylistic.** The first cut of
/// this function pivoted the legs with `countIf`/`anyIf` and grouped by the
/// key triple. On prod's busiest pool (1.68M leg rows) that read **2.60M rows
/// / 109 ms / 182 MiB** to return 21 operations — a `GROUP BY` has to consume
/// the pool's whole slice before `ORDER BY … LIMIT` can pick the newest 21.
/// `optimize_aggregation_in_order` did not help (same rows, 253 ms). Reading
/// the same rows in sort-key order and pairing them here is **115k rows /
/// 9 ms / ~11 MiB** (median of 3), against **159k / 11 ms** for the
/// per-transaction endpoint this replaces: 22× off the first cut, and
/// slightly under the shape it supersedes.
///
/// Take the medians, not single runs — a cold run of EITHER shape reads
/// 0.7–1.0M rows, so one measurement each can invert the comparison. Measured
/// 2026-08-18 on prod; `log_comment` `lore0491-*` / `rep-*` in
/// `system.query_log`.
///
/// For the record: `FINAL` was never the cost — it added 22% (2.60M → 3.17M),
/// not an order of magnitude. It stays off because the producer is
/// deterministic (schema header's single-writer argument), so an unmerged
/// duplicate is byte-identical to its twin.
///
/// **Known consequence: an operation with no amount rows is not listed.** The
/// indexer writes `operation_pools` for an op that *declares* a pool whether
/// or not the transaction succeeded; amounts are written only for value that
/// actually moved. A failed explicit LP op therefore had a row under
/// `/transactions` and has none here — the page answers "what moved through
/// this pool", and a failed op moved nothing.
pub async fn fetch_pool_activity(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    asset_ids: (i64, i64),
    limit: i64,
    cursor: Option<&PoolActivityCursor>,
    direction: Direction,
    event: Option<PoolEvent>,
) -> Result<Vec<PoolActivityRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Where the next window resumes. `<` on the whole triple skips the last
    // kept operation outright — both its legs share that triple, so there is
    // no half-operation to step over.
    let mut after: Option<(i64, i64, i16)> =
        cursor.map(|c| (c.ledger_sequence, c.transaction_id, c.application_order));

    // Two legs per operation, plus slack so the cap rarely lands mid-op.
    let mut window = (limit * 2 + 2).max(64);
    let mut ops: Vec<PairedOp> = Vec::new();

    // One pass when unfiltered (the common case). With `filter[event]` the
    // matching rate is unknown up front — the event is only knowable once both
    // legs are in hand — so the window doubles until the page fills or the
    // pool runs out. Geometric growth keeps this O(log) round trips and never
    // reads more than ~2× the span it had to cover; a linear re-poll would be
    // the slow version of the same idea.
    loop {
        let keyset = match after {
            Some((ls, tid, ao)) => format!(
                " AND (ledger_sequence, transaction_id, application_order) {op} ({ls}, {tid}, {ao})"
            ),
            None => String::new(),
        };
        let sql = format!(
            "SELECT \
                ledger_sequence   AS ls, \
                transaction_id    AS tid, \
                application_order AS ao, \
                asset_id          AS asset_id, \
                amount            AS amount \
             FROM lp_operation_amounts \
             WHERE pool_id = toFixedString(unhex(?), 32) \
               AND ledger_sequence <= (SELECT max(sequence) FROM ledgers) {keyset} \
             ORDER BY ls {order}, tid {order}, ao {order} \
             LIMIT {window}"
        );
        let rows = client
            .query(&sql)
            .bind(pool_id_hex)
            .fetch_all::<PoolLegChRow>()
            .await?;

        let exhausted = (rows.len() as i64) < window;
        let batch = pair_legs(rows, asset_ids, !exhausted);
        if let Some(last) = batch.last() {
            after = Some((last.ls, last.tid, last.ao));
        }

        match event {
            Some(want) => ops.extend(batch.into_iter().filter(|o| o.event() == Some(want))),
            None => ops.extend(batch),
        }

        if exhausted || (ops.len() as i64) >= limit {
            break;
        }
        window *= 2;
    }
    ops.truncate(limit as usize);
    if ops.is_empty() {
        return Ok(Vec::new());
    }

    // Enrich the page's DISTINCT transactions — several operations of one
    // transaction share a row here, so this set is smaller than the page.
    let tx_keys: std::collections::BTreeSet<(i64, i64)> =
        ops.iter().map(|o| (o.ls, o.tid)).collect();
    let txs = fetch_activity_txs(client, &tx_keys).await?;
    let by_tx: HashMap<i64, &ActivityTxRow> = txs.iter().map(|t| (t.id, t)).collect();

    // The OPERATION's own source account. A Stellar operation may declare one,
    // and then it — not the transaction's source — is who performed this
    // operation; `operations_appearances.source_id` is NULL when it does not,
    // which per the XDR means "same as the transaction's". Showing the
    // transaction's source on a per-operation row names the wrong account
    // whenever they differ (measured on prod: 41% of ops in a recent ledger
    // window declare their own, and stellar.expert shows that one).
    //
    // `(ledger_sequence, transaction_id, application_order)` IS this table's
    // sort key, so the page's bounded IN-list is a PK seek with the same
    // partition prune. `max()` rather than `LIMIT 1 BY`: the table holds one
    // row per APPEARANCE, so an operation has several, and aggregation skips
    // the NULLs instead of picking one arbitrarily.
    //
    // `pools_crossed` rides the same seek for free: `pool_ids` is the op's
    // sorted+deduped crossing list, written identically on every appearance
    // row (stage.rs fans the one list out), so `max(length(...))` is just
    // "the length". It is what lets a row say "this trade was one hop of an
    // N-pool route" without carrying the route itself — the route lives on
    // the op's detail page, which the row already links to.
    let in_tuples = tx_keys
        .iter()
        .map(|(ls, tid)| format!("({ls},{tid})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = tx_keys
        .iter()
        .map(|(ls, _)| (ls / 500_000).to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",");
    let op_sources_sql = format!(
        "SELECT \
            ledger_sequence   AS ls, \
            transaction_id    AS tid, \
            application_order AS ao, \
            max(source_id)    AS source_id, \
            max(length(pool_ids)) AS pools_crossed \
         FROM operations_appearances \
         WHERE (ledger_sequence, transaction_id) IN ({in_tuples}) \
           AND intDiv(ledger_sequence, 500000) IN ({partitions}) \
         GROUP BY ls, tid, ao"
    );
    let op_sources = client
        .query(&op_sources_sql)
        .fetch_all::<OpSourceChRow>()
        .await?;
    let by_op: HashMap<(i64, i64, i16), (Option<i64>, u64)> = op_sources
        .iter()
        .map(|r| ((r.ls, r.tid, r.ao), (r.source_id, r.pools_crossed)))
        .collect();

    // Source StrKeys by surrogate id (bloom seek) rather than a whole-
    // `accounts` INNER JOIN — task 0354. One resolve for both kinds of source.
    let account_ids = txs
        .iter()
        .map(|t| t.source_id)
        .chain(by_op.values().filter_map(|(src, _)| *src))
        .collect();
    let accounts = resolve_accounts(client, account_ids).await?;

    // A page row whose transaction did not resolve is DROPPED, not rendered
    // half-blank: it would have no hash to link and no timestamp to sort by.
    // Unreachable unless the tx tables lag the amounts table, and the
    // `max(sequence)` fence above already keeps the seek behind the commit
    // marker.
    Ok(ops
        .into_iter()
        .filter_map(|o| {
            let tx = by_tx.get(&o.tid)?;
            // The operation's own source, falling back to the transaction's —
            // which is what the XDR's absent `sourceAccount` means.
            let (op_source, pools_crossed) = by_op
                .get(&(o.ls, o.tid, o.ao))
                .copied()
                .map_or((None, None), |(src, n)| (src, Some(n as i64)));
            let source_id = op_source.unwrap_or(tx.source_id);
            let source_account = accounts.get(&source_id)?.clone();
            let event = o.event();
            Some(PoolActivityRow {
                leg_amounts: None,
                event_index: 0,
                transaction_hash: tx.hash.clone(),
                ledger_sequence: o.ls,
                transaction_id: o.tid,
                application_order: Some(o.ao),
                event,
                amount_a: event.and(o.amount_a).map(|v| v.to_string()),
                amount_b: event.and(o.amount_b).map(|v| v.to_string()),
                source_account,
                pools_crossed,
                created_at: millis_to_utc(tx.created_at_ms),
            })
        })
        .collect())
}

/// Money arrives as raw `Nullable(Float64)` and is formatted by [`usd_str`]
/// on the Rust side; `fee_revenue` is derived from `volume` here rather
/// than in SQL, so chart and detail run identical arithmetic.
#[derive(Debug, Row, Deserialize)]
struct ChartChRow {
    bucket_ms: i64,
    tvl: Option<f64>,
    volume: Option<f64>,
    samples_in_bucket: u64,
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
    let (series_view, price_bucket_fn) = chart_price_series(interval);

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

    let rows = client
        .query(&sql)
        .bind(pool_id_hex)
        .bind(from.timestamp_millis()) // min(sequence): closed_at >= from
        .bind(from.timestamp_millis()) // max(sequence): closed_at >= from
        .bind(to.timestamp_millis()) // max(sequence): closed_at <  to
        .bind(from.timestamp_millis()) // ledgers dedup subquery: closed_at >= from
        .bind(to.timestamp_millis()) // ledgers dedup subquery: closed_at <  to
        .bind(ctx.leg_a.kind) // pa: identity
        .bind(ctx.leg_a.code.as_str())
        .bind(ctx.leg_a.issuer.as_str())
        .bind(from.timestamp_millis()) // pa: bucket >= floor(from)
        .bind(to.timestamp_millis()) // pa: bucket < to
        .bind(ctx.leg_b.kind) // pb: identity
        .bind(ctx.leg_b.code.as_str())
        .bind(ctx.leg_b.issuer.as_str())
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

/// SELECT column order MUST match this struct (clickhouse positional decode).
#[derive(Debug, Row, Deserialize)]
struct PoolListChRow {
    pool_id_hex: String,
    asset_a_type: i16,
    asset_a_code: Option<String>,
    asset_a_issuer_id: i64,
    asset_a_contract_id: Option<String>,
    asset_a_icon_url: Option<String>,
    asset_b_type: i16,
    asset_b_code: Option<String>,
    asset_b_issuer_id: i64,
    asset_b_contract_id: Option<String>,
    asset_b_icon_url: Option<String>,
    fee_bps: i32,
    created_at_ledger: i64,
    /// `last_updated_ledger` — the list sort/cursor key (see fn doc).
    cursor_ledger: i64,
    participant_count: i64,
    latest_snapshot_ledger: Option<i64>,
    reserve_a: Option<String>,
    reserve_b: Option<String>,
    total_shares: Option<String>,
    latest_snapshot_at_ms: Option<i64>,
    pool_kind: u8,
    deployment_id: i64,
    pool_type_raw: String,
    legs: Vec<i64>,
}

/// `GET /v1/liquidity-pools` — paginated pool list. Mirrors the PG
/// `fetch_pool_list` projection, with two CH-specific structural choices
/// driven by the box-measured read cost (`liquidity_pool_snapshots` = 268 M
/// rows):
///
/// - **Order key = `last_updated_ledger` (NOT `created_at_ledger`).** PG keys
///   on `created_at_ledger` (pool creation). CH `liquidity_pools` dropped that
///   column (PR #175); its only in-window proxy — `min(snapshot
///   ledger_sequence)` — is clamped to the frozen backfill floor (≈ L50.4M)
///   for every pre-window pool, so it is useless as an order key (mass ties)
///   *and* would force a full 268 M-row snapshot GROUP BY just to derive it.
///   `last_updated_ledger` is a native non-NULL column → the list pages the
///   small `liquidity_pools` table (51 k rows) FIRST, then seeks snapshots /
///   positions for only the page's ≤ limit+1 pools (`pool_id` is their leading
///   PK). Box-measured ≈ 55 M rows/page vs ≈ 268 M for the full-scan shape.
///   The wire `created_at_ledger` field still reports the min-snapshot proxy
///   (parity with detail); only the *ordering* differs, and the FE does not
///   consume the list yet, so there is no live ordering regression.
/// - **No `min_tvl` pre-filter.** It used to exist as a `tvl_pools` CTE doing
///   a full-scan `argMax(tvl)` over the snapshot column — a column task 0199
///   established is never written, so it matched nothing. The parameter is
///   now rejected with 400 at the handler rather than silently returning an
///   empty page that contradicts the per-row `tvl` this function computes.
///   Restoring it needs TVL for ALL pools per request (it changes page
///   membership, so it cannot ride the per-page price lookup) — i.e. the
///   prices-side identity-keyed materialized series.
///
/// Read-cost note for the eventual flag flip: the per-page ≈ 55 M is dominated
/// by the `accounts` id→strkey issuer resolution (14 M, non-PK reverse lookup)
/// and the `ledgers` closed_at join; both are bounded and the list is
/// user-initiated (not polled). The `operations_appearances` projection that
/// blocks the transactions endpoint does NOT block the list.
pub async fn fetch_pool_list(
    client: &clickhouse::Client,
    params: &ResolvedPoolListParams,
    direction: Direction,
) -> Result<Vec<PoolRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Keyset on `(last_updated_ledger, pool_id)`, expanded to scalar
    // comparisons. The cursor's `created_at_ledger` slot carries
    // `last_updated_ledger` on the CH path (opaque, ADR 0008). Bounds inlined:
    // `cursor_ledger` is i64 (no injection); `pool_id_hex` is validated hex.
    // A tampered/non-hex cursor degrades to "no keyset" (first page).
    let keyset = match params.cursor.as_ref() {
        Some(c) if is_hex_pool_id(&c.pool_id_hex) => format!(
            "AND ((lp.last_updated_ledger {op} {cl}) \
                  OR (lp.last_updated_ledger = {cl} \
                      AND lower(hex(lp.pool_id)) {op} '{ph}'))",
            op = op,
            cl = c.created_at_ledger,
            ph = c.pool_id_hex,
        ),
        _ => String::new(),
    };

    // Asset filters are bound (untrusted free-text codes / handler-validated
    // issuer StrKeys — clickhouse-rs escapes them). Each `?` appears in the
    // `page` CTE WHERE in this exact push order. Issuer StrKey → surrogate id
    // resolves via an `accounts` PK seek (`ORDER BY (account_id)`), cheap.
    let mut binds: Vec<String> = Vec::new();
    let mut filters = String::new();
    if let Some(code) = params.asset_a_code.as_ref() {
        filters.push_str(" AND lp.asset_a_code = ?");
        binds.push(code.clone());
    }
    if let Some(iss) = params.asset_a_issuer.as_ref() {
        filters.push_str(
            " AND lp.asset_a_issuer_id = \
              (SELECT id FROM accounts FINAL WHERE account_id = ? LIMIT 1)",
        );
        binds.push(iss.clone());
    }
    if let Some(code) = params.asset_b_code.as_ref() {
        filters.push_str(" AND lp.asset_b_code = ?");
        binds.push(code.clone());
    }
    if let Some(iss) = params.asset_b_issuer.as_ref() {
        filters.push_str(
            " AND lp.asset_b_issuer_id = \
              (SELECT id FROM accounts FINAL WHERE account_id = ? LIMIT 1)",
        );
        binds.push(iss.clone());
    }
    // Pool-world filter (task 0374). Inlined, not bound: the value is a
    // handler-validated 0/1 discriminant, injection-free by type, and the
    // bind vector here is string-typed.
    if let Some(kind) = params.pool_kind {
        filters.push_str(&format!(" AND lp.pool_kind = {kind}"));
    }
    // Asset-code needles (0440 / issue #366).
    //
    // Substring, not equality: `USD` has to match the `USDC` pools the user can
    // see on the page. `position` takes the needle literally — no LIKE wildcards
    // and no regex to escape, so caller free-text cannot widen its own match.
    // Case-insensitive here rather than `upper()` on the column: same result,
    // one pass, and it keeps working if the needle ever arrives un-normalized.
    //
    // Native legs are stored with an empty code (`asset_type = 0`, code `''`)
    // while every surface — this list included — renders them as `XLM`. Without
    // the alias, `XLM` matches none of the 11.7k pools that actually hold native
    // XLM, and instead returns ~3.7k pools of credit assets someone minted under
    // the code `XLM` (they exist, including `XLM/XLM` pairs). That is not an
    // empty result, it is a confident wrong one — so the predicate searches what
    // the row displays as.
    //
    // A pair assigns each needle its OWN leg, in either order, rather than
    // asking each needle independently whether it matches somewhere. The
    // difference only shows when the needles overlap, and then it is the whole
    // answer: `USDC/USDC` means the 72 pools with USDC on both sides, not the
    // 2 912 with USDC anywhere. Same for a needle that is a prefix of the other
    // (`USD/USDC`) — one asset must not satisfy both halves of the query.
    //
    // The predicate itself lives in `common::pool_asset_codes` because global
    // search matches pools with the SAME rule (task 0470); two copies would
    // drift, and the native arm above is exactly where a second one goes wrong.
    //
    // A pool identifier in the same box wins outright: it names one pool, so
    // it is a point seek on the primary key rather than a scan, and there is
    // nothing left for a code match to narrow. Before this, the identifier was
    // matched as a substring of an asset code and the page said "no pools".
    if let Some(pool_hex) = params.pool_id_hex.as_ref() {
        filters.push_str(" AND lp.pool_id = unhex(?)");
        binds.push(pool_hex.clone());
    } else if let Some((clause, clause_binds)) =
        asset_codes_predicate(params.asset_codes.as_slice())
    {
        filters.push_str(&format!(" AND {clause}"));
        binds.extend(clause_binds);
    }

    // Latest-snapshot fields via `argMax(...) GROUP BY pool_id` over a bounded
    // `ledger_sequence` band around the page's `last_updated_ledger` range (the
    // `band` CTE, ±10k). Page pools are the most-recently-updated, so their
    // latest snapshot sits in that band — a bounded seek (~0.5M rows / ~50ms)
    // instead of a full per-pool history scan (30M rows, which OOMed the 4 GB
    // read-only profile as PR #335's `LIMIT 1 BY` sort). NO `FINAL`: the band's
    // max ledger per page pool is recent (post-0356/#318 single-image) so
    // per-column `argMax` can't tear; only a pool whose latest snapshot predates
    // #318 (inactive for weeks → deep pages) could, which is accepted.
    // `created_at_ledger` = `min(ledger_sequence)` in the `cr` subquery (cheap
    // narrow streaming scan, dup-invariant → no `FINAL`); `l_snap` seeks
    // `ledgers` by the page's ~20 `last_updated_ledger`s (a full `ledgers` join
    // built a 26M-row / 3.3 GB hash); `sac`/`asset_sac` prune to the page codes.
    //
    // Do NOT rewrite as `ORDER BY ledger_sequence DESC LIMIT 1 BY pool_id`
    // (PR #335, reverted): `LIMIT 1 BY` is NOT a seek — it fully materialises +
    // sorts every snapshot of the page's pools (~30M rows for the busiest 20),
    // OOMing the 4 GB read-only CH profile. A future perf pass must keep the
    // O(page pools) shape (e.g. `argMax` over a whole-row tuple), not a sort.
    //
    // Aggregates wrap in `toNullable(...)` so a no-snapshot pool yields NULL (not
    // the 0/'' default) on the LEFT JOIN miss — `join_use_nulls` is rejected for
    // the read-only CH user, so this is the readonly-safe NULL path. (Every
    // current pool has ≥ 1 snapshot, so this is defensive.) `nullIf(...)` does the
    // same for the empty-string-sentinel string columns. Native legs
    // (asset_code = '') are excluded from the SAC join by the `lp.asset_*_code !=
    // ''` guard so they surface a NULL `contract_id`, matching PG (NULL code → no
    // SAC match).
    let sql = format!(
        "WITH \
         page AS ( \
             SELECT lp.pool_id AS pool_id, lp.asset_a_type AS asset_a_type, \
                    lp.asset_a_code AS asset_a_code, lp.asset_a_issuer_id AS asset_a_issuer_id, \
                    lp.asset_b_type AS asset_b_type, lp.asset_b_code AS asset_b_code, \
                    lp.asset_b_issuer_id AS asset_b_issuer_id, lp.fee_bps AS fee_bps, \
                    lp.last_updated_ledger AS last_updated_ledger, \
                    lp.pool_kind AS pool_kind, lp.deployment_id AS deployment_id, \
                    lp.pool_type_raw AS pool_type_raw, lp.legs AS legs \
             FROM liquidity_pools lp FINAL \
             WHERE 1 = 1{filters} {keyset} \
             ORDER BY last_updated_ledger {order}, pool_id {order} \
             LIMIT {limit} \
         ), \
         band AS ( \
             SELECT min(last_updated_ledger) - 10000 AS lo, \
                    max(last_updated_ledger) + 10000 AS hi FROM page \
         ), \
         codes AS ( \
             SELECT asset_a_code AS c FROM page \
             UNION ALL SELECT asset_b_code FROM page \
         ), \
         sac AS ( \
             SELECT a.asset_code AS asset_code, a.issuer_id AS issuer_id, \
                    max(sc.contract_id)      AS contract_id, \
                    nullIf(max(ae.icon_url), '') AS icon_url \
             FROM assets a \
             LEFT JOIN ( \
                 SELECT asset_type, asset_code, issuer_id, contract_id, \
                        max(sac_contract_id) AS sac_contract_id \
                 FROM asset_sac \
                 WHERE asset_type IN (0, 1) AND asset_code IN (SELECT c FROM codes) \
                 GROUP BY asset_type, asset_code, issuer_id, contract_id \
             ) asac ON asac.asset_type = a.asset_type AND asac.asset_code = a.asset_code \
                   AND asac.issuer_id = a.issuer_id AND asac.contract_id = a.contract_id \
             LEFT JOIN soroban_contracts sc ON sc.id = asac.sac_contract_id AND asac.sac_contract_id != 0 \
             LEFT JOIN ( \
                 SELECT asset_type, asset_code, issuer_id, contract_id, \
                        argMax(icon_url, version) AS icon_url \
                 FROM asset_enrichment \
                 WHERE asset_type IN (0, 1) AND asset_code IN (SELECT c FROM codes) \
                 GROUP BY asset_type, asset_code, issuer_id, contract_id \
             ) ae ON ae.asset_type = a.asset_type AND ae.asset_code = a.asset_code \
                 AND ae.issuer_id = a.issuer_id AND ae.contract_id = a.contract_id \
             WHERE a.asset_type IN (0, 1) AND a.asset_code IN (SELECT c FROM codes) \
               AND (a.asset_code, a.issuer_id) IN ( \
                   SELECT asset_a_code, asset_a_issuer_id FROM page \
                   UNION ALL SELECT asset_b_code, asset_b_issuer_id FROM page) \
             GROUP BY a.asset_code, a.issuer_id \
         ) \
         SELECT \
             lower(hex(lp.pool_id))                          AS pool_id_hex, \
             lp.asset_a_type                                 AS asset_a_type, \
             nullIf(lp.asset_a_code, '')                     AS asset_a_code, \
             lp.asset_a_issuer_id                            AS asset_a_issuer_id, \
             nullIf(sac_a.contract_id, '')                   AS asset_a_contract_id, \
             sac_a.icon_url                                  AS asset_a_icon_url, \
             lp.asset_b_type                                 AS asset_b_type, \
             nullIf(lp.asset_b_code, '')                     AS asset_b_code, \
             lp.asset_b_issuer_id                            AS asset_b_issuer_id, \
             nullIf(sac_b.contract_id, '')                   AS asset_b_contract_id, \
             sac_b.icon_url                                  AS asset_b_icon_url, \
             lp.fee_bps                                      AS fee_bps, \
             ifNull(cr.created_at_ledger, lp.last_updated_ledger) AS created_at_ledger, \
             lp.last_updated_ledger                          AS cursor_ledger, \
             toInt64(ifNull(pc.participant_count, 0))        AS participant_count, \
             s.latest_ledger_sequence                        AS latest_snapshot_ledger, \
             toString(s.reserve_a)                           AS reserve_a, \
             toString(s.reserve_b)                           AS reserve_b, \
             toString(s.total_shares)                        AS total_shares, \
             nullIf(toUnixTimestamp64Milli(l_snap.closed_at), 0) AS latest_snapshot_at_ms, \
             lp.pool_kind                                    AS pool_kind, \
             lp.deployment_id                                AS deployment_id, \
             lp.pool_type_raw                                AS pool_type_raw, \
             lp.legs                                         AS legs \
         FROM page lp \
         LEFT JOIN ( \
             SELECT pool_id, \
                toNullable(max(ledger_sequence))                  AS latest_ledger_sequence, \
                argMax(toNullable(reserve_a), ledger_sequence)    AS reserve_a, \
                argMax(toNullable(reserve_b), ledger_sequence)    AS reserve_b, \
                argMax(toNullable(total_shares), ledger_sequence) AS total_shares \
             FROM liquidity_pool_snapshots \
             WHERE pool_id IN (SELECT pool_id FROM page) \
               AND ledger_sequence BETWEEN (SELECT lo FROM band) AND (SELECT hi FROM band) \
             GROUP BY pool_id \
         ) s ON s.pool_id = lp.pool_id \
         LEFT JOIN ( \
             SELECT pool_id, toNullable(min(ledger_sequence)) AS created_at_ledger \
             FROM liquidity_pool_snapshots \
             WHERE pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) cr ON cr.pool_id = lp.pool_id \
         LEFT JOIN ( \
             SELECT pool_id, count() AS participant_count FROM lp_positions FINAL \
             WHERE shares > 0 AND pool_id IN (SELECT pool_id FROM page) \
             GROUP BY pool_id \
         ) pc ON pc.pool_id = lp.pool_id \
         LEFT JOIN sac sac_a ON sac_a.asset_code = lp.asset_a_code \
                            AND sac_a.issuer_id = lp.asset_a_issuer_id \
         LEFT JOIN sac sac_b ON sac_b.asset_code = lp.asset_b_code \
                            AND sac_b.issuer_id = lp.asset_b_issuer_id \
         /* `GROUP BY sequence` dedups `ledgers` (ReplacingMergeTree, unmerged \
            duplicate rows): without it this LEFT JOIN doubled every page row \
            whose latest snapshot ledger falls in the duplicated range, doubling \
            UI rows and breaking keyset pagination. `any(closed_at)` is exact \
            (measured: closed_at identical across every dup pair). lore-0420 \
            \
            The filter is `page.last_updated_ledger` while the join key is \
            `s.latest_ledger_sequence` — these look mismatched but are the same \
            set, because a pool's last update always writes a snapshot at that \
            ledger: measured 52,284 of 52,284 pools with \
            `last_updated_ledger = max(ledger_sequence)`. If that invariant ever \
            breaks the join simply misses and `latest_snapshot_at_ms` is null — \
            degraded, never wrong. */ \
         LEFT JOIN ( \
             SELECT sequence, any(closed_at) AS closed_at FROM ledgers \
             WHERE sequence IN (SELECT last_updated_ledger FROM page) \
             GROUP BY sequence \
         ) l_snap ON l_snap.sequence = s.latest_ledger_sequence \
         ORDER BY lp.last_updated_ledger {order}, lp.pool_id {order}",
        filters = filters,
        keyset = keyset,
        order = order,
        limit = params.limit,
    );

    let mut query = client.query(&sql);
    for b in &binds {
        query = query.bind(b.as_str());
    }
    let rows = query.fetch_all::<PoolListChRow>().await?;

    // Resolve issuer StrKeys by surrogate id (bloom seek). The old in-query `iss`
    // CTE used `WHERE id IN (SELECT … FROM page)` — the subquery form does not
    // trigger the `idx_acc_id` bloom, so it scanned `accounts.id` (task 0345).
    let issuer_ids = rows
        .iter()
        .flat_map(|r| [r.asset_a_issuer_id, r.asset_b_issuer_id])
        // Exclude the native sentinel `0` — the old `iss` CTE filtered
        // `WHERE … != 0`. A no-op on real data (`accounts.id = cityhash64(strkey)`
        // is never 0), but keeps the resolution unconditionally identical.
        .filter(|&id| id != 0)
        .collect();
    let accounts = resolve_accounts(client, issuer_ids).await?;

    // Phase A2 (issue #367): per-row USD TVL, computed like the detail
    // endpoint (latest reserves × last 1h close per leg; both legs required)
    // from ONE batched price lookup over the page's distinct identities.
    // `volume`/`fee_revenue` stay NULL on the list — detail-only semantics.
    // A prices error degrades every row to NULL TVL (error-logged), it does
    // not fail the list: same resilience contract as the detail endpoint.
    let page_legs: Vec<(PriceLeg, PriceLeg)> = rows
        .iter()
        .map(|r| {
            let issuer_a = accounts.get(&r.asset_a_issuer_id).map(String::as_str);
            let issuer_b = accounts.get(&r.asset_b_issuer_id).map(String::as_str);
            (
                price_leg(r.asset_a_type, r.asset_a_code.as_deref(), issuer_a),
                price_leg(r.asset_b_type, r.asset_b_code.as_deref(), issuer_b),
            )
        })
        .collect();
    let mut unique_legs: Vec<&PriceLeg> = page_legs
        .iter()
        .flat_map(|(a, b)| [a, b])
        .filter(|l| !l.kind.is_empty())
        .collect();
    unique_legs
        .sort_unstable_by(|a, b| (a.kind, &a.code, &a.issuer).cmp(&(b.kind, &b.code, &b.issuer)));
    unique_legs.dedup();
    let closes = match fetch_last_closes(client, &unique_legs).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("DB error in fetch_last_closes (list TVL degraded to NULL): {e}");
            std::collections::HashMap::new()
        }
    };

    Ok(rows
        .into_iter()
        .zip(page_legs)
        .map(|(r, (leg_a, leg_b))| {
            let tvl = match (
                r.reserve_a.as_deref().and_then(parse_f64),
                r.reserve_b.as_deref().and_then(parse_f64),
                closes.get(&leg_a).copied(),
                closes.get(&leg_b).copied(),
            ) {
                (Some(ra), Some(rb), Some(pa), Some(pb)) => Some(usd_str(ra * pa + rb * pb)),
                _ => None,
            };
            PoolRow {
                pool_id_hex: r.pool_id_hex,
                asset_a_type: r.asset_a_type,
                asset_a_type_name: asset_type_name(r.asset_a_type),
                asset_a_code: r.asset_a_code,
                asset_a_issuer: accounts
                    .get(&r.asset_a_issuer_id)
                    .cloned()
                    .filter(|s| !s.is_empty()),
                asset_a_contract_id: r.asset_a_contract_id,
                asset_a_icon_url: r.asset_a_icon_url,
                asset_b_type: r.asset_b_type,
                asset_b_type_name: asset_type_name(r.asset_b_type),
                asset_b_code: r.asset_b_code,
                asset_b_issuer: accounts
                    .get(&r.asset_b_issuer_id)
                    .cloned()
                    .filter(|s| !s.is_empty()),
                asset_b_contract_id: r.asset_b_contract_id,
                asset_b_icon_url: r.asset_b_icon_url,
                fee_bps: r.fee_bps,
                fee_percent: fee_percent_str(r.fee_bps),
                created_at_ledger: r.created_at_ledger,
                cursor_ledger: r.cursor_ledger,
                // Soroban rows: lp_positions' 0 is the wrong population's
                // truth — mask to None (≠ 0), same as detail.
                participant_count: (r.pool_kind == 0).then_some(r.participant_count),
                latest_snapshot_ledger: r.latest_snapshot_ledger,
                reserve_a: r.reserve_a,
                reserve_b: r.reserve_b,
                total_shares: r.total_shares,
                tvl,
                volume: None,
                fee_revenue: None,
                latest_snapshot_at: r.latest_snapshot_at_ms.map(millis_to_utc),
                pool_kind: r.pool_kind,
                deployment_id: r.deployment_id,
                pool_type_raw: r.pool_type_raw,
                legs: r.legs,
            }
        })
        .collect())
}

#[cfg(test)]
#[path = "queries_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "queries_decode_smoke.rs"]
mod decode_smoke;
