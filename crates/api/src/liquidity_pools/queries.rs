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
//! - No freshness window on the detail/list latest-snapshot pick (the PG
//!   design had `snapshots.created_at >= NOW() - 7d`). A classic pool writes
//!   a snapshot on every change of its ledger entry, so the latest one IS its
//!   current state whatever its age — an old snapshot means a quiet pool, not
//!   an outdated reading. The participants endpoint still carries the window
//!   (0374 PR 5 removes it).

use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::common::asset_identity::ResolvedAsset;

use leg_reserves::Reserves;

// ---------------------------------------------------------------------------
// Internal query-result rows + resolved params (not serialized; the handler
// maps these into the public response DTOs).
// ---------------------------------------------------------------------------

/// Canonical pool column projection shared between list and detail.
#[derive(Debug, Clone)]
pub struct PoolRow {
    pub pool_id_hex: String,
    /// `liquidity_pools.pool_kind`, decoded once by `decode_pool_kind`.
    pub pool_kind: domain::PoolKind,
    /// The pool's legs in registration order: two for a classic pool, two to
    /// four for a soroban one.
    pub legs: Vec<PoolLegRow>,
    pub fee_bps: i32,
    pub fee_percent: String,
    pub created_at_ledger: i64,
    /// Ledger value the list keyset orders + paginates on: the pool's last
    /// activity (`list_pools::ACTIVITY_LEDGER`), carried here.
    /// The wire `PoolListCursor.created_at_ledger` slot stays opaque (ADR
    /// 0008); only this field feeds the cursor builder. Unused by detail.
    pub cursor_ledger: i64,
    /// `COUNT(*) FROM lp_positions WHERE pool_id = lp.pool_id AND shares > 0`.
    /// Task 0246 — see DTO doc for surfacing rules.
    pub participant_count: i64,
    pub latest_snapshot_ledger: Option<i64>,
    pub total_shares: Option<String>,
    pub tvl: Option<String>,
    pub volume: Option<String>,
    pub fee_revenue: Option<String>,
    pub latest_snapshot_at: Option<DateTime<Utc>>,
}

/// One leg as the query layer resolved it. The handler names `family` for the
/// wire; the raw discriminant stays here because the price join keys on it.
#[derive(Debug, Clone)]
pub struct PoolLegRow {
    /// `assets.asset_type` — the AssetFamily discriminant, named at the
    /// boundary via [`domain::AssetFamily`] rather than by a local match.
    pub family: i16,
    pub asset_code: Option<String>,
    /// `G…` StrKey, resolved from the surrogate by the shared bloom seek.
    pub issuer: Option<String>,
    /// The token's own `C…` contract — a soroban leg only.
    pub contract_id: Option<String>,
    /// The token's self-declared SEP-41 symbol — what names a soroban leg
    /// that has no classic code.
    pub symbol: Option<String>,
    pub icon_url: Option<String>,
    /// What the pool holds of this leg, in units — see `PoolAssetLeg::reserve`.
    pub reserve: Option<String>,
}

/// Turn one pool's stored leg surrogates into the rows the handler finishes.
///
/// A surrogate the asset dimension does not know still yields a leg: the pool
/// genuinely holds that token, and dropping it would silently shorten the pool.
/// It renders by whatever identity survives — the contract address — which is
/// the honest answer rather than a missing leg.
fn leg_rows(
    leg_ids: &[i64],
    identities: &HashMap<i64, ResolvedAsset>,
    icons: &HashMap<i64, String>,
    reserves: Reserves<'_>,
) -> Vec<PoolLegRow> {
    leg_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            // A raw soroban reserve scales only by decimals that are a fact.
            let scale = identities.get(id).and_then(|r| r.decimals);
            let reserve = reserves.at(i, scale);
            match identities.get(id) {
                Some(r) if r.known => PoolLegRow {
                    family: r.asset_type,
                    asset_code: r.asset_code.clone(),
                    issuer: r.issuer.clone(),
                    // The contract is the asset identity only for a Soroban
                    // token; a classic leg's contract column is its SAC, which
                    // is not a route (see `PoolAssetLeg`).
                    contract_id: (r.asset_type == domain::AssetFamily::Soroban as i16)
                        .then(|| r.contract_strkey.clone())
                        .flatten(),
                    symbol: r.symbol.clone(),
                    icon_url: icons.get(id).cloned(),
                    reserve,
                },
                // Unknown to `assets`: no family, no code — only the contract
                // and its symbol, which `soroban_contracts` and its metadata
                // still name.
                other => PoolLegRow {
                    family: -1,
                    asset_code: None,
                    issuer: None,
                    contract_id: other.and_then(|r| r.contract_strkey.clone()),
                    symbol: other.and_then(|r| r.symbol.clone()),
                    icon_url: None,
                    reserve,
                },
            }
        })
        .collect()
}

