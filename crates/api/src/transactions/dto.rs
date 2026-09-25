//! Request and response DTOs for the transactions endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// `filter[...]` query parameters for `GET /v1/transactions`.
///
/// `limit` and `cursor` are read by a sibling `Pagination<TsIdCursor>`
/// extractor and documented via the handler's `#[utoipa::path(params(...))]`
/// attribute.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListParams {
    /// Filter by source account StrKey (G…).
    #[serde(rename = "filter[source_account]")]
    pub filter_source_account: Option<String>,
    /// Filter by contract StrKey (C…) — matches root op, nested call, or event emission.
    #[serde(rename = "filter[contract_id]")]
    pub filter_contract_id: Option<String>,
    /// Filter by operation type (e.g. `INVOKE_HOST_FUNCTION`).
    #[serde(rename = "filter[operation_type]")]
    pub filter_operation_type: Option<String>,
}

/// Opaque pagination payload for `GET /v1/transactions` and the other
/// transaction lists (encoded via [`common::cursor`](crate::common::cursor)).
///
/// Every list pages on `(ledger_sequence, <within-ledger key>)` inside one
/// partition (canonical SQL 02); the variant names the within-ledger key, so a
/// cursor carries its own keyset and a list refuses any other:
///
/// - `ChPosition` — the transaction's `application_order`, which is also its
///   execution order. `/transactions` under every filter (tasks 0541, 0372);
///   the account and asset lists (task 0575).
/// - `ChSurrogate` — the `transactions.id` hash surrogate. The
///   contract-invocation list, until task 0538 moves it to the position.
///
/// The `src` tag makes the cursor self-describing. Per ADR 0008 the wire
/// format is opaque to clients, so the backend may change the encoding
/// freely; the flip side is that a cursor which decodes but anchors another
/// list's keyset MUST be rejected with `invalid_cursor` rather than silently
/// mis-paginating. A legacy/untagged cursor (pre-0243, no `src`) fails to
/// decode at all. Both fields are non-optional, so a keyset never binds a
/// NULL tuple element.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "src", rename_all = "snake_case")]
pub enum TxListCursor {
    ChSurrogate {
        ledger_sequence: i64,
        transaction_id: i64,
    },
    ChPosition {
        ledger_sequence: i64,
        application_order: i16,
    },
}

impl TxListCursor {
    /// Does this cursor anchor the keyset of `/transactions`? Every statement
    /// keys on the position since task 0372; a surrogate cursor minted by the
    /// operation-type filter before it answers `invalid_cursor` once.
    pub fn fits_transaction_list(&self) -> bool {
        matches!(self, TxListCursor::ChPosition { .. })
    }
}

/// Slim transaction row returned in the list endpoint.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionListItem {
    /// Transaction hash (64-char lowercase hex).
    pub hash: String,
    pub ledger_sequence: i64,
    /// 1-based position of this transaction within its ledger.
    pub application_order: i16,
    /// `null` for Variant A `parse_error` transactions whose envelope
    /// could not be decoded (lore-0209). Always populated for ordinary
    /// (successful or failed-but-decoded) transactions.
    pub source_account: Option<String>,
    /// Fee charged, in raw stroops. Native (XLM) is always 7 decimals, so
    /// there is no `decimals` field — the frontend scales by 1e7.
    pub fee_charged: i64,
    /// Inner-transaction hash (64-char hex) for fee-bump envelopes, `null` otherwise.
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    /// `true` when the transaction touched at least one Soroban contract
    /// (root invocation, nested call, or event emission).
    pub has_soroban: bool,
    /// All distinct operation type names in the transaction
    /// (e.g. `["INVOKE_HOST_FUNCTION", "PAYMENT"]`).
    pub operation_types: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// `memo_type` / `memo` are NOT exposed on the list item by design — list
// endpoints stay DB-only. Memo lives on the transaction detail endpoint
// (`GET /v1/transactions/{hash}`) inside the E3 `heavy` block, which
// already pays for the archive XDR fetch for the full transaction view.
// Adding memo here would require an archive fetch per ledger touched by
// the page, which is wasteful for the list use case and inconsistent
// with the DB-only contract advertised by canonical SQL 02.

