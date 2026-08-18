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
///   * **`filter[asset_code]`** — free text, case-insensitive **substring**
///     match against either leg. Convenience for the list filter (frontend
///     §6.13) where the user types just `USDC` / `XLM` — or `USD`, which
///     matches every `USDC` pool, or `USDC/XLM` for a pair.
///   * **Per-leg `asset_a_code` / `asset_a_issuer` / `asset_b_code` /
///     `asset_b_issuer`** — kept for API consumers that need exact
///     issuer disambiguation (`code, issuer` is the classic identity).
///
/// Both modes can combine — the WHERE clause is additive. `limit` /
/// `cursor` are read by a sibling `Pagination<PoolListCursor>` extractor.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PoolListParams {
    /// Free-text asset filter — case-insensitive substring of either
    /// `asset_a_code` or `asset_b_code` (input is trimmed before the
    /// query). The needle is matched literally: `%`, `_` and regex
    /// metacharacters have no special meaning.
    ///
    /// A `/` makes it a **pair** query: `USDC/XLM` requires both codes to be
    /// present, one on each leg, and the typed order does not matter. Only the
    /// first `/` splits, so `USDC/XLM/BTC` searches for the literal second code
    /// `XLM/BTC` and therefore matches nothing — a pool has two legs.
    ///
    /// Native legs match on `XLM` even though they store an empty code, so
    /// `XLM` returns the pools that actually hold native XLM. Note that it
    /// *also* returns credit assets minted under the code `XLM` — asset codes
    /// are not unique on Stellar, and this filter matches codes, not asset
    /// identity. Callers needing one specific issuer's asset should use the
    /// per-leg `filter[asset_a_code]` + `filter[asset_a_issuer]` pair.
    ///
    /// A pool IDENTIFIER is also accepted here — the `L…` SEP-23 StrKey,
    /// the one canonical form (task 0264) — and selects that single pool
    /// instead of matching asset codes (task 0470). Previously an identifier
    /// was matched as a substring of an asset code, found nothing, and the
    /// list answered "no pools" about a pool that exists.
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
    /// Asset icon URL, resolved from `asset_enrichment` (ADR 0050) so pool
    /// avatars render the same icon as the assets list. Until task 0310 this
    /// read the dead `assets.icon_url` column, which was never populated —
    /// every leg icon came back `None`. Still `None` for assets without an
    /// enriched icon — the FE falls back to the asset-code initial.
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
    /// Populated on **both** the list (Phase A2, one batched price lookup
    /// per page) and the detail endpoint. `tvl` = latest reserves × each
    /// leg's last hourly USD close (`prices.price_usd_series_1h`, ≤ ~2h
    /// stale); `null` unless both legs price (never a one-leg partial) —
    /// untracked assets and stale pools read `null`.
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

// ---------------------------------------------------------------------------
// Activity (task 0491) — the per-operation successor to `/transactions`
// ---------------------------------------------------------------------------

/// What an operation did to the pool, named by the SIGN PAIR of its two legs
/// and nothing else — `lp_operation_amounts.amount` is signed from the pool's
/// perspective, so `+/+` is a deposit, `-/-` a withdrawal and `+/-` a trade.
/// There is no operation-type column to read and no join to `operations`.
///
/// Classified in SQL rather than here, because the same expression is the
/// `filter[event]` predicate: two classifiers would eventually disagree, and
/// the one the user sees must be the one the filter used. This deliberately
/// reverses the client-side policy the retired `/transactions` shape carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PoolEvent {
    Trade,
    Deposit,
    Withdrawal,
}

impl PoolEvent {
    /// The whole classifier: the sign pair of an operation's two legs.
    ///
    /// Both amounts are signed from the pool's perspective, so a leg that
    /// entered the pool is positive. Anything that is not "both in" or "both
    /// out" moved value across the pool in opposite directions, which is a
    /// trade — including the zero-amount edge a dust swap can produce, since
    /// it is still not a deposit and not a withdrawal.
    ///
    /// Callers must only reach here with BOTH legs present; a half-row has no
    /// event (see [`PoolActivityItem::event`]).
    pub fn from_signs(amount_a: i64, amount_b: i64) -> Self {
        if amount_a > 0 && amount_b > 0 {
            Self::Deposit
        } else if amount_a < 0 && amount_b < 0 {
            Self::Withdrawal
        } else {
            Self::Trade
        }
    }

    /// Parse a `filter[event]` value.
    pub fn from_param(value: &str) -> Option<Self> {
        match value {
            "trade" => Some(Self::Trade),
            "deposit" => Some(Self::Deposit),
            "withdrawal" => Some(Self::Withdrawal),
            _ => None,
        }
    }

    /// The accepted spelling, for the `allowed` list a rejection returns.
    /// `const` so that list can be built from these three arms instead of
    /// being retyped next to the handler and drifting from the parser.
    pub const fn as_param(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::Deposit => "deposit",
            Self::Withdrawal => "withdrawal",
        }
    }
}

#[cfg(test)]
mod pool_event_tests {
    use super::PoolEvent;

    /// The classifier itself. It used to live in SQL as a `multiIf` and could
    /// only be checked against a live ClickHouse; in Rust it is the one thing
    /// this endpoint gets wrong most visibly, so it gets the table.
    #[test]
    fn sign_pair_names_the_event() {
        let cases = [
            (120, 3, PoolEvent::Deposit),
            (-4, -9, PoolEvent::Withdrawal),
            (120, -4, PoolEvent::Trade),
            (-4, 120, PoolEvent::Trade),
        ];
        for (a, b, want) in cases {
            assert_eq!(PoolEvent::from_signs(a, b), want, "({a}, {b})");
        }
    }

