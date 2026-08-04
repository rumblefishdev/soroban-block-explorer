//! Request and response DTOs for the liquidity-pool endpoints.
//!
//! Participants endpoint (task 0126) and the list/detail/transactions/chart
//! endpoints (tasks 0052) share this module. Wire shapes mirror canonical
//! SQL `endpoint-queries-clickhouse/{18,19,20,21,23}_*.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// ---------------------------------------------------------------------------
// Participants (task 0126) — UNCHANGED
// ---------------------------------------------------------------------------

/// Cursor payload for `(shares DESC, account_id DESC)` pagination.
///
/// `shares` is carried as a decimal string preserving `NUMERIC(28,7)`
/// precision across the wire so PG comparison stays exact across the
/// fractional component without an f64 round-trip. `account_id` is the
/// surrogate `BIGINT` from `accounts.id` — its direction matches the
/// ORDER BY tie-breaker on equal-shares pages. Cursor stays opaque per
/// ADR 0008; this struct is only deserialized inside the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharesCursor {
    pub shares: String,
    pub account_id: i64,
}

/// One participant row returned by the participants list. Shape pinned to
/// `docs/architecture/database-schema/endpoint-queries-clickhouse/23_get_liquidity_pools_participants.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ParticipantItem {
    /// Participant account StrKey (G...).
    pub account: String,
    /// Pool-share balance carried as a decimal string preserving the
    /// underlying `NUMERIC(28,7)` precision (no f64 round-trip).
    pub shares: String,
    /// Share of the pool, expressed as a decimal-string percentage
    /// (`100 * shares / total_pool_shares`). `None` when the pool has no
    /// snapshot in the freshness window (stale pool); the frontend renders
    /// it as "—" in that case (matches the list-endpoint stale-pool
    /// convention from `18_get_liquidity_pools_list.sql`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_percentage: Option<String>,
    /// Ledger of the first deposit by this account into this pool.
    pub first_deposit_ledger: i64,
    /// Ledger of the most recent change to this position.
    pub last_updated_ledger: i64,
}

// ---------------------------------------------------------------------------
// List / Detail / Transactions / Chart (task 0052)
// ---------------------------------------------------------------------------

/// `filter[...]` query parameters for `GET /v1/liquidity-pools`.
///
/// Two asset-filter modes coexist:
///   * **`filter[asset_code]`** — single-asset, case-insensitive exact
///     match against either leg. Convenience for the Figma list filter
///     (frontend §6.13) where the user types just `USDC` / `XLM`.
///   * **Per-leg `asset_a_code` / `asset_a_issuer` / `asset_b_code` /
///     `asset_b_issuer`** — kept for API consumers that need exact
///     issuer disambiguation (`code, issuer` is the classic identity).
///
/// Both modes can combine — the WHERE clause is additive. `limit` /
/// `cursor` are read by a sibling `Pagination<PoolListCursor>` extractor.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PoolListParams {
    /// Single-asset filter — matches either `asset_a_code` or
    /// `asset_b_code` case-insensitively (input is trimmed + uppercased
    /// before the query). Intended for the Figma list's free-text
    /// "Filter by asset pair" input.
    #[serde(rename = "filter[asset_code]")]
    pub filter_asset_code: Option<String>,
    #[serde(rename = "filter[asset_a_code]")]
    pub filter_asset_a_code: Option<String>,
    #[serde(rename = "filter[asset_a_issuer]")]
    pub filter_asset_a_issuer: Option<String>,
    #[serde(rename = "filter[asset_b_code]")]
    pub filter_asset_b_code: Option<String>,
    #[serde(rename = "filter[asset_b_issuer]")]
    pub filter_asset_b_issuer: Option<String>,
    /// Minimum TVL threshold as a decimal string (matches the underlying
    /// `NUMERIC(28,7)` column without an f64 round-trip).
    #[serde(rename = "filter[min_tvl]")]
    pub filter_min_tvl: Option<String>,
}

