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
/// balance from `account_balances_current`; `null` if no native row exists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountListItem {
    pub account_id: String,
    /// Native (XLM) balance, `NUMERIC(28,7)` as a fixed-precision string.
    pub xlm_balance: Option<String>,
    pub last_seen_ledger: i64,
    pub first_seen_ledger: i64,
    pub home_domain: Option<String>,
}

/// Native rows have `null` `asset_code` / `asset_issuer`; credit rows have both.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountBalance {
    /// `native` | `credit_alphanum4` | `credit_alphanum12`.
    pub asset_type_name: Option<String>,
    /// Raw SMALLINT — stable across label renames.
    #[serde(rename = "type")]
    pub asset_type: i16,
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    /// `NUMERIC(28,7)` as fixed-precision string (preserves trailing zeros).
    pub balance: String,
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
