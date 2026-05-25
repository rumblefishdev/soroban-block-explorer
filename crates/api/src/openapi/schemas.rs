//! Reusable OpenAPI schema components.
//!
//! These shapes are load-bearing: every endpoint handler that ships in
//! M2 (tasks 0043–0053) is expected to return either `ErrorEnvelope`
//! on failure or `Paginated<T>` / a plain domain object on success.
//! Their structural decisions are captured in ADR 0008.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Canonical error response body.
///
/// Intentionally simpler than RFC 7807 (`problem+json`) — see ADR 0008.
/// Machine-readable `code` drives client behaviour; `message` is a
/// human-friendly description; `details` carries optional structured
/// context (field-level validation errors, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorEnvelope {
    /// Stable machine-readable error code, e.g. `"invalid_cursor"`.
    pub code: String,
    /// Human-readable error description.
    pub message: String,
    /// Optional structured details. Shape is error-specific.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Pagination metadata attached to list responses.
///
/// Cursor-based pagination (not offset) — see ADR 0008. Aligns with
/// Stellar Horizon conventions and produces stable listings even when
/// the underlying ledger stream advances between requests.
///
/// Both `cursor` (forward / next page) and `prev_cursor` (backward /
/// previous page) are emitted as opaque base64-JSON strings.
/// `Option<String>` carries the entire signal:
///
/// - `cursor: None` ⇒ last page (no further forward continuation).
/// - `cursor: Some(_)` ⇒ a further forward page exists.
/// - `prev_cursor: None` ⇒ first page (no backward continuation).
/// - `prev_cursor: Some(_)` ⇒ a backward page exists.
///
/// Both cursors carry an internal `dir` discriminant (see
/// `crates/api/src/common/cursor.rs::Direction`) so the backend can
/// branch SQL between forward (DESC `<`) and backward (ASC `>`,
/// reverse for presentation) walks. Clients treat the strings as
/// opaque tokens — round-tripping them through `?cursor=` is the only
/// expected use.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PageInfo {
    /// Opaque cursor identifying the next page, absent when the
    /// client has reached the end of the stream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Opaque cursor identifying the previous page, absent when the
    /// response is the first page (the client has not paged forward
    /// yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_cursor: Option<String>,
    /// Page size that produced `data`. Echoes the client's requested
    /// limit (clamped server-side).
    #[schema(minimum = 1, maximum = 100)]
    pub limit: u32,
}

/// Canonical envelope for paginated list responses.
///
/// Generic over the item type `T` so every endpoint can reuse a single
/// shape. Concrete instantiations (e.g. `Paginated<Transaction>`) are
/// picked up automatically by utoipa-axum via the handler return type
/// when M2 endpoint modules are wired in. Unused in M1 — kept as
/// infrastructure that M2 endpoints will consume.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Paginated<T>
where
    T: ToSchema,
{
    pub data: Vec<T>,
    pub page: PageInfo,
}
