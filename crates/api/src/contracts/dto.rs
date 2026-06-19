//! Request and response DTOs for the contracts endpoints.
//! Wire shapes mirror canonical SQL `endpoint-queries/{11..14}_*.sql`.

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
    /// Sum of `soroban_events_appearances.amount` over the same window
    /// as `recent_invocations`. One appearance row can represent multiple
    /// actual events (`amount > 1`) so we sum rather than COUNT(*) — the
    /// figure matches what `GET /v1/contracts/:id/events` would return
    /// over the window.
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
    /// Token name from the on-chain instance-storage `METADATA` struct
    /// (ADR 0049), via the `soroban_contract_metadata` side table. `None` for
    /// non-token contracts (SAC names derive from the asset code, not here).
    pub name: Option<String>,
    /// Token symbol (SEP-41 `METADATA`). `None` for non-token contracts.
    pub symbol: Option<String>,
    /// Token decimals (SEP-41 `METADATA`) — load-bearing for amount rendering.
    /// `None` for non-token contracts.
    pub decimals: Option<u32>,
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
    /// Folded invocation-tree node count for this appearance.
    pub amount: i32,
    pub created_at: DateTime<Utc>,
    pub successful: bool,
}

/// One row per event — an appearance with `amount > 1` expands to that
/// many rows (per-tx fields repeated, per-event fields unique).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventItem {
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    pub transaction_id: i64,
    pub successful: bool,
    pub amount: i64,
    pub created_at: DateTime<Utc>,
    pub event_type: String,
    pub topics: Vec<serde_json::Value>,
    pub data: serde_json::Value,
}

/// Opaque pagination cursor for `GET /contracts/:id/events`, datasource-tagged
/// (ADR 0008) so a cursor minted for one backend is rejected after a flag flip;
/// a legacy/untagged cursor (no `src`) fails to decode → clean 400.
///
/// - `Pg` — `(created_at, transaction_id)` keyset over the folded appearance
///   index `soroban_events_appearances` (payload overlaid from the Archive at
///   read time).
/// - `Ch` — `(ledger_sequence, transaction_id, event_index)` keyset over the
///   full-content `soroban_events` table (per-event rows; `event_index` is the
///   multi-event-tx tie-break, non-optional so a CH keyset never binds a NULL
///   tuple element).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "src", rename_all = "snake_case")]
pub enum EventCursor {
    Pg {
        ts: DateTime<Utc>,
        id: i64,
    },
    Ch {
        ledger_sequence: i64,
        transaction_id: i64,
        event_index: i16,
    },
}
