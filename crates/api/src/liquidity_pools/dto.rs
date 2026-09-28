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
    /// (`100 * shares / total_pool_shares`, over the pool's latest snapshot
    /// however old — a classic pool snapshots every change). `None` when the
    /// pool has no snapshot or its total is 0; the frontend renders "—".
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
/// Two filters, on two different axes, combining additively:
///   * **`filter[asset_code]`** — free text, case-insensitive **substring**
///     match against any leg. Convenience for the list filter (frontend
///     §6.13) where the user types just `USDC` / `XLM` — or `USD`, which
///     matches every `USDC` pool, or `USDC/XLM` for a pair.
///   * **`filter[pool_kind]`** — which protocol built the pool.
///
/// `limit` / `cursor` are read by a sibling `Pagination<PoolListCursor>`
/// extractor.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PoolListParams {
    /// Free-text asset filter — case-insensitive substring of any leg's
    /// displayed code (input is trimmed before the query). The needle is
    /// matched literally: `%`, `_` and regex metacharacters have no special
    /// meaning.
    ///
    /// A `/` makes it a **pair** query: `USDC/XLM` requires both codes to be
    /// present on TWO DIFFERENT legs, and the typed order does not matter.
    /// Only the first `/` splits, so `USDC/XLM/BTC` searches for the literal
    /// second code `XLM/BTC` and therefore matches nothing.
    ///
    /// Native legs match on `XLM` even though they store an empty code, so
    /// `XLM` returns the pools that actually hold native XLM. Note that it
    /// *also* returns credit assets minted under the code `XLM` — asset codes
    /// are not unique on Stellar, and this filter matches codes, not asset
    /// identity.
    ///
    /// A pool IDENTIFIER is also accepted here — a classic pool's `L…` SEP-23
    /// StrKey or a soroban pool's `C…` contract address — and selects that
    /// single pool
    /// instead of matching asset codes (task 0470). Previously an identifier
    /// was matched as a substring of an asset code, found nothing, and the
    /// list answered "no pools" about a pool that exists.
    #[serde(rename = "filter[asset_code]")]
    pub filter_asset_code: Option<String>,
    /// `classic` | `soroban` — which protocol built the pool.
    ///
    /// Replaces the four per-leg POSITIONAL filters
    /// (`filter[asset_a_code]` + issuer, and the same for `b`). Those named a
    /// leg by its position in a pair, which a list of two-to-four legs has no
    /// equivalent for; `filter[asset_code]` answers the same question without
    /// pinning a position. No client used them — the frontend's only pool
    /// filter is the free-text code box.
    ///
    /// An unknown value is rejected with 400 rather than ignored: a silently
    /// dropped filter returns a page that contradicts what was asked for.
    #[serde(rename = "filter[pool_kind]")]
    pub filter_pool_kind: Option<String>,
    /// Minimum TVL threshold as a decimal string (matches the underlying
    /// `NUMERIC(28,7)` column without an f64 round-trip).
    #[serde(rename = "filter[min_tvl]")]
    pub filter_min_tvl: Option<String>,
}