/// One leg of an LP's asset pair. Surfaces both the decoded
/// `asset_type_name` (SQL `asset_type_name()`) and the raw `asset_type`
/// SMALLINT — same contract as `assets/dto::AssetItem`.
///
/// Linkable identifiers (task 0263 / F-K-9). All link targets are the
/// **asset detail page** (`/assets/...`) — `parse_asset_id` is polymorphic
/// and accepts both C-strkey (SAC) and `code-issuer` composite, so all
/// non-native legs resolve to the same asset row.
///
///   * `asset_type == 0` — native XLM; FE renders unlinked (no on-chain
///     address in classic Stellar protocol; SAC mirror is network-dependent).
///   * `contract_id` — C-strkey of the SAC mirror for a classic credit
///     leg (populated when the leg's `(asset_code, issuer)` classic_credit /
///     native `assets` row carries a deployed SAC facet — `sac_contract_id`
///     resolving a `soroban_contracts.contract_id`, ADR 0051). `None` for legs
///     without a deployed SAC mirror. Pool legs only carry XDR `AssetType` (native /
///     credit_alphanum4 / credit_alphanum12) per `0006_liquidity_pools.sql`,
///     so SAC / Soroban legs are not directly representable here;
///     `contract_id` surfaces the SAC mirror look-up so the FE can
///     route to the asset detail page via `/assets/${contract_id}`.
///   * `issuer` + `asset_code` (classic credit, no SAC mirror) — FE
///     routes to `/assets/${asset_code}-${issuer}` (composite form
///     accepted by `parse_asset_id`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolAssetLeg {
    /// `native | classic_credit | soroban` (ADR 0051 — `sac` retired). `null`
    /// only on schema drift.
    pub asset_type_name: Option<String>,
    /// Raw SMALLINT (0=native, 1=classic_credit, 3=soroban). 2 (`sac`) is retired.
    pub asset_type: i16,
    pub asset_code: Option<String>,
    pub issuer: Option<String>,
    /// C-strkey of the deployed SAC mirror for the leg's `(asset_code, issuer)`
    /// classic_credit / native asset (ADR 0051 — resolved via the row's
    /// `sac_contract_id` facet). `None` for native legs and for classic credit
    /// legs without a deployed SAC.
    pub contract_id: Option<String>,
    /// Asset icon URL, mirrored from the leg's `assets.icon_url` row so
    /// pool avatars render the same icon as the assets list. `None` for
    /// native legs and assets without an enriched icon — the FE falls back
    /// to the asset-code initial.
    pub icon_url: Option<String>,
}

/// One pool row returned by the list endpoint. Shape pinned to canonical
/// SQL `18_get_liquidity_pools_list.sql`. Pools without a fresh snapshot
/// in the freshness window come back with `null` for every dynamic field
/// (`reserve_a`, `reserve_b`, `total_shares`, `tvl`, `volume`,
/// `fee_revenue`, `latest_snapshot_*`); frontend renders these as "stale".
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolItem {
    /// SEP-23 strkey (`L...`, 56 chars). DB stores `BYTEA(32)` per ADR
    /// 0024; the handler encodes to strkey at the response boundary so
    /// the wire shape matches the Stellar ecosystem canonical form
    /// (CAP-38 / SEP-23).
    pub pool_id: String,
    pub asset_a: PoolAssetLeg,
    pub asset_b: PoolAssetLeg,
    pub fee_bps: i32,
    /// `fee_bps / 100` as decimal string. Conversion done server-side so
    /// the frontend can render directly (frontend §6.13/§6.14).
    pub fee_percent: String,
    pub created_at_ledger: i64,
    /// Count of active liquidity providers (`lp_positions WHERE shares > 0`).
    /// Computed from the live table — not dependent on the snapshot
    /// freshness window, so it is populated even on stale pools (where
    /// `tvl`/`volume`/`fee_revenue` are NULL).
    pub participant_count: i64,
    pub latest_snapshot_ledger: Option<i64>,
    pub reserve_a: Option<String>,
    pub reserve_b: Option<String>,
    pub total_shares: Option<String>,
    /// USD, decimal string rounded to cents (task 0199 compute-at-read).
    /// **Detail endpoint only** — the list returns `null` (list-side TVL is
    /// a follow-up). `tvl` = latest reserves × each leg's last hourly USD
    /// close (`prices.price_usd_series_1h`, ≤ ~2h stale); `null` unless
    /// both legs price (never a one-leg partial).
    pub tvl: Option<String>,
    /// USD, decimal string rounded to cents. **Detail endpoint only.**
    /// Gross trade volume over the last 24h (`gross_volume_a` sum) priced
    /// at the leg-A last hourly close; `null` when the pool is unpriceable.
    pub volume: Option<String>,
    /// USD, decimal string rounded to cents. **Detail endpoint only.**
    /// `volume × fee_bps / 10000` — the pool's 24h fee estimate.
    pub fee_revenue: Option<String>,
    pub latest_snapshot_at: Option<DateTime<Utc>>,
}

