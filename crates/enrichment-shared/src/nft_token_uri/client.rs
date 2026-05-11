//! `NftTokenUriFetcher` — LRU-cached Soroban RPC + HTTP/IPFS client.
//!
//! Pipeline: `simulateTransaction(InvokeContract(token_uri, [ScVal::U32]))`
//! → ScVal::String → `validate_uri` → `ipfs://` to gateway → HTTP GET
//! → Content-Type branch. See `super::mod` for the side-by-side with
//! SEP-1, the rationale for source-naming, and the defensive-guard list.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use moka::future::Cache as FutureCache;
use reqwest::redirect::Policy;
use serde_json::{Value, json};
use stellar_xdr::curr::{
    ContractId, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, ReadXdr, ScAddress, ScString, ScSymbol,
    ScVal, SequenceNumber, StringM, Transaction, TransactionEnvelope, TransactionExt,
    TransactionV1Envelope, Uint256, VecM, WriteXdr,
};
use tracing::{debug, instrument};

use super::errors::NftTokenUriError;

/// Body cap for NFT metadata JSON (typical files <10 KB).
pub(super) const MAX_BODY_BYTES: usize = 256 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_CAPACITY: u64 = 1024;
const USER_AGENT: &str = concat!("soroban-block-explorer/", env!("CARGO_PKG_VERSION"));
/// SDF public mainnet RPC. Switch via `with_rpc_url` if rate-limited.
pub(super) const DEFAULT_SOROBAN_RPC_URL: &str = "https://mainnet.sorobanrpc.com";
pub(super) const IPFS_GATEWAY_BASE: &str = "https://cloudflare-ipfs.com/ipfs/";

/// `token_uri` is the OpenZeppelin / ERC-721 metadata-extension
/// function name. Stellar Soroban NFT contracts copy the convention.
const TOKEN_URI_FN: &str = "token_uri";

/// Fetcher for the per-NFT `token_uri()` JSON metadata pipeline
/// (Soroban RPC + HTTP / IPFS gateway).
///
/// `cache_key` is `"{contract_id}:{token_id}"`. Cache stores the parsed
/// `Option<Value>` (Some = real JSON or synthesised image-shape;
/// `None` reserved for future "fetched, intentionally empty" cases).
/// Errors are surfaced wrapped in `Arc` (moka's shared-failure idiom)
/// so the worker can classify transient-vs-permanent via
/// [`super::errors::is_transient`].
#[derive(Clone)]
pub struct NftTokenUriFetcher {
    client: reqwest::Client,
    rpc_url: Arc<String>,
    cache: FutureCache<String, Arc<Option<Value>>>,
}

impl NftTokenUriFetcher {
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_rpc_url(DEFAULT_SOROBAN_RPC_URL.to_owned())
    }

    /// Test / advanced hook: build a fetcher pointing at a custom RPC
    /// endpoint. Production callers use [`Self::new`].
    pub fn with_rpc_url(rpc_url: String) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // No redirects: a 30x bypasses `validate_uri`'s host check.
            .redirect(Policy::limited(0))
            .user_agent(USER_AGENT)
            .build()?;
        let cache = FutureCache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(CACHE_CAPACITY)
            .build();
        Ok(Self {
            client,
            rpc_url: Arc::new(rpc_url),
            cache,
        })
    }

    /// Resolve `(contract_id, token_id)` → metadata JSON.
    ///
    /// Mirrors `Sep1Fetcher::fetch`: `Ok(Some(json))` on success
    /// (JSON-metadata or synthesised `{"image": …}` shape for the
    /// direct-image convention); `Err(Arc<NftTokenUriError>)` on any
    /// failure path. The api detail handler folds errors fail-soft to
    /// `null` via `.ok().flatten()`; the worker classifies via
    /// [`super::errors::is_transient`] for SQS-retry-vs-sentinel-write.
    ///
    /// `moka::try_get_with` caches `Ok` only — neither transient nor
    /// permanent errors poison the slot. The trade-off is that a
    /// broken NFT may re-enter `fetch_uncached` on repeat traffic; in
    /// exchange we keep observability (every permanent fail logs at
    /// the worker call site) and self-healing (a flaky 4xx from an
    /// IPFS gateway is re-fetched on the next attempt instead of
    /// being cemented for the cache TTL).
    #[instrument(skip(self), fields(contract_id = %contract_id, token_id = %token_id))]
    pub async fn resolve(
        &self,
        contract_id: &str,
        token_id: &str,
    ) -> Result<Option<Value>, Arc<NftTokenUriError>> {
        let key = format!("{contract_id}:{token_id}");
        let client = self.client.clone();
        let rpc_url = Arc::clone(&self.rpc_url);
        let contract_id = contract_id.to_owned();
        let token_id = token_id.to_owned();

        let cached = self
            .cache
            .try_get_with(key, async move {
                let json = fetch_uncached(&client, &rpc_url, &contract_id, &token_id).await?;
                Ok::<_, NftTokenUriError>(Arc::new(Some(json)))
            })
            .await?;
        Ok((*cached).clone())
    }
}

