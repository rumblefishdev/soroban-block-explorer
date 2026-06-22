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
        // Soroban RPC errors are **permanent by default** — see
        // `is_transient_soroban_rpc_pattern`. A genuine network failure
        // (connect / timeout / 5xx) surfaces as the `Http` variant above,
        // NOT here: a `SorobanRpc(String)` only exists once the RPC already
        // answered HTTP 200 with a JSON body carrying the error, so it is a
        // contract- or request-level fault that retry cannot fix.
        NftTokenUriError::SorobanRpc(msg) => is_transient_soroban_rpc_pattern(msg),
        _ => false,
    }
}

/// `SorobanRpc` errors that are genuinely retryable (server-side, not
/// contract/request-level). **Default is permanent** — only these escape to
/// transient.
///
/// Inverts the pre-2026-06 deny-list (which defaulted `SorobanRpc` to
/// *transient* and enumerated permanent patterns). That leaked any
/// un-enumerated permanent shape to an infinite SQS retry → DLQ: e.g.
/// `Error(WasmVm, MissingValue)` "trying to invoke non-existent contract
/// function" (a contract that simply lacks `token_uri`) was classified
/// transient because it was not on the list. Since a stringified
/// `SorobanRpc` always means "the RPC responded" (network faults are the
/// `Http` variant), permanent-by-default is the correct shape: the prior
/// audit's permanent patterns (`MismatchingParameterLen`, `symbol not found
/// in slice of strs`, `Error(WasmVm, UnexpectedSize)`, `Error(Storage,
/// MissingValue)`, `Error(WasmVm, MissingValue)`) are now all permanent
/// without enumeration. A misfire (a true transient phrased outside the
/// allow-list → false sentinel) is recoverable with `--retry-sentinels`;
/// the inverse (false transient → DLQ loop) is not.
///
/// The allow-list still errs narrow on purpose, but covers the common
/// server/gateway 5xx phrasings a provider emits during an outage — left out,
/// a provider's bad hour silently converts a chunk of the population to
/// permanent sentinels (recoverable only by an operator running
/// `--retry-sentinels`). Matching is case-insensitive.
fn is_transient_soroban_rpc_pattern(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    // JSON-RPC server-side error codes — retryable.
    m.contains("-32603")   // "Internal error"
        || m.contains("-32000") // generic server error (execution/timeout)
        // Rate-limiting / load-shedding phrasings.
        || m.contains("rate limit")
        || m.contains("try again")
        || m.contains("overloaded")
        // Gateway / upstream 5xx phrasings (LB/proxy in front of the RPC).
        || m.contains("unavailable") // "service unavailable" / 503
        || m.contains("bad gateway") // 502
        || m.contains("gateway timeout") // 504
        || m.contains("timeout")
        || m.contains("deadline") // "context deadline exceeded"
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
    fn soroban_rpc_internal_error_is_transient() {
        // JSON-RPC -32603 = server-side internal error → retryable.
        assert!(is_transient(&NftTokenUriError::SorobanRpc(
            "{\"code\":-32603,\"message\":\"Internal error\"}".into()
        )));
        assert!(is_transient(&NftTokenUriError::SorobanRpc(
            "rate limit exceeded, try again".into()
        )));
    }

    #[test]
    fn soroban_rpc_gateway_5xx_is_transient() {
        // Provider/LB outage phrasings (mixed case) — server-side, retryable.
        for msg in [
            "503 Service Unavailable",
            "502 Bad Gateway",
            "504 Gateway Timeout",
            "upstream request timeout",
            "context deadline exceeded",
            "{\"code\":-32000,\"message\":\"execution timeout\"}",
        ] {
            assert!(
                is_transient(&NftTokenUriError::SorobanRpc(msg.into())),
                "{msg:?} should be transient"
            );
        }
    }

    #[test]
    fn soroban_rpc_wasmvm_missing_value_is_permanent() {
        // The reported bug: contract lacks the `token_uri` entrypoint. A retry
        // can never make the function appear → permanent (was leaking transient
        // → DLQ loop under the old deny-list).
        assert!(!is_transient(&NftTokenUriError::SorobanRpc(
            "HostError: Error(WasmVm, MissingValue) … trying to invoke non-existent contract function, token_uri".into()
        )));
    }

    #[test]
    fn soroban_rpc_unknown_shape_defaults_permanent() {
        // Any un-enumerated SorobanRpc error is permanent by default (the RPC
        // answered → contract/request fault), not transient.
        assert!(!is_transient(&NftTokenUriError::SorobanRpc(
            "HostError: Error(Contract, #5)".into()
        )));
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
