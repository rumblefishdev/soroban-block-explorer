//! Error taxonomy for the `token_uri()` fetcher.
//!
//! Two-bucket split mirrors the worker's retry semantics
//! (`enrich_and_persist::EnrichError`):
//!
//! - **Permanent** — producer / contract bug or genuinely-unfetchable
//!   metadata (4xx, malformed JSON, missing `token_uri()`, contract
//!   doesn't honour the convention). Caller writes the sentinel `''`
//!   for every column.
//! - **Transient** — RPC / IPFS gateway 5xx, network blip, timeout.
//!   Caller surfaces `EnrichError::Transient` so the worker requests
//!   an SQS retry → DLQ after the redrive policy max-receive count.
//!
//! At the API runtime type-2 call site (`runtime_enrichment::nft_token_uri`)
//! every error variant collapses to `None` — the API never 5xx's because
//! of an enrichment failure (matches the SEP-1 + stellar archive pattern).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NftTokenUriError {
    /// Soroban RPC call failed (network, 5xx, contract reverted,
    /// `simulateTransaction` returned no `retval`, or token does not
    /// honour the `token_uri(token_id)` SEP/CAP convention). The
    /// transient-vs-permanent split is determined per-call by the
    /// fetcher.
    #[error("soroban rpc: {0}")]
    SorobanRpc(String),

    /// HTTP / IPFS gateway fetch failed for the URI returned by
    /// `token_uri()`. Includes 4xx (permanent), 5xx + timeout
    /// (transient).
    #[error("metadata fetch ({host}): {source}")]
    Http {
        host: String,
        #[source]
        source: reqwest::Error,
    },

    /// Body exceeded the per-fetch cap before fully buffering.
    #[error("metadata body exceeded {limit} bytes")]
    BodyTooLarge { limit: usize },

    /// Body was not valid UTF-8 / not parseable as JSON.
    #[error("malformed metadata JSON")]
    MalformedJson(#[source] serde_json::Error),

    /// `token_uri()` returned a Content-Type the fetcher does not know
    /// how to interpret. Includes the unrecognised content-type for
    /// triage.
    #[error("unsupported metadata content-type: {0}")]
    UnsupportedContentType(String),

    /// URI returned by `token_uri()` (or `image` field inside the
    /// fetched JSON) used a scheme the fetcher refuses to follow.
    /// Allowed: `https://`, `ipfs://`. Refused: `http://` (no TLS),
    /// `file://`, `data:`, `javascript:`, anything else.
    #[error("unsafe URI scheme: {uri}")]
    UnsafeScheme { uri: String },

    /// URI passed scheme check but failed structural validation: empty,
    /// userinfo present (`user:pass@host`), IP-literal hostname (`127.0.0.1`,
    /// `169.254.169.254`, `[::1]`, …), or non-RFC-1035 characters.
    /// Treated as permanent — the contract is misconfigured.
    #[error("malformed URI: {uri}")]
    MalformedUri { uri: String },

    /// Producer / parser bug — input from DB did not match the shape
    /// the fetcher expects. `field` is the tag (`"token_id (not u32)"`,
    /// `"contract_id strkey"`, …); `value` is the offending content
    /// quoted into the log. Hard-fail by design — visible instead of
    /// silent staleness. Examples:
    /// - `token_id` not parseable as `u32` (contract uses `ScVal::U64`
    ///   / `ScVal::String` / `ScVal::Bytes` token_ids instead of the
    ///   OpenZeppelin / ERC-721 sequential-counter convention);
    /// - `contract_id` not a valid `C...` Soroban StrKey.
    #[error("malformed {field}: {value}")]
    MalformedInput { field: &'static str, value: String },

    /// JSON-RPC response shape did not match the expected
    /// `{result: {results: [{xdr: ...}]}}` envelope, or the inner
    /// `xdr` field decoded to a non-`ScVal::String` value.
    #[error("malformed simulateTransaction response: {0}")]
    MalformedRpcResponse(String),

    /// XDR encode/decode failure (envelope serialisation or ScVal
    /// deserialisation).
    #[error("XDR codec: {0}")]
    Xdr(#[from] stellar_xdr::curr::Error),
}

/// Classifier for the worker's `EnrichError` mapping. Transient
/// failures route to `EnrichError::Transient` → SQS retry → DLQ;
/// permanent failures fall through to the sentinel-write path so
/// the row is recorded as "fetch attempted, no value" without
/// burning the SQS retry budget.
///
/// Mirrors `sep1_assets::is_transient`. Default is **permanent** — only
/// the variants below escape to retry.
pub fn is_transient(err: &NftTokenUriError) -> bool {
    match err {
        NftTokenUriError::Http { source, .. } => {
            source.is_timeout()
                || source.is_connect()
                || source
                    .status()
                    .map(|s| s.is_server_error())
                    .unwrap_or(false)
        }
        // Soroban RPC errors collapse a wide surface (network, 5xx,
        // contract revert). Discriminate by error message content:
        // some known patterns are fundamentally permanent (contract
        // function signature mismatches, missing symbols) — these
        // would burn SQS retry budget forever if classified as
        // transient. Audit 2026-05-13 (Bug #6) identified the
        // patterns below.
        NftTokenUriError::SorobanRpc(msg) => !is_permanent_soroban_rpc_pattern(msg),
        _ => false,
    }
}

/// Heuristic — Soroban RPC error messages that are fundamentally
/// permanent (function signature won't change, missing function won't
/// appear, archived contract instance won't return). Adding patterns
/// is safe and additive.
fn is_permanent_soroban_rpc_pattern(msg: &str) -> bool {
    // `Func(MismatchingParameterLen)` — function exists but arity mismatch.
    // Real Soroban NFTs split between SEP-50 (1-arg `token_uri(token_id)`)
    // and SEP-39 (0-arg `token_uri()`). Either signature is fixed once
    // the contract is deployed.
    msg.contains("MismatchingParameterLen")
        // `"symbol not found in slice of strs"` — contract simply does
        // not export the function we tried to call. Common for the
        // Bug #3 false-positive NFTs (SAC contracts wrongly flagged).
        || msg.contains("symbol not found in slice of strs")
        // Generic "Error(WasmVm, UnexpectedSize)" — emitted alongside
        // MismatchingParameterLen and similar permanent VM contract
        // shape mismatches.
        || msg.contains("Error(WasmVm, UnexpectedSize)")
        // `Error(Storage, MissingValue)` —
        // "trying to get non-existing value for contract instance".
        // The contract instance entry has been pruned from live state
        // and won't return without re-deployment. Treated as permanent:
        // any future activity on that contract would push the instance
        // back to live state, at which point a fresh enrichment trigger
        // will run anyway. Retrying meanwhile is pure SQS retry budget
        // waste. Audit 2026-05-13 measured 23/1000 transient-classified
        // errors of this shape in the false-positive NFT population.
        || msg.contains("Error(Storage, MissingValue)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_too_large_is_permanent() {
        assert!(!is_transient(&NftTokenUriError::BodyTooLarge { limit: 1 }));
    }

    #[test]
    fn unsafe_scheme_is_permanent() {
        assert!(!is_transient(&NftTokenUriError::UnsafeScheme {
            uri: "data:".into()
        }));
    }

    #[test]
    fn malformed_uri_is_permanent() {
        assert!(!is_transient(&NftTokenUriError::MalformedUri {
            uri: "https://127.0.0.1/".into()
        }));
    }

    #[test]
    fn soroban_rpc_is_transient() {
        assert!(is_transient(&NftTokenUriError::SorobanRpc("5xx".into())));
    }

    #[test]
    fn soroban_rpc_mismatching_parameter_len_is_permanent() {
        assert!(!is_transient(&NftTokenUriError::SorobanRpc(
            "HostError: Error(WasmVm, UnexpectedSize) … MismatchingParameterLen … token_uri".into()
        )));
    }

    #[test]
    fn soroban_rpc_symbol_not_found_is_permanent() {
        assert!(!is_transient(&NftTokenUriError::SorobanRpc(
            "HostError: Error(Value, InvalidInput) … symbol not found in slice of strs … token_uri"
                .into()
        )));
    }

    #[test]
    fn soroban_rpc_storage_missing_value_is_permanent() {
        assert!(!is_transient(&NftTokenUriError::SorobanRpc(
            "HostError: Error(Storage, MissingValue) … trying to get non-existing value for contract instance".into()
        )));
    }
}