/// One leg of a pool.
///
/// An element of [`PoolItem::legs`], never half of a pair: a classic pool has
/// exactly two legs, a soroban pool has two to four (three- and four-leg stable
/// pools exist on mainnet).
///
/// # Which vocabulary the kind speaks
///
/// `asset_type_name` speaks the asset **FAMILY** domain — `native` |
/// `classic_credit` | `soroban` — the same three labels `/v1/assets` emits and
/// the frontend already maps. It used to speak the XDR `AssetType` domain here
/// (`credit_alphanum4` / `credit_alphanum12` / `pool_share`) while the sibling
/// endpoint spoke the family one: one field name, two vocabularies, coinciding
/// on the single word `native`. The rule it now follows is the one task 0496
/// paid for in production — a renderer may only use the vocabulary of the enum
/// its value came from — and the value is produced by
/// [`domain::AssetFamily::as_str`] rather than by a local match, so there is no
/// copy to drift.
///
/// The raw discriminant is deliberately NOT published beside the label. It was,
/// and it carried nothing the label does not (the label is a pure function of
/// it) while inviting exactly the mismatch above. The alphanum4/alphanum12
/// width it distinguished is rendered nowhere in the app, and is recoverable
/// from the code's length if it is ever wanted. It also has no honest value for
/// a soroban token: XDR has no slot for one, and its `3` already means
/// `pool_share`.
///
/// # Linkable identifiers
///
/// Every link target is the asset detail page. The precedence is not a
/// preference — each rung is the only form that resolves for its kind:
///
///   * native → `/assets/native`, the reserved token for the XLM singleton,
///     which has no `code-issuer` identity to compose.
///   * `asset_code` + `issuer` → `/assets/{code}-{issuer}`.
///   * `contract_id` → `/assets/{contract_id}`, for a soroban token, whose
///     contract IS its asset identity.
///
/// A classic or native leg's SAC mirror is not published here. It is not a
/// link target — the asset endpoint pins a contract lookup to the soroban
/// family, so a SAC address resolves nothing (measured 2026-08-13, after an
/// earlier contract-first order sent ~93k classic legs to a dead page) — and
/// no pool surface renders it. The asset's own page (`/v1/assets`) carries it
/// with its deployment state.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolAssetLeg {
    /// `native` | `classic_credit` | `soroban`. `None` for a leg whose token
    /// the registry has no row for (one live pool has one) — there is no
    /// family to name.
    pub asset_type_name: Option<String>,
    pub asset_code: Option<String>,
    pub issuer: Option<String>,
    /// The token's own contract, set ONLY for a `soroban` leg. `None` for
    /// native and classic credit.
    pub contract_id: Option<String>,
    /// The token's self-declared SEP-41 symbol, from contract metadata — what
    /// names a soroban leg that has no classic code. Not unique: many
    /// contracts claim the same one, so `contract_id` stays the identity.
    pub symbol: Option<String>,
    /// Asset icon URL from `asset_enrichment` (ADR 0050), so pool avatars match
    /// the assets list. NOT from `assets`, whose `icon_url` column was dropped
    /// in task 0310 after measuring 0 of 411,654 rows populated. `None` for an
    /// asset with no enriched icon — the frontend falls back to the initial.
    pub icon_url: Option<String>,
    /// What the pool holds of this leg, in the asset's own units, as a decimal
    /// string (a JSON number is a browser double and a big reserve would lose
    /// digits). On the leg, not as a `reserve_a` / `reserve_b` pair, because a
    /// pool has two to four legs. `null` when no source knows it — never `0`.
    /// A classic pool reads its latest snapshot. A soroban pool reads its
    /// newest `pool_state_changes` row, served for a native or classic leg
    /// (7 decimals by protocol); a soroban-token leg stays `null` until its
    /// scale is read from the token's metadata.
    pub reserve: Option<String>,
}

