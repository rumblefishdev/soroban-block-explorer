//! Request and response DTOs for the ledgers endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::openapi::schemas::Paginated;
use crate::transactions::dto::TransactionListItem;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------
//
// Both endpoints use the project-default `crate::common::cursor::TsIdCursor`
// (`{ ts, id }`) for pagination — list orders by `(closed_at, sequence)` and
// embedded transactions order by `(created_at, id)`, both `(timestamptz, i64)`
// pairs that fit the cursor codec without a custom payload type.

/// Slim ledger row returned in the list endpoint and reused inside the
/// detail response as the header block.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LedgerListItem {
    pub sequence: i64,
    /// Ledger hash (64-char lowercase hex).
    pub hash: String,
    pub closed_at: DateTime<Utc>,
    pub protocol_version: i32,
    pub transaction_count: i32,
    pub base_fee: i64,
}

/// Detail response for `GET /v1/ledgers/:sequence`.
///
/// Header columns are the same shape as `LedgerListItem`. `prev_sequence` /
/// `next_sequence` identify the adjacent indexed ledgers in `sequence`
/// order (computed via LATERAL on the `ledgers` PK) and double as a
/// chain-position signal — when `next_sequence` is null the ledger is the
/// chain head, which drives the short-TTL Cache-Control branch in the
/// handler. `transactions` is the embedded paginated list of
/// `TransactionListItem` rows, served DB-only (memo and other heavy
/// fields belong to the transaction detail endpoint, not list rows).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LedgerDetailResponse {
    pub sequence: i64,
    /// Ledger hash (64-char lowercase hex).
    pub hash: String,
    pub closed_at: DateTime<Utc>,
    pub protocol_version: i32,
    pub transaction_count: i32,
    pub base_fee: i64,
    /// Sequence number of the ledger that closed immediately before this
    /// one. `null` at the chain tail (no earlier ledger persisted).
    pub prev_sequence: Option<i64>,
    /// Sequence number of the ledger that closed immediately after this
    /// one. `null` at the chain head (this is the most recent ledger we
    /// have indexed).
    pub next_sequence: Option<i64>,
    /// Paginated linked transactions, DB-only `TransactionListItem` rows.
    pub transactions: Paginated<TransactionListItem>,
}

// ---------------------------------------------------------------------------
// Internal query-result rows (not serialized; the handler maps these into the
// public response types above). Produced by `queries_ch`.
// ---------------------------------------------------------------------------

/// LATERAL-derived navigation pair. Kept separate from the public DTO
/// because the response type composes this with an embedded paginated
/// list (`transactions`) that does not come from a single SQL row.
#[derive(Debug)]
pub struct LedgerDetailRow {
    pub sequence: i64,
    pub hash: String,
    pub closed_at: DateTime<Utc>,
    pub protocol_version: i32,
    pub transaction_count: i32,
    pub base_fee: i64,
    pub prev_sequence: Option<i64>,
    pub next_sequence: Option<i64>,
}
