//! DTOs for XDR-sourced fields, composed with DB-sourced light slices to
//! produce final E3/E14 endpoint responses.
//!
//! Per ADR 0027 Part III + ADR 0029 + ADR 0033: the DB stores identity and
//! index columns; event, memo, signature, and envelope detail lives only on
//! the public Stellar archive. For E14 this means the entire event payload
//! (type, topics, data, event index) is S3-sourced — there is no DB-side
//! event row to merge against. E3 still carries its DB tx-light slice and
//! composes it with an XDR heavy struct via `merge_e3_response`.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// E3 (`GET /transactions/:hash`) — fields sourced from XDR parse.
///
/// Returned as the `heavy` block inside `E3Response<TransactionDetailLight>`.
/// On upstream fetch failure the caller substitutes `None` and
/// `merge_e3_response` sets `heavy_fields_status = "unavailable"`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct E3HeavyFields {
    /// Memo type as ASCII tag (`"text"`, `"id"`, `"hash"`, `"return"`, `"none"`).
    pub memo_type: Option<String>,
    /// Memo payload rendered as string (hex for hash/return, decimal for id).
    pub memo: Option<String>,
    /// Transaction-level signatures (hint + signature bytes hex-encoded).
    pub signatures: Vec<SignatureDto>,
    /// Fee-bump envelope `feeSource` StrKey when the outer tx is a fee-bump.
    pub fee_bump_source: Option<String>,
    /// Base64-encoded `TransactionEnvelope`.
    pub envelope_xdr: Option<String>,
    /// Base64-encoded `TransactionResult`.
    pub result_xdr: Option<String>,
    /// Base64-encoded `TransactionMeta` — the ledger-entry changes (who held
    /// what before/after). The 0046 spec originally withheld it; reversed by
    /// task 0460 #13: it is the one raw layer the page could not show, and
    /// the raw-data section renders every XDR blob with a Lab deep link.
    pub result_meta_xdr: Option<String>,
    /// The host-VM debug channel (`v4.diagnostic_events`): the `fn_call` /
    /// `fn_return` trace, `core_metrics` counters, and — when diagnostic mode
    /// is on, which is always for the archive — a byte-identical COPY of every
    /// consensus event above. Not hashed into consensus, and CAP-67's own
    /// event stream (`getEvents`) does not carry it at all. Presenting these
    /// alongside `contract_events` as one list shows the copies as extra
    /// events; they are one channel about the other, not a continuation of it.
    pub diagnostic_events: Vec<XdrEventDto>,
    /// The consensus event stream: `contract` + `system` events from the
    /// tx-level and per-operation containers. This — and only this — is what
    /// CAP-67 / `getEvents` mean by "the events of a transaction".
    pub contract_events: Vec<XdrEventDto>,
    /// Operations with full XDR-decoded details (type-specific JSON).
    pub operations: Vec<XdrOperationDto>,
    /// Transaction result code (e.g. `"txSuccess"`, `"txFailed"`).
    /// `None` only when the transaction had a parse error.
    pub result_code: Option<String>,
    /// Nested Soroban invocation tree, derived from `result_meta_xdr` at
    /// extraction time.
    pub operation_tree: Option<serde_json::Value>,
}

/// Single signature on a transaction envelope.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SignatureDto {
    /// 4-byte hint (lowercase hex, 8 chars).
    pub hint: String,
    /// Signature bytes (lowercase hex).
    pub signature: String,
}

/// Common shape for contract events and diagnostic events.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XdrEventDto {
    /// `"contract"`, `"system"`, or `"diagnostic"`.
    pub event_type: String,
    /// Contract address (StrKey) that emitted the event, if any.
    pub contract_id: Option<String>,
    /// Decoded topic array.
    pub topics: Vec<serde_json::Value>,
    /// Decoded event data payload.
    pub data: serde_json::Value,
    /// The stellar-rpc event id, exactly as `getEvents` returns it (ADR 0059).
    /// Consensus events arrive sorted by it, which is execution order: fee
    /// charge, operation events, fee refund. `None` for diagnostic events.
    pub id: Option<String>,
    /// Zero-based envelope position of the operation that emitted this event
    /// (CAP-67 per-operation container only; `None` for fee and diagnostic
    /// events). Matches `XdrOperationDto.application_order - 1`.
    pub operation_index: Option<i16>,
    /// The id's event number: the position in the operation, or for a fee
    /// event the ledger's (or, for `after_tx`, the transaction's) counter of
    /// that stage. `None` for diagnostic events.
    pub event_index: Option<u32>,
    /// CAP-67 `TransactionEvent.stage` — `"before_all_txs"`, `"after_tx"` or
    /// `"after_all_txs"`: when a tx-level event fired. The fee charge is
    /// `before_all_txs`; the refund is `after_tx` before protocol 23 and
    /// `after_all_txs` from it (archive meta, task 0541). `None` for
    /// per-operation and diagnostic events, which carry no stage.
    pub stage: Option<String>,
}

/// Operation raw parameters (XDR-decoded full details).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XdrOperationDto {
    /// Operation type tag (e.g. `"payment"`, `"invoke_host_function"`).
    pub op_type: String,
    /// Application order within the transaction (1-based, matches Horizon
    /// `paging_token` convention).
    pub application_order: i16,
    /// Full operation details (type-specific JSON).
    pub details: serde_json::Value,
    /// Per-operation result code from the transaction result XDR, using the
    /// XDR library's variant names: `"Success"`, `"LowReserve"`, `"Trapped"`,
    /// op-level rejections as `"OpNoAccount"` etc. Present on failed
    /// transactions too — the failing op's code is the fail reason (task
    /// 0352). `None` when the result XDR carried no per-op array
    /// (validation-level failures) or was unavailable.
    pub result_code: Option<String>,
}

/// Merged E3 response: DB light fields + optional XDR heavy fields.
///
/// `heavy_fields_status` = `Ok` when `heavy` is `Some`, `Unavailable` when
/// the public-archive fetch failed and the caller degraded gracefully.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct E3Response<TxLight> {
    #[serde(flatten)]
    #[schema(inline)]
    pub light: TxLight,
    pub heavy: Option<E3HeavyFields>,
    pub heavy_fields_status: HeavyFieldsStatus,
}

/// Indicates whether the XDR-sourced fields were loaded successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeavyFieldsStatus {
    Ok,
    Unavailable,
}
