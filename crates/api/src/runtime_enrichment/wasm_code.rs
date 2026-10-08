//! On-demand contract WASM fetch + decompilation (task 0465, refs #374).
//!
//! Two halves, both fail-soft at the handler boundary:
//!
//! - [`WasmCodeFetcher`] — transport: `getLedgerEntries` against a Soroban
//!   RPC pool (`SOROBAN_RPC_URLS`, required — read by
//!   `enrichment_shared::soroban_rpc`, as for `nft_token_uri`).
//!   Contract code is content-addressed, so a fetched blob is verified
//!   against the requested hash before use.
//! - [`decompile_blocking`] — CPU: `soroban-ret` (pinned `=0.0.4`) Rust
//!   emission with WAT fallback. Runs on the blocking pool (same rationale
//!   as `stellar_archive`): the full-mainnet sweep measured median 28 ms /
//!   p99 1.1 s, but the tail reaches minutes — the handler bounds it with
//!   `tokio::time::timeout`.
//!
//! Deliberately no persistence: decompilation is recomputed per request and
//! the response is cacheable by hash (`Cache-Control` at the handler).
//! Revisit a cache only if real traffic says so (task 0465 §Open Points).

use std::sync::Arc;

use super::rpc_pool::{RpcFailure, RpcPool};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use stellar_xdr::{
    Hash, LedgerEntryData, LedgerKey, LedgerKeyContractCode, Limits, ReadXdr, WriteXdr,
};

/// Version of the pinned `soroban-ret` crate, surfaced on the wire so the
/// frontend can label output provenance. Keep in lockstep with the
/// `soroban-ret = "=0.0.4"` pin in `Cargo.toml` on every bump.
pub const SOROBAN_RET_VERSION: &str = "0.0.4";

/// Errors from the WASM fetch path. The handler maps every variant to a
/// 5xx; code that is not live (archived/expired) is `Ok(None)` → 404.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("invalid wasm hash: {0}")]
    BadHash(String),
    #[error("XDR encode/decode: {0}")]
    Xdr(String),
    #[error("all RPC endpoints failed; last: {0}")]
    Rpc(String),
    #[error("RPC returned an error object: {0}")]
    RpcError(String),
}

/// Pooled Soroban RPC client for fetching contract code by wasm hash.
/// Cheaply cloneable; lives on [`super::RuntimeEnrichment`].
#[derive(Clone)]
pub struct WasmCodeFetcher {
    rpc: RpcPool,
}

impl WasmCodeFetcher {
    /// Production constructor. RPC pool from `SOROBAN_RPC_URLS` (required —
    /// `enrichment_shared::soroban_rpc::rpc_urls_from_env`).
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            rpc: RpcPool::new()?,
        })
    }

    /// Explicit RPC pool — tests, and anything that already holds the list.
    pub fn with_rpc_urls(rpc_urls: Vec<String>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            rpc: RpcPool::with_rpc_urls(rpc_urls)?,
        })
    }

    /// Fetch the contract code bytes for a lowercase-hex wasm hash.
    ///
    /// `Ok(None)` means the RPC answered but holds no live `CONTRACT_CODE`
    /// entry for this hash (expired/archived — the sweep found 0 such
    /// cases on mainnet, but the state is reachable in principle).
    pub async fn fetch_wasm(&self, wasm_hash_hex: &str) -> Result<Option<Vec<u8>>, FetchError> {
        let bytes = hex::decode(wasm_hash_hex).map_err(|e| FetchError::BadHash(e.to_string()))?;
        let hash: [u8; 32] = bytes
            .try_into()
            .map_err(|_| FetchError::BadHash("hash must be 32 bytes".into()))?;
        let key = LedgerKey::ContractCode(LedgerKeyContractCode { hash: Hash(hash) });
        let key_b64 = BASE64.encode(
            key.to_xdr(Limits::none())
                .map_err(|e| FetchError::Xdr(e.to_string()))?,
        );
        let entries = self
            .rpc
            .get_ledger_entries(vec![key_b64])
            .await
            .map_err(|e| match e {
                RpcFailure::Unreachable(m) => FetchError::Rpc(m),
                RpcFailure::ErrorObject(m) => FetchError::RpcError(m),
            })?;
        // Every endpoint answered and none holds the entry: not live.
        let Some(entry_xdr) = entries.first().and_then(|e| e["xdr"].as_str()) else {
            return Ok(None);
        };
        let entry_bytes = BASE64
            .decode(entry_xdr)
            .map_err(|e| FetchError::Xdr(format!("entry base64: {e}")))?;
        let data = LedgerEntryData::from_xdr(entry_bytes, Limits::none())
            .map_err(|e| FetchError::Xdr(e.to_string()))?;
        let LedgerEntryData::ContractCode(code_entry) = data else {
            return Err(FetchError::Xdr("entry is not CONTRACT_CODE".into()));
        };
        // Content-addressed sanity check: the ledger key we asked for IS
        // the sha256 of the code; a mismatch means a broken RPC.
        if code_entry.hash.0 != hash {
            return Err(FetchError::RpcError("returned code hash mismatch".into()));
        }
        Ok(Some(code_entry.code.to_vec()))
    }
}