/// One row from `/liquidity-pools/:id/transactions`. Shape pinned to
/// canonical SQL `20_get_liquidity_pools_transactions.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolTransactionItem {
    pub hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    /// Fee charged, in raw stroops. Native (XLM) is always 7 decimals, so
    /// there is no `decimals` field — the frontend scales by 1e7.
    pub fee_charged: i64,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    /// Distinct `op_type_name(...)` labels for every op in the tx, sorted
    /// asc. Frontend §6.14 categorises trade vs LP-mgmt activity from this
    /// list (policy lives client-side, not in SQL).
    pub operation_types: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Cursor payload for `GET /v1/liquidity-pools` paginated by
/// `(created_at_ledger DESC, pool_id DESC)`. The `pool_id` half travels
/// as 64-char lowercase hex; the SQL decodes it back to BYTEA inside the
/// keyset predicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolListCursor {
    pub created_at_ledger: i64,
    pub pool_id_hex: String,
}

/// Query params for `GET /v1/liquidity-pools/:id/chart`.
///
/// All three params are **optional**. Sensible defaults match the picked
/// interval so a bare request returns a useful chart:
///   - `interval` default: `1d`
///   - `to` default: `now()`
///   - `from` default: `to - <interval-appropriate window>` —
///     `1h → 7 days` (168 buckets), `1d → 90 days` (90 buckets),
///     `1w → 104 weeks` (104 buckets, ≈ 2 years)
///
/// Caller can override any subset. The bucket-count cap (handler-side)
/// rejects ranges that would explode aggregation cost.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ChartParams {
    /// Bucket width: `1h` | `1d` | `1w`. Validated against an allowlist.
    /// Default: `1d`.
    pub interval: Option<String>,
    /// Inclusive lower bound, ISO 8601 / RFC 3339 timestamp.
    /// Default: `to` minus the interval-appropriate window (see struct doc).
    pub from: Option<String>,
    /// Exclusive upper bound, ISO 8601 / RFC 3339 timestamp.
    /// Default: `now()`.
    pub to: Option<String>,
}

/// One row from the chart endpoint. All money fields are **USD decimal
/// strings rounded to cents**, computed at read from on-chain quantities ×
/// the in-cluster price series (task 0199, ADR 0053):
/// - `tvl` — "TVL at close of bucket": last priceable snapshot's
///   `reserve_a·price_a + reserve_b·price_b`. `null` when either leg has
///   no price at that point (untracked asset, pre-listing history, or a
///   provider-side price gap).
/// - `volume` — SUM over the bucket of per-ledger gross trade volume ×
///   the leg-A price at that ledger's time. `null` for no-swap buckets and
///   for buckets where a swap couldn't be priced (never a partial sum).
/// - `fee_revenue` — `volume × fee_bps / 10000`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChartDataPoint {
    pub bucket: DateTime<Utc>,
    pub tvl: Option<String>,
    pub volume: Option<String>,
    pub fee_revenue: Option<String>,
    pub samples_in_bucket: i64,
}

/// `GET /v1/liquidity-pools/:id/chart` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChartResponse {
    /// Echoed pool ID — SEP-23 strkey (`L...`, 56 chars), same form the
    /// client supplied in the path.
    pub pool_id: String,
    pub interval: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub data_points: Vec<ChartDataPoint>,
}
