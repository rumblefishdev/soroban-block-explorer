//! Request and response DTOs for the contracts endpoints.
//! The SQL behind these shapes is in `contracts::queries`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::runtime_enrichment::source_verification::VerificationStatus;

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

/// The classic asset a SAC contract is the contract-side facet of (ADR 0051,
/// task 0441). `None` on non-SAC contracts — and on the rare SAC with no
/// resolvable `asset_sac` facet row (2 of 3,946 on prod), where the frontend
/// keeps the bare SAC badge. Native XLM is `asset_code: null, issuer: null`;
/// a classic asset always carries both (an asset code alone is ambiguous —
/// prod holds many issuers of "USDC").
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SacAsset {
    /// Classic asset code (e.g. `USDC`); `null` = native XLM.
    pub asset_code: Option<String>,
    /// Issuer account G-strkey; `null` = native XLM.
    pub issuer: Option<String>,
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
    /// The asset this SAC mirrors; `null` unless `is_sac` (see [`SacAsset`]).
    pub sac_asset: Option<SacAsset>,
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
    /// The asset this SAC mirrors; `null` unless `is_sac` (see [`SacAsset`]).
    pub sac_asset: Option<SacAsset>,
    /// Task 0327: contract mutability, 3-state.
    /// - `Some(true)` → **Upgradeable**: the current WASM imports
    ///   `update_current_contract_wasm` (a self-upgrade path).
    /// - `Some(false)` → **Immutable/frozen**: it cannot upgrade itself (a SAC
    ///   has no WASM and is always `Some(false)`).
    /// - `None` → **Unknown**: the WASM interface hasn't been parsed yet (stub /
    ///   pre-0327 row) — the frontend renders no chip.
    ///
    /// Derived from the WASM at parse time
    /// (`wasm_programs.metadata.upgradeable`), not from a ledger flag
    /// (none exists).
    pub upgradeable: Option<bool>,
    /// Task 0548 / CAP-85 (protocol 28): the contract this one borrows its code
    /// from, and the tag naming which of that owner's executables it runs. Both
    /// `null` for every contract that carries its own executable.
    ///
    /// When they are set, `wasm_hash` above is the hash the reference resolves
    /// to at the time of the request — the code the contract genuinely runs,
    /// which is what the protocol's own `get_address_executable` reports too.
    /// It is NOT stored against this contract: the owner can re-point the whole
    /// fleet without a single ledger change touching it, so a stored copy would
    /// go quietly out of date. Read these two to tell "runs its own code" from
    /// "runs someone else's".
    pub executable_owner: Option<String>,
    pub executable_tag: Option<String>,
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
/// `wasm_programs.metadata` (JSONB). Field shape mirrors the
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
    /// Who made the call: a `G…` account or a `C…` contract (task 0600).
    pub caller: Option<String>,
    pub created_at: DateTime<Utc>,
    pub successful: bool,
}

/// One row per event — the full-content `soroban_events` table stores one
/// row per event (no appearance-fold expansion).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventItem {
    /// The stellar-rpc event id, exactly as `getEvents` returns it
    /// (ADR 0059).
    pub id: String,
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    pub successful: bool,
    pub created_at: DateTime<Utc>,
    pub event_type: String,
    pub topics: Vec<serde_json::Value>,
    pub data: serde_json::Value,
}

/// Opaque pagination cursor for `GET /contracts/:id/events`, tagged (ADR 0008)
/// so a cursor of an older keyset fails to decode → clean 400.
///
/// Keyset = the stellar-rpc event id `(ledger_sequence, transaction_index,
/// operation_index, event_index)`, the `soroban_events` key after the
/// contract (ADR 0059). A cursor minted before it (`src: ch`, keyed by
/// `transaction_id`) no longer decodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "src", rename_all = "snake_case")]
pub enum EventCursor {
    ChEventId {
        ledger_sequence: i64,
        transaction_index: u32,
        operation_index: u16,
        event_index: u32,
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
    /// Soroban-compliance diagnostics for this binary: constructs the
    /// decompiler does not model well, so the reader knows *why* a
    /// reconstruction may be thin. Empty on the WAT paths.
    pub diagnostics: Vec<DecompileDiagnostic>,
}

/// One entry of [`DecompiledResponse::diagnostics`].
///
/// `severity` is soroban-ret's own (`warning` | `info`) and is deliberately
/// passed through rather than re-graded here — with one caveat for the UI:
/// `call_indirect` warnings are usually benign `core::fmt` vtables, so they
/// should not be rendered as errors.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DecompileDiagnostic {
    /// `floating_point` | `reference_types` | `multi_value` |
    /// `multi_memory` | `call_indirect` | `unknown_instruction` |
    /// `non_rust_sdk`, or a lowercased new variant from a later release.
    pub category: String,
    /// `warning` | `info`.
    pub severity: String,
    pub message: String,
    /// Index of the offending function, for function-scoped checks.
    pub function_index: Option<u32>,
}

/// Response of `GET /v1/contracts/{contract_id}/verification` (task 0639).
///
/// What independent verifiers say about the contract's current Wasm, read
/// from a verifier aggregator (BlockSmith Query API) on request. `status` is
/// the one branch the page needs; `source` and `build` describe the record
/// it shows.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceVerificationResponse {
    pub contract_id: String,
    /// Lowercase hex hash the verifiers were asked about; null when
    /// `unavailable` (nothing was asked).
    pub wasm_hash: Option<String>,
    /// The one branch the page needs; see `VerificationStatus`.
    pub status: VerificationStatus,
    /// When the aggregator last refreshed this answer (RFC 3339); null when
    /// `unavailable`.
    pub checked_at: Option<String>,
    /// Every verifier's own answer, in the aggregator's order.
    pub verifiers: Vec<SourceVerifier>,
    /// The published source archive of the shown record, when there is one.
    pub source: Option<VerifiedSourceArchive>,
    /// How the shown record was built (SEP-58 fields), when there is one.
    pub build: Option<BuildRecipe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SourceVerifier {
    /// Display name as the verifier publishes it.
    pub name: Option<String>,
    pub url: String,
    /// The verifier's own result, e.g. `verified` or `mismatched`.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifiedSourceArchive {
    /// Content-addressed archive on the verifier's storage.
    pub url: String,
    /// Lowercase hex sha256 the archive must hash to.
    pub sha256: String,
    /// `tar.gz` or `zip`.
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuildRecipe {
    /// Cargo package the verifier built (`--package`).
    pub package: Option<String>,
    /// Cargo manifest inside the archive (`--manifest-path`).
    pub manifest_path: Option<String>,
    /// Build image, pinned by digest (`bldimg`).
    pub image: Option<String>,
    /// Build options as the verifier recorded them (`bldopt`).
    pub options: Vec<String>,
    /// Build arguments, in order (`bldarg`; empty means `contract build`).
    pub arguments: Vec<String>,
    /// When the verifier processed the build (RFC 3339).
    pub verified_at: Option<String>,
    /// The verifier's full report, when it publishes one.
    pub report_url: Option<String>,
}

/// Response of `GET /v1/contracts/{contract_id}/verified-source` (task 0639):
/// the unpacked archive of a `verified` record, after its sha256 matched.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifiedSourceResponse {
    pub contract_id: String,
    pub wasm_hash: String,
    pub archive_sha256: String,
    /// Every regular file, sorted by path.
    pub files: Vec<VerifiedSourceFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifiedSourceFile {
    /// Path inside the archive, as the archive states it.
    pub path: String,
    pub size: u64,
    /// UTF-8 text; `null` for a file that is not text.
    pub content: Option<String>,
}