/// Cold path: Soroban RPC + HTTP fetch + Content-Type branch.
/// Pulled out of the cache closure so tests can drive it directly.
async fn fetch_uncached(
    client: &reqwest::Client,
    rpc_url: &str,
    contract_id: &str,
    token_id: &str,
) -> Result<Value, NftTokenUriError> {
    let token_u32 = token_id
        .parse::<u32>()
        .map_err(|_| NftTokenUriError::MalformedInput {
            field: "token_id (not u32)",
            value: token_id.to_owned(),
        })?;

    let envelope_b64 = build_simulate_envelope(contract_id, token_u32)?;
    let result_xdr_b64 = simulate_transaction(client, rpc_url, &envelope_b64).await?;
    let uri = decode_token_uri_result(&result_xdr_b64)?;

    validate_uri(&uri)?;
    let url = resolve_ipfs_to_https(&uri);
    let host = host_of(&url).unwrap_or_else(|| url.clone());

    debug!(uri = %uri, url = %url, "nft token_uri resolved; fetching metadata");

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|source| NftTokenUriError::Http {
            host: host.clone(),
            source,
        })?;
    let status = resp.status();
    if !status.is_success() {
        let err = resp
            .error_for_status()
            .expect_err("non-success status guaranteed to map to Err");
        return Err(NftTokenUriError::Http { host, source: err });
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type.contains("application/json") || content_type.contains("text/json") {
        let bytes = capped_body(resp, &host).await?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(NftTokenUriError::MalformedJson)?;
        Ok(value)
    } else if content_type.starts_with("image/") {
        // Direct-image convention (e.g. JamesBachini Soroban example):
        // `token_uri()` returns the image binary URL directly, no JSON
        // wrapper. Synthesise `{ "image": "<url>" }` so the worker's
        // extract_columns + the api detail handler see a uniform shape.
        // `name` / `collection_name` are legitimately absent in source.
        Ok(json!({ "image": url }))
    } else {
        Err(NftTokenUriError::UnsupportedContentType(content_type))
    }
}

/// Build base64-encoded `TransactionEnvelope` for `token_uri(token_id_u32)`.
/// Source account, fee, seq_num are dummy — simulate path ignores them.
fn build_simulate_envelope(
    contract_id: &str,
    token_id_u32: u32,
) -> Result<String, NftTokenUriError> {
    let contract = stellar_strkey::Contract::from_string(contract_id).map_err(|_| {
        NftTokenUriError::MalformedInput {
            field: "contract_id strkey",
            value: contract_id.to_owned(),
        }
    })?;
    let contract_address = ScAddress::Contract(ContractId(Hash(contract.0)));
    let function_name = ScSymbol(StringM::try_from(TOKEN_URI_FN.as_bytes().to_vec())?);
    let args: VecM<ScVal> = vec![ScVal::U32(token_id_u32)].try_into()?;

    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address,
                function_name,
                args,
            }),
            auth: VecM::default(),
        }),
    };

    let tx = Transaction {
        source_account: MuxedAccount::Ed25519(Uint256([0u8; 32])),
        fee: 100,
        seq_num: SequenceNumber(0),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: vec![op].try_into()?,
        ext: TransactionExt::V0,
    };

    let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    });

    let xdr = envelope.to_xdr(Limits::none())?;
    Ok(BASE64.encode(xdr))
}