/// DB-sourced light slice for the transaction detail endpoint.
///
/// Composed with `E3HeavyFields` via `merge_e3_response` (task 0150). All
/// XDR-sourced fields (memo, result_code, signatures, events, operation
/// details, envelope_xdr/result_xdr, operation_tree) live in `heavy`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TransactionDetailLight {
    /// Transaction hash (64-char lowercase hex).
    pub hash: String,
    pub ledger_sequence: i64,
    /// 1-based position of this transaction within its ledger.
    pub application_order: i16,
    /// `null` for Variant A `parse_error` transactions whose envelope
    /// could not be decoded (lore-0209).
    pub source_account: Option<String>,
    /// The source account's on-chain `home_domain` (SEP-1 anchor domain),
    /// `null` when it sets none. The frontend asks that domain for the SEP-2
    /// federated address it claims for this account (task 0443, issue #363).
    pub source_account_home_domain: Option<String>,
    /// Fee charged, in raw stroops. Native (XLM) is always 7 decimals, so
    /// there is no `decimals` field — the frontend scales by 1e7.
    pub fee_charged: i64,
    /// Inner-transaction hash (64-char hex) for fee-bump envelopes, `null` otherwise.
    pub inner_tx_hash: Option<String>,
    pub successful: bool,
    pub operation_count: i16,
    pub has_soroban: bool,
    pub created_at: DateTime<Utc>,
    /// `true` when the XDR parser encountered an error for this transaction.
    pub parse_error: bool,
    pub operations: Vec<OperationItem>,
    /// Accounts touched by this transaction. Populated only when
    /// `heavy_fields_status = "unavailable"`; otherwise `[]` and consumers
    /// should rely on the heavy block.
    pub participants: Vec<String>,
    /// Soroban event appearance index rows. Same fallback semantics as
    /// `participants`. Full topics + data live in `heavy.contract_events`.
    pub soroban_events: Vec<EventAppearanceItem>,
    /// Soroban invocation appearance index rows. Same fallback semantics
    /// as `participants`. Full call hierarchy lives in `heavy.operation_tree`.
    pub soroban_invocations: Vec<InvocationAppearanceItem>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct EventAppearanceItem {
    pub contract_id: String,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InvocationAppearanceItem {
    pub contract_id: String,
    /// Root caller G-StrKey. Per ADR 0034 nested-call hierarchy is XDR-only.
    pub caller_account: Option<String>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OperationItem {
    /// Equal to `application_order` (the table has no surrogate id since
    /// PR #175). Use `application_order` for apply-order display and to join
    /// against `XdrOperationDto.application_order` from the heavy overlay.
    pub appearance_id: i64,
    /// Operation type tag in canonical SCREAMING_SNAKE_CASE
    /// (e.g. `"INVOKE_HOST_FUNCTION"`).
    pub type_name: String,
    /// Raw `OperationType` SMALLINT (ADR 0031).
    #[serde(rename = "type")]
    pub op_type: i16,
    pub source_account: Option<String>,
    pub destination_account: Option<String>,
    pub contract_id: Option<String>,
    /// Asset code (≤12 chars) for classic asset operations.
    pub asset_code: Option<String>,
    pub asset_issuer: Option<String>,
    /// Liquidity pools crossed by this operation, as SEP-23 strkeys
    /// (`L...`, 56 chars). Encoded from the DB hex form at the response
    /// boundary so cross-entity link targets match the
    /// `/v1/liquidity-pools/:id` route shape. Single-element for LP
    /// deposit/withdraw; the full crossed-pool list for path payments and
    /// offers that filled against a pool (task 0261/0268 — replaces the
    /// former nullable scalar `pool_id`).
    ///
    /// Empty `[]` means "no pool crossed" — authoritative on the ClickHouse
    /// read path, which extracts pool crossings from claim atoms across
    /// path-payment, offer, and LP deposit/withdraw ops.
    pub pool_ids: Vec<String>,
    /// 1-based per-tx apply position carrying on-chain operation order
    /// (task 0192). For folded appearance rows (multiple identical-identity
    /// envelope ops collapsed into one row, see task 0163) this is the
    /// MIN of the folded ops' indices — the position of the row's first
    /// occurrence in `tx.operations[]`. `None` for pre-task-0192 rows
    /// where the column was not yet populated; clients fall back to
    /// `appearance_id` order in that case.
    pub application_order: Option<i16>,
    pub ledger_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests;
