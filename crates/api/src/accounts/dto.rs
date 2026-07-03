//! Wire shapes mirror canonical SQL `endpoint-queries/{06,07}_*.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query params for `GET /v1/accounts` (list). `filter[with_domain]` keeps
/// only accounts that set a `home_domain` (known/anchor accounts).
///
/// No `filter[q]`: account StrKeys are opaque base32 (not human-meaningful),
/// so an address prefix is a useless browse filter; exact-address lookup is
/// the global search's job (`/v1/search` redirects a full G-strkey straight
/// to the account detail page).
#[derive(Debug, Deserialize, IntoParams)]
pub struct AccountsListParams {
    /// Base sort order on `last_seen_ledger`: `asc` | `desc` (default).
    /// Sticky — re-send on every page alongside `cursor`.
    pub order: Option<String>,
    #[serde(rename = "filter[with_domain]")]
    pub filter_with_domain: Option<bool>,
}

/// Query params for `GET /v1/accounts/{account_id}/transactions`.
/// `order` controls the sort direction on `created_at` (PG) /
/// `ledger_sequence` (CH). Default is `desc` (newest-first).
#[derive(Debug, Deserialize, IntoParams)]
pub struct AccountTxListParams {
    /// Sort order on transaction time: `asc` | `desc` (default).
    /// Sticky — re-send on every page alongside `cursor`.
    pub order: Option<String>,
}

/// One row of `GET /v1/accounts`. Identity + native (XLM) balance + the
/// first/last-seen activity window + `home_domain`. Ordered by
/// `last_seen_ledger` (the only indexed sort). `xlm_balance` is the native
/// balance from the unified `balances` table; `null` if no native row exists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountListItem {
    pub account_id: String,
    /// Native (XLM) balance as a RAW `Int128` stroop string (human value =
    /// ÷10⁷); the frontend scales by 7 decimals — same raw-amount contract as
    /// the account-detail balances.
    pub xlm_balance: Option<String>,
    pub last_seen_ledger: i64,
    pub first_seen_ledger: i64,
    pub home_domain: Option<String>,
}

/// Native rows have `null` `asset_code` / `asset_issuer`; classic credit rows have
/// both; Soroban (type-3) token rows carry `contract_id` + on-chain `name`/`symbol`
/// instead (the account portfolio includes Soroban balances, task 0331 Option C).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountBalance {
    /// Horizon-style label from `asset_type_name`: `native` | `credit_alphanum4`
    /// | `credit_alphanum12` for classic. Soroban (type-3) rows are also returned
    /// — identify them via `contract_id`, not this label.
    pub asset_type_name: Option<String>,
    /// Raw SMALLINT — stable across label renames.
    #[serde(rename = "type")]
    pub asset_type: i16,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    /// C-strkey of the Soroban token's contract (type-3 only) — the identity +
    /// link target (`/assets/${contract_id}`). `null` for native / classic.
    pub contract_id: Option<String>,
    /// Asset display `name`, from two disjoint sources by asset type: classic /
    /// native → off-chain SEP-1 enrichment (`asset_enrichment`, only ~3% of classic
    /// assets carry one); Soroban (type-3) → on-chain `METADATA` (e.g. "USDC-EURC
    /// Soroswap LP Token", 100% coverage). Distinct from the `symbol` ticker.
    /// `null` when neither source has a name.
    pub name: Option<String>,
    /// On-chain token `symbol` (type-3, from `METADATA`, e.g. "SMOL") — the short
    /// ticker. `null` for native / classic (they carry `asset_code`).
    pub symbol: Option<String>,
    /// RAW integer balance as a string (`Int128`) — scale by `decimals` (task 0331
    /// Option C: one convention for all asset types; classic `decimals` = 7). The
    /// account portfolio now includes Soroban (type-3) token balances too.
    pub balance: String,
    /// Display decimals — 7 for classic, on-chain `METADATA` for Soroban tokens.
    pub decimals: u32,
    pub last_updated_ledger: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountDetailResponse {
    pub account_id: String,
    pub sequence_number: i64,
    pub balances: Vec<AccountBalance>,
    pub home_domain: Option<String>,
    pub first_seen_ledger: i64,
    pub last_seen_ledger: i64,
    /// `true` when the account was removed from the ledger via `account_merge`
    /// and never re-funded (its last lifecycle event is the merge). Derived,
    /// not stored. CH-only — the PG fallback always reports `false`.
    pub deleted: bool,
}

/// Slim — `inner_tx_hash` / `contract_ids[]` live on `/v1/transactions` only.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountTransactionItem {
    /// 64-char lowercase hex.
    pub hash: String,
    pub ledger_sequence: i64,
    /// 1-based position in ledger.
    pub application_order: i16,
    pub source_account: String,
    /// Stroops.
    pub fee_charged: i64,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub operation_types: Vec<String>,
    pub created_at: DateTime<Utc>,
}