/// One pool row returned by the list endpoint. Shape pinned to canonical
/// SQL `18_get_liquidity_pools_list.sql`. Pools without a fresh snapshot
/// in the freshness window come back with `null` for every dynamic field
/// (each leg's `reserve`, `total_shares`, `tvl`, `volume`, `fee_revenue`,
/// `latest_snapshot_*`); frontend renders these as "stale".
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolItem {
    /// A classic pool's SEP-23 strkey (`L…`) or a soroban pool's contract
    /// address (`C…`), 56 chars. DB stores the same 32 bytes for both (ADR
    /// 0024); the handler encodes them by kind at the response boundary.
    pub pool_id: String,
    /// `classic` | `soroban` ([`domain::PoolKind`]). Load-bearing, not
    /// decoration: the same 32 bytes render as a SEP-23 `L…` strkey for a
    /// classic pool and a `C…` contract address for a soroban one, and
    /// rendering one as the other yields a well-formed WRONG key rather than
    /// an error.
    pub pool_kind: domain::PoolKind,
    /// The pool's legs in registration order — two for a classic pool, two to
    /// four for a soroban one. Replaces the `asset_a` / `asset_b` pair, which
    /// could not express a three-leg pool and forced a soroban row to write
    /// placeholder values that read as native XLM downstream.
    pub legs: Vec<PoolAssetLeg>,
    pub fee_bps: i32,
    /// `fee_bps / 100` as decimal string. Conversion done server-side so
    /// the frontend can render directly (frontend §6.13/§6.14).
    pub fee_percent: String,
    pub created_at_ledger: i64,
    /// Count of active liquidity providers (`lp_positions WHERE shares > 0`).
    /// Computed from the live table — not dependent on the snapshot
    /// freshness window, so it is populated even on stale pools (where
    /// `tvl`/`volume`/`fee_revenue` are NULL).
    ///
    /// `null` for a soroban pool: its providers are the holders of its share
    /// token, which `lp_positions` does not index, so a `0` there would claim
    /// an empty pool rather than an unread one.
    pub participant_count: Option<i64>,
    pub latest_snapshot_ledger: Option<i64>,
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
/// and nothing else — `pool_operation_amounts.amount` is signed from the pool's
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
    /// The whole classifier: the signs of an operation's leg amounts.
    ///
    /// Every amount is signed from the pool's perspective, so a leg that
    /// entered the pool is positive. Anything that is not "all in" or "all
    /// out" moved value across the pool in opposite directions, which is a
    /// trade — including the zero-amount edge a dust swap can produce, since
    /// it is still not a deposit and not a withdrawal.
    ///
    /// Callers must only reach here with EVERY leg present; a half-row has no
    /// event (see [`PoolActivityItem::event`]).
    pub fn from_signs(amounts: &[i64]) -> Self {
        if amounts.iter().all(|&a| a > 0) {
            Self::Deposit
        } else if amounts.iter().all(|&a| a < 0) {
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
mod pool_event_tests;

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
/// `(ledger_sequence, application_order, operation_index)` — the sort-key
/// prefix of `pool_operation_amounts` minus its `asset_id` tail: the
/// transaction's position and the operation's 0-based index (ADR 0059).
///
/// A plain struct, not an enum tagged by datasource. A stale cursor fails to
/// deserialize and the extractor answers `invalid_cursor` on its own: the
/// retired `/transactions` one carried `tiebreak`, and the one keyed on
/// `transaction_id` before task 0372 lacks `operation_index`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolActivityCursor {
    pub ledger_sequence: i64,
    pub application_order: i16,
    pub operation_index: i16,
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
    /// `null` when not every leg of the pool landed in `pool_operation_amounts`
    /// for this operation. Rare but real: 350 of 6.09M operations in the
    /// 100k ledgers to 64,576,995 carry one leg only. The read stays total
    /// rather than classifying a half-row.
    pub event: Option<PoolEvent>,
    /// One amount per leg, in the order of the pool's `legs`: `amounts[i]` is
    /// what moved in `legs[i]`. A list, not an `a` / `b` pair, for the same
    /// reason the pool's legs are one: a Soroban pool has two to four.
    ///
    /// Signed from the POOL's perspective: positive entered the pool, negative
    /// left it. Raw units as a decimal string — a JSON number is a double in
    /// the browser, so an amount above 2^53 would silently lose digits.
    ///
    /// The sign is the payload, not decoration: it is what names `event`, so
    /// the frontend must not take an absolute value before deciding direction.
    /// Every entry is `null` in the malformed case above.
    pub amounts: Vec<Option<String>>,
    /// Who performed THIS OPERATION — the operation's own source account when
    /// it declares one, otherwise the transaction's, which is what an absent
    /// `Operation.sourceAccount` means in the XDR.
    ///
    /// Not simply the transaction's source: on a per-operation row that names
    /// the wrong account whenever the two differ, which on prod is 41% of
    /// operations in a recent ledger window. The retired `/transactions` shape
    /// could only ever carry the transaction's, since its row WAS one.
    pub source_account: String,
    /// How many pools the WHOLE operation crossed — `length(pool_ids)` from
    /// the same appearance seek that resolves the source account. `1` for
    /// every deposit/withdrawal (an LP op declares exactly one pool) and for
    /// a single-hop trade; `> 1` marks this row as one hop of a longer path
    /// payment, whose full route lives on the op's detail page. `null` only
    /// when the appearance row is missing — unknown, never guessed to `1`.
    pub pools_crossed: Option<i64>,
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
    /// Echoed pool ID — `L…` or `C…`, same form the client supplied in the
    /// path.
    pub pool_id: String,
    pub interval: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub data_points: Vec<ChartDataPoint>,
}