/// One Soroban-compliance diagnostic, flattened for the wire.
///
/// `category` and `severity` are rendered from soroban-ret's own enums —
/// both are `#[non_exhaustive]`, so a future release can add variants and
/// we degrade to their `Debug` name rather than failing to compile.
#[derive(Debug)]
pub struct Diagnostic {
    /// `floating_point` | `call_indirect` | `non_rust_sdk` | …
    pub category: String,
    /// `warning` | `info`.
    pub severity: String,
    pub message: String,
    /// Index of the offending function, when the check is function-scoped.
    pub function_index: Option<u32>,
}

/// Result of one decompilation run, ready to serialize at the handler.
#[derive(Debug)]
pub struct Decompiled {
    /// `"rust"` or `"wat"` — what `source` actually contains.
    pub representation: &'static str,
    pub source: String,
    /// SDK version from `contractmetav0`, when present (Rust path only).
    pub sdk_version: Option<String>,
    /// `pub fn` count in the emitted Rust (None for WAT).
    pub functions: Option<u32>,
    /// `todo!(` marker count — unrecovered values (None for WAT).
    /// Interim completeness metric per the soroban-ret team's guidance;
    /// replaced by `soroban_ret::recovery` once released.
    pub todo_holes: Option<u32>,
    /// Distinct `var_N` identifiers — unrecovered names (None for WAT).
    pub unknown_vars: Option<u32>,
    /// Set when Rust was requested but emission failed and `source`
    /// carries the WAT fallback instead.
    pub rust_error: Option<String>,
    /// Soroban-compliance diagnostics for this binary — constructs the
    /// decompiler does not model well (floats, reference types, non-Rust
    /// SDK…). Empty on the WAT-only paths, which skip the Soroban stage.
    pub diagnostics: Vec<Diagnostic>,
}

/// Bounds how many Rust decompilations may run at once.
///
/// `tokio::time::timeout` cancels the *await*, not the blocking work — a
/// decompilation that overruns the handler's deadline keeps burning a
/// blocking-pool thread after the client has been answered (the mainnet
/// sweep found binaries needing 100 s+). The permit is moved into the
/// blocking closure, so it is released when the work truly ends, not when
/// the request gives up: overruns can pile up to `available_parallelism`
/// and no further.
static DECOMPILE_SLOTS: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();

fn decompile_slots() -> &'static Arc<tokio::sync::Semaphore> {
    DECOMPILE_SLOTS.get_or_init(|| {
        let permits = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(2)
            .max(1);
        Arc::new(tokio::sync::Semaphore::new(permits))
    })
}

/// Run [`decompile_blocking`] on the blocking pool under the concurrency
/// bound. Wrap the call in `tokio::time::timeout` for the client-facing
/// deadline — the timeout covers waiting for a slot as well.
///
/// WAT is deliberately **not** gated: it is the fallback the UI reaches for
/// when Rust fails, so it must not starve behind saturated Rust slots
/// (measured at 0.45 s even for a 2.1 MB output).
pub async fn decompile_on_blocking_pool(
    wasm: Vec<u8>,
    want_wat: bool,
) -> Result<Decompiled, String> {
    let permit = if want_wat {
        None
    } else {
        Some(
            Arc::clone(decompile_slots())
                .acquire_owned()
                .await
                .map_err(|e| format!("decompile slot: {e}"))?,
        )
    };
    tokio::task::spawn_blocking(move || {
        // Held for the real duration of the work, including any overrun
        // past the handler's timeout.
        let _permit = permit;
        decompile_blocking(&wasm, want_wat)
    })
    .await
    .map_err(|join_err| format!("decompile task join error: {join_err}"))?
}

