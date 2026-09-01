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
    /// Participant StrKey. Classic pools: always an account (`G...`).
    /// Soroban pools: a share-token holder — an account (`G...`) OR a
    /// contract (`C...`); Aquarius LPs routinely stake their LP tokens, so a
    /// staking/locker contract holding most of the supply is normal, not a
    /// data error.
    pub account: String,
    /// Pool-share balance carried as a decimal string preserving the
    /// underlying `NUMERIC(28,7)` precision (no f64 round-trip). Soroban
    /// pools: the share-token balance scaled by the token's on-chain
    /// metadata decimals.
    ///
    /// **Nullable on purpose, unlike `first_deposit_ledger` beside it.** That
    /// field's coverage rests on a fact about OUR index — the family is newer
    /// than our event floor — which is stable and verifiable. This one rests on
    /// a fact about the VENDOR's contract: that a share token publishes
    /// `decimals`. All 483 on production do, and every one says 7 — but this
    /// PR already found the other half of that lesson, where five deployments
    /// run an older pool contract that publishes no `Router` key at all. A
    /// measurement over today's instances is not a guarantee about a contract
    /// version we have not met.
    ///
    /// So an unknown scale surfaces as absent rather than as a raw integer
    /// posing as a scaled amount, and `share_percentage` — which is scale-free
    /// — still reports, so the row stays useful.
    pub shares: Option<String>,
    /// Share of the pool, expressed as a decimal-string percentage
    /// (`100 * shares / total_pool_shares`). `None` when the pool has no
    /// snapshot in the freshness window (stale pool); the frontend renders
    /// it as "—" in that case (matches the list-endpoint stale-pool
    /// convention from `18_get_liquidity_pools_list.sql`). Soroban pools:
    /// `100 * balance / sum(all positive balances)` of the share token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_percentage: Option<String>,
    /// Ledger this holder's position began. Always present, either world.
    ///
    /// Classic pools read it from `lp_positions`. Soroban pools have no such
    /// column — `balances` records current state, not a first sighting — so it
    /// comes from the share token's own `mint` / incoming `transfer` events,
    /// whichever came first.
    ///
    /// Required rather than nullable because the coverage is STRUCTURAL: every
    /// LP share token was deployed after our event floor (the family's first
    /// mint is L50,639,009 against a floor of L50,457,424), so the event that
    /// gave a current holder their tokens is always one we hold. Measured
    /// 655/655 on the busiest token, contract holders included. A holder we
    /// cannot date is treated as a defect and dropped, like one we cannot
    /// name — never served as an absent field.
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
    /// `classic` | `soroban` — restrict the union list to one pool world
    /// (task 0374). Omitted = both. Validated in the handler so a bad value
    /// gets this API's error envelope with the allowed list.
    #[serde(rename = "filter[pool_kind]")]
    pub filter_pool_kind: Option<String>,
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
///   * `asset_type == 0` — native XLM; routes to `/assets/native`, the
///     reserved token for the classic XLM singleton (it has no `code-issuer`
///     identity to compose).
///   * `contract_id` — C-strkey of the SAC mirror for a classic credit OR
///     native leg (populated when the leg's `(asset_code, issuer)`
///     classic_credit / native `assets` row carries a deployed SAC facet —
///     `sac_contract_id` resolving a `soroban_contracts.contract_id`,
///     ADR 0051). `None` for legs without a deployed SAC mirror.
///
///     Native legs were excluded from this until task 0470. The exclusion was
///     deliberate (`a19ac8f6`) and its reason was Postgres parity — PG returned
///     NULL there. Postgres is retired, and the same native `assets` row
///     already publishes that SAC as `sac_contract_id` on `/v1/assets/native`
///     and in the assets list, which the frontend renders. Withholding it here
///     alone made one asset describe itself two ways depending on the endpoint.
///     Pool legs only carry XDR `AssetType` (native /
///     credit_alphanum4 / credit_alphanum12) per `0006_liquidity_pools.sql`,
///     so SAC / Soroban legs are not directly representable here;
///     `contract_id` surfaces the SAC mirror look-up so the FE can
///     route to the asset detail page via `/assets/${contract_id}`.
///   * `issuer` + `asset_code` (classic credit, no SAC mirror) — FE
///     routes to `/assets/${asset_code}-${issuer}` (composite form
///     accepted by `parse_asset_id`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolAssetLeg {
    /// Label in the **XDR `AssetType`** vocabulary — this is a pool LEG, and
    /// leg types come from `liquidity_pools.asset_a_type`/`asset_b_type`,
    /// which store the protocol discriminant: `native` |
    /// `credit_alphanum4` | `credit_alphanum12`. `null` only on schema drift.
    /// (Task 0496 mirror: this doc used to carry the `AssetFamily` legend,
    /// declaring 2 "retired" — while 54 456 production legs carry
    /// 2 = `credit_alphanum12`.)
    pub asset_type_name: Option<String>,
    /// Raw SMALLINT, XDR `AssetType`: 0=native, 1=credit_alphanum4,
    /// 2=credit_alphanum12. Not the `assets.asset_type` family domain.
    pub asset_type: i16,
    pub asset_code: Option<String>,
    pub issuer: Option<String>,
    /// C-strkey of the deployed SAC mirror for the leg's `(asset_code, issuer)`
    /// classic_credit / native asset (ADR 0051 — resolved via the row's
    /// `sac_contract_id` facet). `None` only when no SAC is deployed for that
    /// asset. Native XLM has one, and reports it here (task 0470) exactly as
    /// `/v1/assets/native` already did.
    pub contract_id: Option<String>,
    /// Asset icon URL, resolved from `asset_enrichment` (ADR 0050) so pool
    /// avatars render the same icon as the assets list. Until task 0310 this
    /// read the dead `assets.icon_url` column, which was never populated —
    /// every leg icon came back `None`. Still `None` for assets without an
    /// enriched icon — the FE falls back to the asset-code initial.
    pub icon_url: Option<String>,
}

