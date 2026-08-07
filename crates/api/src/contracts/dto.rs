//! Request and response DTOs for the contracts endpoints.
//! Wire shapes mirror canonical SQL `endpoint-queries-clickhouse/{11..14}_*.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Query params for `GET /v1/contracts` (list). Mirrors the assets-list
/// filter shape: `filter[type]` (contract class) + `filter[q]` (search by
/// contract id or name, full-text via `search_vector`).
#[derive(Debug, Deserialize, IntoParams)]
pub struct ContractsListParams {
    /// Contract class: `token | other | nft | fungible`.
    #[serde(rename = "filter[type]")]
    pub filter_type: Option<String>,
    /// Free-text search over contract id + name (`search_vector`).
    #[serde(rename = "filter[q]")]
    pub filter_q: Option<String>,
}

/// One row of `GET /v1/contracts`. Identity + classification + deploy
/// provenance + a 7-day activity signal. All fields come straight from
/// `soroban_contracts` (+ a deployer join and a windowed invocation count);
/// nullable fields are `None` until the contract's deploy is observed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContractListItem {
    pub contract_id: String,
    /// Raw SMALLINT class (0=token, 1=other, 2=nft, 3=fungible). `null`
    /// until deployment is observed.
    pub contract_type: Option<i16>,
    /// `token | other | nft | fungible`. `null` only on schema drift / no type.
    pub contract_type_name: Option<String>,
    /// Stellar Asset Contract flag (stored, not derived from `contract_type`).
    pub is_sac: bool,
    /// Deployer account G-strkey; `null` until the deploy op is observed.
    pub deployer: Option<String>,
    /// Ledger the deploy was observed at; `null` until then.
    pub deployed_at_ledger: Option<i64>,
    /// Invocation count over the last 7 days (windowed; matches the detail
    /// `ContractStats.recent_invocations` semantics).
    pub recent_invocations: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContractStats {
    pub recent_invocations: i64,
    pub recent_unique_callers: i64,
    /// Event count in the same window as `recent_invocations` (NOT the full
    /// `/events` history — that endpoint pages all events with no time bound).
    /// `count()`s the `soroban_events` rows (one row per event) written from the
    /// parser event stream (diagnostics dropped at parse; System + Contract kept).
    pub recent_events: i64,
    /// Echoed window label (e.g. `"7 days"`) so the UI can label "last N days".
    pub stats_window: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContractDetailResponse {
    pub contract_id: String,
    pub wasm_hash: Option<String>,
    pub wasm_uploaded_at_ledger: Option<i64>,
    pub deployer: Option<String>,
    pub deployed_at_ledger: Option<i64>,
    pub contract_type_name: Option<String>,
    pub contract_type: Option<i16>,
    pub is_sac: bool,
    /// Task 0327: contract mutability, 3-state.
    /// - `Some(true)` → **Upgradeable**: the current WASM imports
    ///   `update_current_contract_wasm` (a self-upgrade path).
    /// - `Some(false)` → **Immutable/frozen**: it cannot upgrade itself (a SAC
    ///   has no WASM and is always `Some(false)`).
    /// - `None` → **Unknown**: the WASM interface hasn't been parsed yet (stub /
    ///   pre-0327 row) — the frontend renders no chip.
    ///
    /// Derived from the WASM at parse time
    /// (`wasm_interface_metadata.metadata.upgradeable`), not from a ledger flag
    /// (none exists).
    pub upgradeable: Option<bool>,
    pub stats: ContractStats,
}

/// One parameter on a Soroban contract function signature.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContractFunctionParam {
    pub name: String,
    pub type_name: String,
}

/// A single public function signature extracted from a Soroban contract's
/// WASM spec. Mirror of `xdr_parser::types::ContractFunction`, which is
/// the indexer-side source of the persisted shape.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContractFunctionSig {
    pub name: String,
    /// Documentation string; may be empty.
    pub doc: String,
    pub inputs: Vec<ContractFunctionParam>,
    /// Output type names; empty array == void return.
    pub outputs: Vec<String>,
}