/// POST `simulateTransaction`, return base64 `result.results[0].xdr`.
async fn simulate_transaction(
    client: &reqwest::Client,
    rpc_url: &str,
    envelope_b64: &str,
) -> Result<String, NftTokenUriError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "simulateTransaction",
        "params": {
            "transaction": envelope_b64,
            "xdrFormat": "base64",
        },
    });
    let host = host_of(rpc_url).unwrap_or_else(|| rpc_url.to_owned());
    let resp = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|source| NftTokenUriError::Http {
            host: host.clone(),
            source,
        })?;
    let status = resp.status();
    if !status.is_success() {
        let err = resp
            .error_for_status()
            .expect_err("non-success status guaranteed to map to Err");
        return Err(NftTokenUriError::Http { host, source: err });
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|source| NftTokenUriError::Http { host, source })?;

    if let Some(err) = body.get("error") {
        return Err(NftTokenUriError::SorobanRpc(err.to_string()));
    }
    let result = body
        .get("result")
        .ok_or_else(|| NftTokenUriError::MalformedRpcResponse("missing result".into()))?;
    // Contract-side errors land in `result.error` (not top-level).
    if let Some(err) = result.get("error").and_then(Value::as_str) {
        return Err(NftTokenUriError::SorobanRpc(err.to_owned()));
    }
    result
        .get("results")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|r| r.get("xdr"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| NftTokenUriError::MalformedRpcResponse("missing results[0].xdr".into()))
}

/// Decode base64 ScVal XDR into URI string. Only `ScVal::String` is
/// accepted: `ScSymbol` is limited to 32 bytes in XDR and cannot hold a
/// realistic URI, and any other variant is a producer-side contract bug.
fn decode_token_uri_result(xdr_b64: &str) -> Result<String, NftTokenUriError> {
    let raw = BASE64
        .decode(xdr_b64)
        .map_err(|e| NftTokenUriError::MalformedRpcResponse(format!("xdr base64: {e}")))?;
    let bytes = match ScVal::from_xdr(&raw, Limits::none())? {
        ScVal::String(ScString(s)) => s.into_vec(),
        other => {
            return Err(NftTokenUriError::MalformedRpcResponse(format!(
                "token_uri returned non-String ScVal: {other:?}"
            )));
        }
    };
    String::from_utf8(bytes)
        .map_err(|e| NftTokenUriError::MalformedRpcResponse(format!("token_uri not UTF-8: {e}")))
}

/// `ipfs://...` → `https://<gateway>/ipfs/...`; HTTPS passes through.
pub(super) fn resolve_ipfs_to_https(uri: &str) -> String {
    uri.strip_prefix("ipfs://")
        .map(|rest| format!("{IPFS_GATEWAY_BASE}{rest}"))
        .unwrap_or_else(|| uri.to_owned())
}

/// Extract bare host from `https://host[:port]/...` for error attribution.
fn host_of(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_owned())
}

/// URI safety check: only `https://` (RFC1035 host, no IP literal /
/// userinfo) and `ipfs://` (non-empty CID) pass.
pub(super) fn validate_uri(uri: &str) -> Result<(), NftTokenUriError> {
    let uri = uri.trim();
    let bad = || NftTokenUriError::MalformedUri {
        uri: uri.to_owned(),
    };
    if uri.is_empty() {
        return Err(bad());
    }
    let host = if let Some(rest) = uri.strip_prefix("https://") {
        rest
    } else if let Some(rest) = uri.strip_prefix("ipfs://") {
        if rest.is_empty() {
            return Err(bad());
        }
        // Reject path-traversal segments — a contract returning
        // `ipfs://Qm../../etc/passwd` (or percent-encoded variants)
        // could trick a misbehaving gateway into serving an unrelated
        // file. The gateway is the last line of defence, but rejecting
        // up-front keeps the contract-vs-our-validator boundary clean.
        let lowered = rest.to_ascii_lowercase();
        if lowered.contains("%2e%2e") || rest.split('/').any(|seg| seg == ".." || seg == ".") {
            return Err(bad());
        }
        return Ok(());
    } else {
        return Err(NftTokenUriError::UnsafeScheme {
            uri: uri.to_owned(),
        });
    };
    let authority = host.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err(bad()); // userinfo masks the host check
    }
    let host_only = authority.split(':').next().unwrap_or("");
    if host_only.is_empty() {
        return Err(bad());
    }
    if !host_only
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(bad());
    }
    if host_only.parse::<IpAddr>().is_ok() {
        return Err(bad());
    }
    if !host_only.contains('.') {
        return Err(bad()); // reject `localhost` etc. — must be public DNS
    }
    Ok(())
}