/// Decompile `wasm`. CPU-bound and synchronous — call from
/// `tokio::task::spawn_blocking` with a timeout around the join handle.
///
/// `want_wat` requests the WAT representation directly; otherwise Rust is
/// attempted first and WAT serves as the in-response fallback (sweep:
/// 99.5% of mainnet hashes take the Rust path). `Err` only when every
/// representation failed.
pub fn decompile_blocking(wasm: &[u8], want_wat: bool) -> Result<Decompiled, String> {
    if want_wat {
        let wat = soroban_ret::wasm_to_wat(wasm).map_err(|e| e.to_string())?;
        return Ok(Decompiled {
            representation: "wat",
            source: wat,
            sdk_version: None,
            functions: None,
            todo_holes: None,
            unknown_vars: None,
            rust_error: None,
            diagnostics: Vec::new(),
        });
    }
    let options = soroban_ret::DecompileOptions::default();
    match soroban_ret::decompile_with_options(wasm, &options) {
        Ok(result) => {
            let counts = MarkerCounts::of(&result.source);
            let diagnostics = map_diagnostics(&result.validation);
            Ok(Decompiled {
                representation: "rust",
                source: result.source,
                sdk_version: result.sdk_version,
                functions: Some(counts.functions),
                todo_holes: Some(counts.todo_holes),
                unknown_vars: Some(counts.unknown_vars),
                rust_error: None,
                diagnostics,
            })
        }
        Err(rust_err) => {
            let wat = soroban_ret::wasm_to_wat(wasm)
                .map_err(|wat_err| format!("rust: {rust_err}; wat: {wat_err}"))?;
            Ok(Decompiled {
                representation: "wat",
                source: wat,
                sdk_version: None,
                functions: None,
                todo_holes: None,
                unknown_vars: None,
                rust_error: Some(rust_err.to_string()),
                diagnostics: Vec::new(),
            })
        }
    }
}

/// Flatten soroban-ret's validation report for the wire.
///
/// Both enums are `#[non_exhaustive]`; the wildcard arms fall back to the
/// `Debug` name so a new variant in a later release surfaces as an unknown
/// category rather than breaking the build.
fn map_diagnostics(report: &soroban_ret::ValidationReport) -> Vec<Diagnostic> {
    use soroban_ret::{DiagnosticCategory as C, DiagnosticSeverity as S};
    report
        .diagnostics
        .iter()
        .map(|d| Diagnostic {
            category: match d.category {
                C::FloatingPoint => "floating_point".to_owned(),
                C::ReferenceTypes => "reference_types".to_owned(),
                C::MultiValue => "multi_value".to_owned(),
                C::MultiMemory => "multi_memory".to_owned(),
                C::CallIndirect => "call_indirect".to_owned(),
                C::UnknownInstruction => "unknown_instruction".to_owned(),
                C::NonRustSdk => "non_rust_sdk".to_owned(),
                other => format!("{other:?}").to_lowercase(),
            },
            severity: match d.severity {
                S::Warning => "warning".to_owned(),
                S::Info => "info".to_owned(),
                other => format!("{other:?}").to_lowercase(),
            },
            message: d.message.clone(),
            function_index: d.function_index,
        })
        .collect()
}

/// Completeness markers counted over emitted Rust. Matches the full-mainnet
/// sweep methodology (task 0465 `benchmark/run_sweep.py`): `todo!(` in both
/// `prettyplease` spellings, `var_N` as distinct whole identifiers.
/// Measures completeness, not correctness.
struct MarkerCounts {
    functions: u32,
    todo_holes: u32,
    unknown_vars: u32,
}

impl MarkerCounts {
    fn of(src: &str) -> Self {
        let todo_holes = (src.matches("todo!(").count() + src.matches("todo !(").count()) as u32;
        let functions = src.matches("pub fn ").count() as u32;

        let mut vars = std::collections::HashSet::new();
        let bytes = src.as_bytes();
        let mut search_from = 0;
        while let Some(rel) = src[search_from..].find("var_") {
            let start = search_from + rel;
            search_from = start + 4;
            // whole-identifier boundary on the left
            if start > 0 {
                let prev = bytes[start - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' {
                    continue;
                }
            }
            let digits_start = start + 4;
            let mut end = digits_start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            // require at least one digit and an identifier boundary on the right
            if end > digits_start
                && (end == bytes.len()
                    || (!bytes[end].is_ascii_alphanumeric() && bytes[end] != b'_'))
            {
                vars.insert(&src[start..end]);
            }
        }
        Self {
            functions,
            todo_holes,
            unknown_vars: vars.len() as u32,
        }
    }
}

#[cfg(test)]
mod tests;