    /// A zero leg is not a deposit and not a withdrawal, so it falls to trade
    /// rather than to whichever branch happens to be first.
    #[test]
    fn zero_leg_is_not_a_deposit() {
        assert_eq!(PoolEvent::from_signs(0, 5), PoolEvent::Trade);
        assert_eq!(PoolEvent::from_signs(0, -5), PoolEvent::Trade);
        assert_eq!(PoolEvent::from_signs(0, 0), PoolEvent::Trade);
    }

    /// `as_param` feeds the `allowed` list a rejection returns and
    /// `from_param` reads the caller's value back, so drift between them would
    /// advertise a value the endpoint then refuses.
    #[test]
    fn filter_value_round_trips() {
        for e in [PoolEvent::Trade, PoolEvent::Deposit, PoolEvent::Withdrawal] {
            assert_eq!(PoolEvent::from_param(e.as_param()), Some(e), "{e:?}");
        }
        assert_eq!(PoolEvent::from_param("swap"), None);
        assert_eq!(PoolEvent::from_param(""), None);
    }
}

/// `filter[...]` query parameters for `GET /v1/liquidity-pools/{id}/activity`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PoolActivityParams {
    /// `trade` | `deposit` | `withdrawal`. Applied as a `HAVING` on the same
    /// expression that produces `event`, so the filtered list and the chips
    /// cannot disagree.
    ///
    /// Rows whose `event` is `null` (a leg missing — see [`PoolActivityItem`])
    /// match no filter value: we cannot claim such a row is a trade.
    ///
    /// A `String`, not a `PoolEvent`, deliberately: every `filter[…]` param in
    /// this API takes text and is validated in the handler
    /// (`ChartParams::interval`, assets' `filter[sac]`). Deserializing straight
    /// into the enum would make serde reject a bad value, and serde's rejection
    /// is axum's plain-text `QueryRejection` — not the `ErrorEnvelope` this
    /// endpoint documents, and with no `allowed` list for the caller.
    #[serde(rename = "filter[event]")]
    pub event: Option<String>,
}

/// Cursor payload for `GET /v1/liquidity-pools/{id}/activity`, keyed on
/// `(ledger_sequence, transaction_id, application_order)` — the sort-key
/// prefix of `lp_operation_amounts` minus its `asset_id` tail.
///
/// A plain struct, not an enum tagged by datasource. The retired
/// `/transactions` cursor carried `tiebreak`, which is absent here, so a
/// stale one fails to deserialize and the extractor answers `invalid_cursor`
/// on its own — no explicit source guard needed (the retired endpoint needed
/// `pool_tx_cursor_matches_source` only because both of its variants
/// deserialized cleanly).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolActivityCursor {
    pub ledger_sequence: i64,
    pub transaction_id: i64,
    pub application_order: i16,
}

/// One row from `GET /v1/liquidity-pools/{id}/activity` — **one operation
/// against this pool**, not one transaction (task 0491, issue #371).
///
/// The transaction-level fields the retired `/transactions` shape carried
/// (`fee_charged`, `operation_count`, `has_soroban`, `successful`,
/// `operation_types`) are gone: the first three describe the transaction, not
/// this row, and repeating them per operation invites reading a transaction
/// fee as an operation fee. `operation_types` is replaced by `event`, which is
/// what it was approximating.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolActivityItem {
    /// Transaction hash (64-char lowercase hex). NOT unique across rows — a
    /// transaction running several operations against this pool appears once
    /// per operation, so a row key needs `application_order` too.
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    /// The operation's 1-based position in its transaction (Horizon's
    /// `application_order`), and the `#op-N` anchor on the transaction detail
    /// page this row links to (task 0482).
    pub application_order: i16,
    /// `null` only for the malformed case where the pool's two legs did not
    /// both land in `lp_operation_amounts`. Unreachable by construction — an
    /// op that touches a pool moves both legs — but the read stays total
    /// rather than classifying a half-row.
    pub event: Option<PoolEvent>,
    /// Signed from the POOL's perspective: positive entered the pool, negative
    /// left it. Raw stroops as a decimal string, scaled by 7 at render like
    /// every other amount here — a JSON number is a double in the browser, so
    /// a leg above 2^53 stroops would silently lose digits.
    ///
    /// The sign is the payload, not decoration: it is what names `event`, so
    /// the frontend must not take an absolute value before deciding direction.
    /// `null` on both legs in the malformed case above.
    pub amount_a: Option<String>,
    pub amount_b: Option<String>,
    pub source_account: String,
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
/// strings with exactly two decimals**, computed at read from on-chain
/// quantities × the in-cluster price series (task 0199, ADR 0053):
/// - `tvl` — "TVL at close of bucket": last priceable snapshot's
///   `reserve_a·price_a + reserve_b·price_b`. A leg with no candle in its
///   own bucket falls back to its most recent close within 48 h, so a
///   pool whose second leg has not traded today still reports; `null`
///   when either leg has no price within that window (untracked asset,
///   pre-listing history, or a provider-side gap such as the
///   2026-07-21..08-03 freeze).
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