/// Stream body, bail out if cumulative size > `MAX_BODY_BYTES`.
pub(super) async fn capped_body(
    mut resp: reqwest::Response,
    host: &str,
) -> Result<Vec<u8>, NftTokenUriError> {
    let mut buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    loop {
        match resp.chunk().await {
            Ok(Some(chunk)) => {
                if buf.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
                    return Err(NftTokenUriError::BodyTooLarge {
                        limit: MAX_BODY_BYTES,
                    });
                }
                buf.extend_from_slice(&chunk);
            }
            Ok(None) => return Ok(buf),
            Err(source) => {
                return Err(NftTokenUriError::Http {
                    host: host.to_owned(),
                    source,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_uri_accepts_https() {
        assert!(validate_uri("https://example.com/123.json").is_ok());
        assert!(validate_uri("https://gateway.pinata.cloud/ipfs/Qm.../1.json").is_ok());
    }

    #[test]
    fn validate_uri_accepts_ipfs() {
        assert!(validate_uri("ipfs://QmXyZ...").is_ok());
    }

    #[test]
    fn validate_uri_rejects_http() {
        assert!(matches!(
            validate_uri("http://example.com/1.json"),
            Err(NftTokenUriError::UnsafeScheme { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_file_scheme() {
        assert!(matches!(
            validate_uri("file:///etc/passwd"),
            Err(NftTokenUriError::UnsafeScheme { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_data_uri() {
        assert!(matches!(
            validate_uri("data:application/json,{}"),
            Err(NftTokenUriError::UnsafeScheme { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_javascript() {
        assert!(matches!(
            validate_uri("javascript:alert(1)"),
            Err(NftTokenUriError::UnsafeScheme { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_ip_literal_v4() {
        assert!(matches!(
            validate_uri("https://127.0.0.1/1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("https://169.254.169.254/latest/meta-data/"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_userinfo() {
        assert!(matches!(
            validate_uri("https://user:pass@evil.example/1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_empty() {
        assert!(matches!(
            validate_uri(""),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("   "),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_ipfs_path_traversal() {
        assert!(matches!(
            validate_uri("ipfs://Qm../../etc/passwd"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("ipfs://QmFoo/../../1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("ipfs://QmFoo/%2e%2e/1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("ipfs://QmFoo/./1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
    }

    #[test]
    fn validate_uri_rejects_no_dot_host() {
        // Wiremock + tests can still target IP literals (rejected above)
        // or the host with an explicit FQDN. Bare `localhost` is rejected
        // so a contract returning `https://localhost/…` cannot smuggle
        // a SSRF target past the host-check.
        assert!(matches!(
            validate_uri("https://localhost/1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
    }

    #[test]
    fn resolve_ipfs_swaps_scheme() {
        assert_eq!(
            resolve_ipfs_to_https("ipfs://QmFoo/1.json"),
            "https://cloudflare-ipfs.com/ipfs/QmFoo/1.json"
        );
        assert_eq!(
            resolve_ipfs_to_https("https://example.com/1.json"),
            "https://example.com/1.json"
        );
    }

    #[test]
    fn host_of_extracts_authority() {
        assert_eq!(
            host_of("https://example.com/path"),
            Some("example.com".into())
        );
        assert_eq!(
            host_of("https://example.com:8443/x"),
            Some("example.com".into())
        );
        assert_eq!(host_of("ipfs://Qm.../path"), None);
        assert_eq!(host_of("garbage"), None);
    }

    #[test]
    fn build_envelope_roundtrip_decodes_token_id() {
        // Verify the envelope builder produces XDR that decodes back to
        // the same InvokeContract args. Uses a synthetic strkey so the
        // test doesn't depend on any live contract id.
        let contract = stellar_strkey::Contract([0xAB; 32]).to_string();
        let envelope_b64 = build_simulate_envelope(&contract, 4521).expect("build ok");
        let raw = BASE64.decode(&envelope_b64).expect("base64 decode");
        let env = TransactionEnvelope::from_xdr(&raw, Limits::none()).expect("xdr roundtrip");
        let TransactionEnvelope::Tx(v1) = env else {
            panic!("expected V1 envelope");
        };
        let op = v1.tx.operations.first().expect("one op");
        let OperationBody::InvokeHostFunction(invoke) = &op.body else {
            panic!("expected InvokeHostFunction");
        };
        let HostFunction::InvokeContract(args) = &invoke.host_function else {
            panic!("expected InvokeContract");
        };
        assert_eq!(args.args.len(), 1);
        match &args.args[0] {
            ScVal::U32(n) => assert_eq!(*n, 4521),
            other => panic!("expected ScVal::U32, got {other:?}"),
        }
        assert_eq!(args.function_name.0.as_slice(), TOKEN_URI_FN.as_bytes());
    }

    #[test]
    fn decode_result_handles_scval_string() {
        let uri = b"ipfs://QmTest/4521.json";
        let scval = ScVal::String(ScString(StringM::try_from(uri.to_vec()).unwrap()));
        let b64 = BASE64.encode(scval.to_xdr(Limits::none()).unwrap());
        assert_eq!(
            decode_token_uri_result(&b64).unwrap(),
            "ipfs://QmTest/4521.json"
        );
    }

    #[test]
    fn decode_result_rejects_non_string_scval() {
        let scval = ScVal::U32(42);
        let b64 = BASE64.encode(scval.to_xdr(Limits::none()).unwrap());
        assert!(matches!(
            decode_token_uri_result(&b64),
            Err(NftTokenUriError::MalformedRpcResponse(_))
        ));
    }

    // wiremock-driven JSON-RPC tests. End-to-end metadata-URL fetch
    // not wiremocked: `validate_uri` rejects loopback hosts on purpose.

    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn scval_string_b64(uri: &str) -> String {
        let scval = ScVal::String(ScString(
            StringM::try_from(uri.as_bytes().to_vec()).unwrap(),
        ));
        BASE64.encode(scval.to_xdr(Limits::none()).unwrap())
    }

    #[tokio::test]
    async fn simulate_transaction_happy_path() {
        let mock = MockServer::start().await;
        let xdr_b64 = scval_string_b64("ipfs://QmDeadBeef/4521.json");

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(serde_json::json!({
                "method": "simulateTransaction",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "latestLedger": 100,
                    "minResourceFee": "0",
                    "results": [{ "auth": [], "xdr": xdr_b64 }],
                },
            })))
            .mount(&mock)
            .await;

        let client = reqwest::Client::new();
        let envelope = "AAAA".to_owned(); // body content irrelevant; mock matches by method
        let got = simulate_transaction(&client, &mock.uri(), &envelope)
            .await
            .expect("happy path");
        // RPC layer returns the raw xdr_b64; ScVal decode is a separate fn.
        assert_eq!(
            decode_token_uri_result(&got).unwrap(),
            "ipfs://QmDeadBeef/4521.json"
        );
    }

    #[tokio::test]
    async fn simulate_transaction_jsonrpc_error() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": { "code": -32600, "message": "invalid request" },
            })))
            .mount(&mock)
            .await;

        let client = reqwest::Client::new();
        let err = simulate_transaction(&client, &mock.uri(), "AAAA")
            .await
            .expect_err("JSON-RPC error must propagate");
        assert!(matches!(err, NftTokenUriError::SorobanRpc(_)));
    }

    #[tokio::test]
    async fn simulate_transaction_contract_revert() {
        // RPC server returns 200 but result.error indicates a contract revert.
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "latestLedger": 100,
                    "error": "host fn error: missing entry",
                },
            })))
            .mount(&mock)
            .await;

        let client = reqwest::Client::new();
        let err = simulate_transaction(&client, &mock.uri(), "AAAA")
            .await
            .expect_err("contract-side error must propagate");
        assert!(matches!(err, NftTokenUriError::SorobanRpc(_)));
    }

    #[tokio::test]
    async fn simulate_transaction_missing_results_array() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": { "latestLedger": 100 },
            })))
            .mount(&mock)
            .await;

        let client = reqwest::Client::new();
        let err = simulate_transaction(&client, &mock.uri(), "AAAA")
            .await
            .expect_err("malformed response must surface");
        assert!(matches!(err, NftTokenUriError::MalformedRpcResponse(_)));
    }

    #[tokio::test]
    async fn simulate_transaction_5xx_is_http_error() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock)
            .await;

        let client = reqwest::Client::new();
        let err = simulate_transaction(&client, &mock.uri(), "AAAA")
            .await
            .expect_err("5xx must surface as Http");
        let NftTokenUriError::Http { source, .. } = &err else {
            panic!("expected Http variant, got {err:?}");
        };
        assert_eq!(source.status().map(|s| s.as_u16()), Some(503));
        assert!(super::super::errors::is_transient(&err));
    }

    #[tokio::test]
    async fn resolve_propagates_permanent_error_as_err() {
        // Permanent fails (MalformedInput here) must surface as Err so
        // the worker call site can warn-log every occurrence. `moka`'s
        // `try_get_with` does not cache Err, so a repeat call re-enters
        // `fetch_uncached` — observability + self-healing over the
        // sub-ms cache-hit savings of a negative cache.
        let fetcher = NftTokenUriFetcher::with_rpc_url("http://unused".to_owned()).expect("build");
        let err = fetcher
            .resolve("not-a-strkey", "42")
            .await
            .expect_err("permanent fail must propagate as Err");
        assert!(matches!(*err, NftTokenUriError::MalformedInput { .. }));
        // Repeat call: must also propagate Err (not silently cached).
        let err2 = fetcher
            .resolve("not-a-strkey", "42")
            .await
            .expect_err("repeat permanent fail must still propagate");
        assert!(matches!(*err2, NftTokenUriError::MalformedInput { .. }));
    }

    #[tokio::test]
    async fn fetch_uncached_rejects_non_u32_token_id() {
        // Pure structural check — `fetch_uncached` short-circuits before
        // any network call when token_id isn't a u32.
        let client = reqwest::Client::new();
        let err = super::fetch_uncached(&client, "http://unused", "C...", "not-a-number")
            .await
            .expect_err("non-u32 token_id must hard-fail");
        assert!(matches!(
            err,
            NftTokenUriError::MalformedInput { field, .. } if field.contains("token_id")
        ));
    }

    #[tokio::test]
    async fn fetch_uncached_rejects_bad_contract_strkey() {
        let client = reqwest::Client::new();
        let err = super::fetch_uncached(&client, "http://unused", "not-a-strkey", "42")
            .await
            .expect_err("malformed contract strkey must hard-fail");
        assert!(matches!(
            err,
            NftTokenUriError::MalformedInput { field, .. } if field.contains("contract_id")
        ));
    }

    // Live mainnet smoke. Default-ignored — hits SDF public RPC. Run:
    //   cargo test -p enrichment-shared --lib live_mainnet -- --ignored --nocapture

    const LIVE_RPC_URL: &str = "https://mainnet.sorobanrpc.com";
    const LIVE_NFT_CONTRACT: &str = "CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY";

    /// 0-arg variant: JamesBachini's tutorial contract defines
    /// `token_uri(env) -> String` (no caller args). The default builder
    /// sends 1 arg per ERC-721, so we craft a 0-arg envelope here.
    fn build_zero_arg_envelope(contract_id: &str) -> Result<String, NftTokenUriError> {
        let contract = stellar_strkey::Contract::from_string(contract_id).map_err(|_| {
            NftTokenUriError::MalformedInput {
                field: "contract_id strkey",
                value: contract_id.to_owned(),
            }
        })?;
        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(ContractId(Hash(contract.0))),
                    function_name: ScSymbol(StringM::try_from(TOKEN_URI_FN.as_bytes().to_vec())?),
                    args: VecM::default(),
                }),
                auth: VecM::default(),
            }),
        };
        let tx = Transaction {
            source_account: MuxedAccount::Ed25519(Uint256([0u8; 32])),
            fee: 100,
            seq_num: SequenceNumber(0),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![op].try_into()?,
            ext: TransactionExt::V0,
        };
        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx,
            signatures: VecM::default(),
        });
        Ok(BASE64.encode(envelope.to_xdr(Limits::none())?))
    }

    #[tokio::test]
    #[ignore = "hits live SDF mainnet RPC; run with --ignored"]
    async fn live_mainnet_zero_arg_token_uri_success() {
        let client = reqwest::Client::new();
        let envelope = build_zero_arg_envelope(LIVE_NFT_CONTRACT).expect("envelope");
        let xdr_b64 = simulate_transaction(&client, LIVE_RPC_URL, &envelope)
            .await
            .expect("RPC call must succeed for known-good 0-arg token_uri");
        let uri = decode_token_uri_result(&xdr_b64).expect("ScVal::String decode");
        eprintln!("✓ live mainnet token_uri returned: {uri}");
        assert!(!uri.is_empty(), "URI must be non-empty");
        // Only the schemes `validate_uri` actually accepts —
        // tightened from a wider list that briefly allowed bare CID
        // prefixes (`Qm…`, `bafy…`) which would never pass production
        // validation.
        assert!(
            uri.starts_with("https://") || uri.starts_with("ipfs://"),
            "URI scheme not recognised: {uri}"
        );
    }
}
