//! Request and response DTOs for the assets endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// `filter[...]` query parameters for `GET /v1/assets`.
///
/// `limit` / `cursor` are read by a sibling `Pagination<AssetIdCursor>`
/// extractor and are intentionally absent here.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    /// `native | classic_credit | soroban` (ADR 0051 — `sac` is no longer a
    /// type; use `filter[sac]` for the SAC view).
    #[serde(rename = "filter[type]")]
    pub filter_type: Option<String>,
    /// Substring match against `asset_code`; SQL wraps in `%...%`.
    /// Caller MUST NOT pass `%` / `_` literals.
    #[serde(rename = "filter[code]")]
    pub filter_code: Option<String>,
    /// SAC property filter (ADR 0051): `true` restricts the list to assets whose
    /// Stellar Asset Contract is deployed on-chain (`sac_deployed`) — the old
    /// `filter[type]=sac` view, now a facet predicate over classic_credit /
    /// native rows. Reserved (un-deployed) SAC addresses are excluded. Any other
    /// value is ignored.
    #[serde(rename = "filter[sac]")]
    pub filter_sac: Option<String>,
}

/// Asset row returned by list and detail. Surfaces both the decoded
/// `asset_type_name` (SQL `token_asset_type_name()`) and the raw `asset_type`
/// SMALLINT — canonical SQL `08_get_assets_list.sql` projection.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssetItem {
    /// Canonical identifier — the single token usable as `/assets/{id}`:
    /// the reserved `native` token for native XLM, the contract StrKey
    /// (`C…`) for contract-backed assets (SAC / Soroban), otherwise the
    /// `CODE-ISSUER` composite (classic credit, e.g. `USDC-GA…`). Replaces
    /// the dropped numeric surrogate (PR #175 / the PG→CH composite move):
    /// CH keys assets on `(asset_type, asset_code, issuer_id, contract_id)`,
    /// with no surrogate.
    pub id: String,
    /// `native | classic_credit | soroban` (ADR 0051 — `sac` retired). `null`
    /// only on schema drift.
    pub asset_type_name: Option<String>,
    /// Raw SMALLINT (0=native, 1=classic_credit, 3=soroban). 2 (`sac`) is retired.
    pub asset_type: i16,
    pub asset_code: Option<String>,
    pub issuer: Option<String>,
    /// Soroban contract StrKey (`C…`) — set ONLY for `soroban` (type=3), where
    /// the contract IS the asset. `null` for native / classic_credit (a wrapping
    /// SAC's address rides in `sac_contract_id`).
    pub contract_id: Option<String>,
    /// SAC facet (ADR 0051): the wrapping Stellar Asset Contract's `C…` StrKey
    /// for a native / classic_credit asset that has one, re-derived on read from
    /// `code:issuer` (never stored). `null` when the asset has no observed SAC.
    pub sac_contract_id: Option<String>,
    /// Whether `sac_contract_id`'s SAC is deployed on-chain (ADR 0051). `null`
    /// when the asset has no SAC; `false` = reserved-but-un-deployed address
    /// (render the contract link non-clickable).
    pub sac_deployed: Option<bool>,
    pub name: Option<String>,
    /// On-chain SEP-41 token symbol (Soroban `METADATA`). `null` for classic
    /// (use `asset_code`) and native.
    pub symbol: Option<String>,
    /// Display decimals — on-chain `METADATA` for Soroban tokens, else 7
    /// (Stellar classic precision). Load-bearing for amount rendering.
    pub decimals: u32,
    /// Total supply as a RAW integer string (`Int128`) — scale by `decimals` for
    /// display (task 0331 Option C: one convention for ALL asset types; classic
    /// `decimals` is 7). E.g. `"63836094715548"`. `null` = no balance data
    /// (a token/asset with no holders). Sourced from `balance_aggregates` over the
    /// unified `balances` table.
    pub total_supply: Option<String>,
    /// Active-holder count (`amount > 0`) from the unified `balances` aggregate
    /// (all asset types — trustline holders + contract holders). `null` = no data.
    pub holder_count: Option<i32>,
    pub icon_url: Option<String>,
}

/// Detail response. `description` is populated from the issuer stellar.toml
/// `CURRENCIES[].desc` field; `home_page` is populated from
/// `DOCUMENTATION.ORG_URL` (SEP-1 has no per-currency `home_page` field —
/// the org URL is the closest semantic match and preserves backward
/// compatibility with the previous DB-sourced column). Both default to
/// `null` for native XLM, assets without an issuer, issuers without an
/// on-chain `home_domain`, fetch failures, and stellar.toml files with
/// no matching `[[CURRENCIES]]` entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssetDetailResponse {
    #[serde(flatten)]
    #[schema(inline)]
    pub item: AssetItem,
    /// `soroban_contracts.deployed_at_ledger` — `null` for classic / native.
    pub deployed_at_ledger: Option<i64>,
    pub description: Option<String>,
    pub home_page: Option<String>,
}

