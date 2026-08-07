//! On-demand contract WASM fetch + decompilation (task 0465, refs #374).
//!
//! Two halves, both fail-soft at the handler boundary:
//!
//! - [`WasmCodeFetcher`] — transport: `getLedgerEntries` against a Soroban
//!   RPC pool (`SOROBAN_RPC_URLS` comma-sep → `SOROBAN_RPC_URL` → SDF
//!   default, same convention as `enrichment-shared::nft_token_uri`).
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
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use stellar_xdr::{
    Hash, LedgerEntryData, LedgerKey, LedgerKeyContractCode, Limits, ReadXdr, WriteXdr,
};

/// Version of the pinned `soroban-ret` crate, surfaced on the wire so the
/// frontend can label output provenance. Keep in lockstep with the
/// `soroban-ret = "=0.0.4"` pin in `Cargo.toml` on every bump.
pub const SOROBAN_RET_VERSION: &str = "0.0.4";

/// SDF public mainnet RPC — the single default when no `SOROBAN_RPC_URLS` /
/// `SOROBAN_RPC_URL` env is set.
const DEFAULT_SOROBAN_RPC_URL: &str = "https://mainnet.sorobanrpc.com";

/// Errors from the WASM fetch path. The handler maps every variant to a
/// 5xx except [`FetchError::NotLive`] (archived/expired entry → 404).
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
    client: reqwest::Client,
    rpc_urls: Arc<Vec<String>>,
}

impl WasmCodeFetcher {
    /// Production constructor. RPC pool from `SOROBAN_RPC_URLS` (comma-sep)
    /// → single `SOROBAN_RPC_URL` → SDF default.
    pub fn new() -> Result<Self, reqwest::Error> {
        let rpc_urls = std::env::var("SOROBAN_RPC_URLS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .or_else(|| std::env::var("SOROBAN_RPC_URL").ok().map(|u| vec![u]))
            .unwrap_or_else(|| vec![DEFAULT_SOROBAN_RPC_URL.to_owned()]);
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("sorobanscan-api")
            .build()?;
        Ok(Self {
            client,
            rpc_urls: Arc::new(rpc_urls),
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
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLedgerEntries",
            "params": { "keys": [key_b64] },
        });

        let mut last_err = String::from("no endpoints configured");
        for url in self.rpc_urls.iter() {
            let resp = match self.client.post(url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            };
            if !resp.status().is_success() {
                last_err = format!("{url}: HTTP {}", resp.status());
                continue;
            }
            let value: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            };
            if let Some(err) = value.get("error") {
                return Err(FetchError::RpcError(err.to_string()));
            }
            let entries = value["result"]["entries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
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
            return Ok(Some(code_entry.code.to_vec()));
        }
        Err(FetchError::Rpc(last_err))
    }
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
        });
    }
    let options = soroban_ret::DecompileOptions::default();
    match soroban_ret::decompile_with_options(wasm, &options) {
        Ok(result) => {
            let counts = MarkerCounts::of(&result.source);
            Ok(Decompiled {
                representation: "rust",
                source: result.source,
                sdk_version: result.sdk_version,
                functions: Some(counts.functions),
                todo_holes: Some(counts.todo_holes),
                unknown_vars: Some(counts.unknown_vars),
                rust_error: None,
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
            })
        }
    }
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
mod tests {
    use super::*;

    #[test]
    fn counts_markers_in_emitted_rust() {
        let src = r#"
            pub fn transfer(env: Env) -> i128 {
                let x = get(&todo!("unknown value")).unwrap();
                let y = todo !("host call");
                var_1 + var_2 + var_1
            }
            pub fn balance(env: Env) -> i128 { var_10 }
        "#;
        let c = MarkerCounts::of(src);
        assert_eq!(c.functions, 2);
        assert_eq!(c.todo_holes, 2);
        assert_eq!(c.unknown_vars, 3); // var_1, var_2, var_10 — deduped
    }

    #[test]
    fn var_matching_requires_identifier_boundaries() {
        // `my_var_3` is part of a longer identifier; `var_` with no digits
        // and `var_x` are not markers.
        let src = "my_var_3 var_ var_x var_7";
        let c = MarkerCounts::of(src);
        assert_eq!(c.unknown_vars, 1); // only var_7
    }

    #[test]
    fn decompiles_a_trivial_wasm_to_rust() {
        // Smallest valid wasm module: magic + version. `DecompileMode::Auto`
        // falls back to generic-wasm decompilation, so even a spec-less
        // module takes the Rust path (with zero functions).
        let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let d = decompile_blocking(&wasm, false).expect("rust via generic mode");
        assert_eq!(d.representation, "rust");
        assert!(d.rust_error.is_none());
        assert_eq!(d.functions, Some(0));
    }

    #[test]
    fn garbage_bytes_fail_both_paths() {
        let not_wasm = b"definitely not a wasm module";
        assert!(decompile_blocking(not_wasm, false).is_err());
        assert!(decompile_blocking(not_wasm, true).is_err());
    }

    /// Live-RPC smoke test (run explicitly: `cargo test -- --ignored`).
    /// The hash is the most-instantiated mainnet contract (task 0465 sweep);
    /// size asserted against the bytes fetched during the sweep.
    #[tokio::test]
    #[ignore = "hits live mainnet RPC"]
    async fn fetches_real_wasm_by_hash() {
        let fetcher = WasmCodeFetcher::new().expect("build fetcher");
        let code = fetcher
            .fetch_wasm("07097f83dae3b746db7dba3263d9cc334efb88a9a7d5450fb96ca19f33d284b0")
            .await
            .expect("rpc ok")
            .expect("entry live");
        assert_eq!(code.len(), 6831);
    }

    #[test]
    fn wat_direct_request() {
        let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        let d = decompile_blocking(&wasm, true).expect("wat");
        assert_eq!(d.representation, "wat");
        assert!(d.rust_error.is_none());
    }
}