mod get_pool;
mod get_pool_chart;
mod leg_reserves;
mod list_participants;
mod list_pool_activity;
mod list_pools;
mod total_shares;
mod usd_analytics;

pub use get_pool::fetch_pool_by_id;
pub use get_pool_chart::fetch_pool_chart;
pub use list_participants::{fetch_participants, pool_exists};
pub use list_pool_activity::{fetch_pool_activity, fetch_pool_asset_ids};
pub use list_pools::{ResolvedPoolListParams, fetch_pool_list};
pub use usd_analytics::{
    PoolPriceContext, fetch_pool_price_context, fetch_pool_usd_analytics, price_leg,
};

/// The largest scale treated as a fact. A `u128` has 39 digits, so no real
/// token needs more; a larger value is broken or hostile metadata (two live
/// contracts declare 43,224) and would otherwise size the padding below.
const MAX_SCALE: u32 = 38;

/// A raw integer amount as a decimal string, scaled by `decimals`.
///
/// STRING SURGERY, not arithmetic: the value is a `u128` out of contract
/// storage, an `f64` drops digits above 2^53, and a `Decimal128` division would
/// have to pick its scale up front. Inserting the point is exact at every
/// magnitude.
fn scale_decimal_str(raw: &str, decimals: u32) -> Option<String> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) || decimals > MAX_SCALE {
        return None;
    }
    let d = decimals as usize;
    if d == 0 {
        return Some(raw.to_string());
    }
    // Left-pad so there is always at least one integer digit.
    let padded = format!("{raw:0>width$}", width = d + 1);
    let split = padded.len() - d;
    let frac = padded[split..].trim_end_matches('0');
    Some(if frac.is_empty() {
        padded[..split].to_string()
    } else {
        format!("{}.{}", &padded[..split], frac)
    })
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

#[cfg(test)]
mod tests;

/// Live-CH **decode** smoke for the LP read path.
///
/// The curl `FORMAT TSV/Vertical/JSON` box smokes do NOT exercise the
/// clickhouse-rs RowBinary decoder, so a wire-type↔struct mismatch — e.g. a
/// scalar `(SELECT count() …)` typed `Nullable(UInt64)` decoded into an `i64`
/// field (the detail `participant_count` bug, task 0243) — passes a curl check
/// yet 500s the live endpoint with `schema mismatch`. A pure-Rust round-trip
/// can't catch it either (the struct serializes consistently with itself). The
/// only real guard is decoding rows that an actual CH produced.
///
/// This test runs each cheap LP CH fetch fn against a real CH and asserts the
/// rows decode (no error). It **skips cleanly when `CH_URL` is unset**, so CI
/// (no CH access) is unaffected. Run it against a reachable CH — a local
/// replica or an SSH tunnel to the box:
///
/// ```text
/// CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
///   cargo test -p api --lib decode_smoke -- --nocapture
/// ```
///
/// `transactions` is intentionally excluded: its driver scans the whole
/// `operations_appearances` table (~7.87B rows) until the `pool_id` projection
/// lands, so exercising it here would blow the read quota. Its row struct is all
/// direct, non-null columns (audited — no Nullable-decode risk).
#[cfg(test)]
mod decode_smoke;