/// Transaction row for `/assets/:id/transactions`. Pure-DB; mirrors
/// canonical SQL `10_get_assets_transactions.sql`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssetTransactionItem {
    pub hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    pub successful: bool,
    pub fee_charged: i64,
    pub created_at: DateTime<Utc>,
    pub operation_count: i16,
    pub has_soroban: bool,
    /// Distinct `op_type_name(...)` labels for every op in the tx, sorted asc.
    pub operation_types: Vec<String>,
}

// ---------------------------------------------------------------------------
// Internal query types + helpers (not serialized; produced/consumed by
// queries_ch and the handler). Relocated from the deleted PG queries.rs
// (task 0244).
// ---------------------------------------------------------------------------

/// Detail/list row for an asset. Handler maps this to the wire `AssetItem`.
#[derive(Debug, Clone)]
pub struct AssetRow {
    pub asset_type: i16,
    /// Pre-decoded via `token_asset_type_name()` SQL helper. `None` only
    /// when the discriminant is outside the schema CHECK range — defensive
    /// against future schema drift.
    pub asset_type_name: Option<String>,
    pub asset_code: Option<String>,
    /// Already resolved through `accounts.account_id` join.
    pub issuer: Option<String>,
    /// Already resolved through `soroban_contracts.contract_id` join.
    pub contract_id: Option<String>,
    pub name: Option<String>,
    /// On-chain SEP-41 token symbol from `soroban_contract_metadata` (task 0297);
    /// `None` for classic/native.
    pub symbol: Option<String>,
    /// Display decimals — on-chain `METADATA` for Soroban tokens, else 7
    /// (Stellar classic precision).
    pub decimals: u32,
    pub total_supply: Option<String>,
    pub holder_count: Option<i32>,
    pub icon_url: Option<String>,
    /// `soroban_contracts.deployed_at_ledger` — populated for SAC and
    /// Soroban-native rows; `None` for native and classic_credit.
    pub deployed_at_ledger: Option<i64>,
    /// `accounts.home_domain` for the issuer, used as the SEP-1 lookup
    /// key in `get_asset` runtime enrichment (task 0188). `None` for
    /// native, no-issuer, and issuer accounts that did not set
    /// `home_domain` on-chain.
    pub issuer_home_domain: Option<String>,
    /// Surrogate key columns — cursor keyset only, never on the wire. These
    /// are the 4-tuple CH orders `assets` by `(asset_type, asset_code,
    /// issuer_id, contract_id)`; `0` / `''` stand in for "absent" (native has
    /// no issuer_id, classic-credit has no contract_id), matching CH defaults.
    pub issuer_id: i64,
    pub contract_surrogate_id: i64,
    /// SAC facet (ADR 0051): the surrogate of the wrapping SAC's `C…` StrKey,
    /// or `0` when the asset has no observed SAC. Never on the wire — the
    /// handler re-derives the display StrKey from `code:issuer` when non-zero.
    pub sac_contract_surrogate: i64,
    /// Whether the `sac_contract_surrogate` SAC is deployed on-chain (ADR 0051).
    pub sac_deployed: bool,
}

#[derive(Debug)]
pub struct AssetTxRow {
    pub id: i64,
    pub hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    pub successful: bool,
    pub fee_charged: i64,
    pub created_at: DateTime<Utc>,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
}

/// Pagination payload for `GET /v1/assets`. The keyset walks the natural
/// identity 4-tuple `(asset_type, asset_code, issuer_id, contract_id)` — the
/// exact `ORDER BY` of the CH `assets` table. `issuer_id` / `contract_id` are
/// the surrogate key columns (`0` = absent); `asset_code` is `''` for native.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetKeyCursor {
    pub asset_type: i16,
    pub asset_code: String,
    pub issuer_id: i64,
    pub contract_id: i64,
}

/// Resolved, validated `GET /v1/assets` list params handed to `fetch_list`.
pub struct ResolvedListParams {
    pub limit: i64,
    pub cursor: Option<AssetKeyCursor>,
    pub asset_type: Option<i16>,
    /// Raw substring (no `%` / `_` from the caller). The SQL builder
    /// wraps it in `%...%` for the trigram match.
    pub asset_code: Option<String>,
    /// SAC property filter (ADR 0051): restrict to assets with a SAC
    /// (`sac_contract_id != 0`) — the old `filter[type]=sac` view.
    pub sac_only: bool,
}

/// Resolved asset identity used to gate the `/transactions` sub-resource query.
pub struct AssetIdentity<'a> {
    pub asset_code: Option<&'a str>,
    pub issuer: Option<&'a str>,
    pub contract_id: Option<&'a str>,
}

/// True when the asset has a DB-side identity ops can key on. Native XLM and
/// friends have none → the handler short-circuits with an empty page rather
/// than emit a degenerate `WHERE ()`.
pub fn asset_predicate_present(identity: &AssetIdentity<'_>) -> bool {
    let has_classic = identity.asset_code.is_some() && identity.issuer.is_some();
    let has_contract = identity.contract_id.is_some();
    has_classic || has_contract
}