/// One leg of a SOROBAN pool (task 0374). Pools registered by AMM routers
/// carry 2–4 token-contract legs in emission order (`get_tokens()` order) —
/// a pair cannot represent them, so soroban pools publish `legs[]` instead
/// of `asset_a`/`asset_b`.
///
/// `family == "unresolved"` is a real state, not an error: a leg surrogate
/// that resolves through neither the SAC facet nor a bespoke-token `assets`
/// row must surface explicitly rather than as a plausible empty asset
/// (house rule: no misleading fallbacks).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolLegItem {
    /// `native` | `classic_credit` | `soroban` | `unresolved` — the
    /// `AssetFamily` of the resolved asset (task 0496), NOT the XDR
    /// `AssetType` vocabulary the classic pair legs use.
    pub family: String,
    /// Classic asset code; `null` for native, soroban and unresolved legs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_code: Option<String>,
    /// Classic issuer StrKey (`G...`); `null` outside `classic_credit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// C-strkey of the leg's token CONTRACT — the address `add_pool`
    /// registered. For a classic-family leg this is the deployed SAC; for a
    /// soroban leg the token itself. `null` only when unresolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    /// On-chain SEP-41 symbol (soroban tokens; from contract metadata).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// On-chain SEP-41 name; same sourcing as `symbol`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Display scale for `reserve`. `7` for every classic-family leg
    /// (protocol-fixed); a soroban token's from its on-chain metadata.
    /// `null` = token never published metadata — render amounts as
    /// unresolved, never with a guessed scale (an 18-decimal token shown at
    /// a default 7 is 10^11 off and looks like data, not like a bug).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u32>,
    /// Latest reserve of this leg in RAW token units, as a decimal string
    /// (scale by `decimals` to render). From the pool's latest ledger-state
    /// write (`pool_state_changes`); `null` until state is indexed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserve: Option<String>,
}