/// Soroban contract interface metadata persisted in
/// `wasm_interface_metadata.metadata` (JSONB). Field shape mirrors the
/// indexer's `xdr_parser::types::ContractInterface` exactly — the API
/// hands the same JSON object to clients that the indexer wrote.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ContractInterfaceMetadata {
    pub functions: Vec<ContractFunctionSig>,
    /// Raw WASM byte length (informational).
    pub wasm_byte_len: i64,
}

/// `interface_metadata` is `null` for SAC / pre-upload / stub rows;
/// stubs (task 0153) are filtered at the SQL layer so they don't leak.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InterfaceResponse {
    pub contract_id: String,
    pub wasm_hash: Option<String>,
    pub interface_metadata: Option<ContractInterfaceMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InvocationItem {
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    pub caller_account: Option<String>,
    pub created_at: DateTime<Utc>,
    pub successful: bool,
}

/// One row per event — the full-content `soroban_events` table stores one
/// row per event (no appearance-fold expansion).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventItem {
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    pub transaction_id: i64,
    pub successful: bool,
    pub created_at: DateTime<Utc>,
    pub event_type: String,
    pub topics: Vec<serde_json::Value>,
    pub data: serde_json::Value,
}

/// Opaque pagination cursor for `GET /contracts/:id/events`, datasource-tagged
/// (ADR 0008) so a cursor minted for one backend is rejected after a flag flip;
/// a legacy/untagged cursor (no `src`) fails to decode → clean 400.
///
/// Keyset `(ledger_sequence, transaction_id, event_index)` over the
/// full-content `soroban_events` table (per-event rows; `event_index` is the
/// multi-event-tx tie-break, non-optional so a keyset never binds a NULL
/// tuple element).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "src", rename_all = "snake_case")]
pub enum EventCursor {
    Ch {
        ledger_sequence: i64,
        transaction_id: i64,
        event_index: i16,
    },
}
/// Pagination payload for `GET /v1/contracts`. `soroban_contracts` is
/// unpartitioned with no `created_at`, so the natural order is `id DESC`.
/// Serialized into the opaque wire cursor (ADR 0008), so it lives on the DTO
/// boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractIdCursor {
    pub id: i64,
}

/// Query params for `GET /v1/contracts/{contract_id}/decompiled`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DecompiledParams {
    /// Requested representation: `rust` (default) or `wat`. When `rust` is
    /// requested but emission fails, the response carries the WAT fallback
    /// with `representation: "wat"` and `rust_error` set — no second
    /// round-trip needed.
    pub format: Option<String>,
}

/// Response of `GET /v1/contracts/{contract_id}/decompiled` (task 0465).
///
/// Source is reconstructed on demand by the pinned `soroban-ret` crate —
/// experimental by nature: unrecovered values surface as explicit `todo!()`
/// holes in the Rust text. The marker counts measure completeness, not
/// correctness (a hole-free function can still be wrong); the frontend
/// keeps its permanent "auto-reconstructed" notice regardless.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DecompiledResponse {
    pub contract_id: String,
    /// Lowercase hex hash of the decompiled binary — the response is
    /// immutable per (`wasm_hash`, `soroban_ret_version`).
    pub wasm_hash: String,
    /// What `source` contains: `rust` or `wat`.
    pub representation: String,
    pub source: String,
    /// Soroban SDK version from the binary's `contractmetav0`, when present.
    pub sdk_version: Option<String>,
    /// Decompiler version that produced `source`.
    pub soroban_ret_version: String,
    /// `pub fn` count in the emitted Rust; `null` for WAT.
    pub functions: Option<u32>,
    /// `todo!()` marker count (unrecovered values); `null` for WAT.
    pub todo_holes: Option<u32>,
    /// Distinct `var_N` identifiers (unrecovered names); `null` for WAT.
    pub unknown_vars: Option<u32>,
    /// Set when `rust` was requested but emission failed — `source` then
    /// carries the WAT fallback.
    pub rust_error: Option<String>,
}