/// One pool row returned by the list endpoint. Shape pinned to canonical
/// SQL `18_get_liquidity_pools_list.sql`. Pools without a fresh snapshot
/// in the freshness window come back with `null` for every dynamic field
/// (`reserve_a`, `reserve_b`, `total_shares`, `tvl`, `volume`,
/// `fee_revenue`, `latest_snapshot_*`); frontend renders these as "stale".
///
/// Since task 0374 the list is a UNION of both pool worlds, discriminated
/// by `pool_kind`:
/// - `"classic"` — CAP-38 pools: `asset_a`/`asset_b` populated, `legs` null.
/// - `"soroban"` — AMM-contract pools: `legs[]` populated (2–4 entries),
///   `asset_a`/`asset_b` null, `protocol`/`pool_type` describe the AMM.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolItem {
    /// SEP-23 strkey (`L...`, 56 chars) for classic pools; the pool
    /// CONTRACT's `C...` strkey for soroban pools (its id bytes are a
    /// contract address payload, and rendering them as `L...` would produce
    /// a well-formed WRONG key). DB stores 32 bytes per ADR 0024.
    pub pool_id: String,
    /// `classic` | `soroban` — which world this row comes from and which
    /// leg representation it carries. See struct doc.
    pub pool_kind: String,
    /// Protocol label of the registering router, resolved at read time from
    /// the deployment (task 0374 T1): `aquarius` for the vendor-documented
    /// router. `null` for classic pools AND for router deployments whose
    /// operator is unverified — an unlabelled live router shares Aquarius's
    /// code with fully disjoint admin roles, and labelling it "aquarius"
    /// would be attribution we cannot back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    /// Verbatim pool-type symbol from the registration event
    /// (`constant` | `stable` | `concentrated` | ...). Un-normalised on
    /// purpose — three vendor vocabularies exist for one shape; folding
    /// them is read-time interpretation. `null` for classic pools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_type: Option<String>,
    /// Soroban pools only: the token legs in emission order. `null` for
    /// classic pools (use `asset_a`/`asset_b`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legs: Option<Vec<PoolLegItem>>,
    /// Classic pools only; `null` for soroban pools (use `legs`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_a: Option<PoolAssetLeg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_b: Option<PoolAssetLeg>,
    pub fee_bps: i32,
    /// `fee_bps / 100` as decimal string. Conversion done server-side so
    /// the frontend can render directly (frontend §6.13/§6.14).
    pub fee_percent: String,
    pub created_at_ledger: i64,
    /// Count of active liquidity providers (`lp_positions WHERE shares > 0`).
    /// Computed from the live table — not dependent on the snapshot
    /// freshness window, so it is populated even on stale pools (where
    /// `tvl`/`volume`/`fee_revenue` are NULL). `null` for soroban pools:
    /// their participants are share-token holders, and counting them per
    /// list row is a full `balances` scan per pool — the participants
    /// endpoint answers it per pool instead. `null` ≠ 0.
    pub participant_count: Option<i64>,
    pub latest_snapshot_ledger: Option<i64>,
    pub reserve_a: Option<String>,
    pub reserve_b: Option<String>,
    pub total_shares: Option<String>,
    /// USD, decimal string rounded to cents (task 0199 compute-at-read).
    /// Populated on **both** the list (one batched price lookup per page)
    /// and the detail endpoint, in **both pool worlds**. `tvl` = latest
    /// reserves × each leg's last hourly USD close
    /// (`prices.price_usd_series_1h`, ≤ ~2h stale); `null` unless EVERY leg
    /// prices — never a partial sum over the legs that happened to resolve —
    /// so untracked assets and stale pools read `null`.
    ///
    /// Soroban pools sum over their 2–4 `legs`, each scaled by its own
    /// `decimals`, and price SAC legs by classic identity while bespoke
    /// tokens key on `asset_kind = 'contract'`. They were `null` until
    /// review #438: the analytics path read `asset_a`/`asset_b`, which a
    /// soroban row carries as storage defaults, so every one of them showed a
    /// plotted TVL curve on its chart above an empty TVL figure.
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
    /// Soroban feed only: the event's index within its transaction — the
    /// keyset's last component there (a soroban pool can emit several flow
    /// events in one tx). `0` on classic cursors via the serde default, so
    /// pre-existing opaque cursors keep decoding.
    #[serde(default)]
    pub event_index: i64,
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
    /// page this row links to (task 0482). `null` on the soroban feed —
    /// contract events have no per-op anchor, and a `0` sentinel would both
    /// build a dangling `#op-0` link and collide row keys when one
    /// transaction emits several flow events.
    pub application_order: Option<i16>,
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
    /// Always `null` on the soroban feed (its rows are single-pool contract
    /// events by construction).
    pub pools_crossed: Option<i64>,
    /// SOROBAN pools only: per-leg movements, `leg_index` into the pool's
    /// `legs[]`. Published instead of `amount_a`/`amount_b` (a soroban pool
    /// can have 3–4 legs, and a trade touches exactly two of them by token
    /// address). Amounts are RAW token units as signed decimal strings from
    /// the POOL's perspective (positive entered the pool) — scale by the
    /// matching leg's `decimals` at render. `null` on classic rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leg_amounts: Option<Vec<PoolLegAmount>>,
    pub created_at: DateTime<Utc>,
}

/// One leg's movement inside a soroban pool event (see
/// [`PoolActivityItem::leg_amounts`]).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PoolLegAmount {
    /// Index into the pool's `legs[]` (emission order).
    pub leg_index: u32,
    /// Signed raw units from the pool's perspective, as a decimal string.
    pub amount: String,
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
    /// Echoed pool ID — 56-char StrKey (`L...` classic / `C...` soroban),
    /// same form the client supplied in the path.
    pub pool_id: String,
    pub interval: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub data_points: Vec<ChartDataPoint>,
}

#[cfg(test)]
#[path = "dto_tests.rs"]
mod tests;
