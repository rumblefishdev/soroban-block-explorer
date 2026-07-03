//! `NftTokenUriFetcher` — LRU-cached Soroban RPC + HTTP/IPFS client.
//!
//! Pipeline: `simulateTransaction(InvokeContract(token_uri, [ScVal::U32]))`
//! → ScVal::String → `validate_uri` → `ipfs://` to gateway → HTTP GET
//! → Content-Type branch. See `super::mod` for the side-by-side with
//! SEP-1, the rationale for source-naming, and the defensive-guard list.

use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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

use super::errors::{NftTokenUriError, is_endpoint_fault};

/// Body cap for NFT metadata JSON (typical files <10 KB).
pub(super) const MAX_BODY_BYTES: usize = 256 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_CAPACITY: u64 = 1024;
const USER_AGENT: &str = concat!("soroban-block-explorer/", env!("CARGO_PKG_VERSION"));
/// SDF public mainnet RPC — the single default when no `SOROBAN_RPC_URLS` /
/// `SOROBAN_RPC_URL` env is set. The fetcher rotates + fails over across the
/// whole pool when given more (task 0311).
pub(super) const DEFAULT_SOROBAN_RPC_URL: &str = "https://mainnet.sorobanrpc.com";
/// Default IPFS gateways, tried in order with failover. Both serve path-style
/// `/ipfs/<CID>` with HTTP 200 (no redirect — required by our
/// `Policy::limited(0)` SSRF guard) and are reachable from the prod box
/// (task 0311 sieve, 2026-06-22). The prior single default
/// `cloudflare-ipfs.com` was sunset by Cloudflare → dead.
pub(super) const DEFAULT_IPFS_GATEWAYS: &[&str] = &[
    "https://ipfs.io/ipfs/",
    "https://gateway.pinata.cloud/ipfs/",
];

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
    /// RPC endpoint pool — round-robin + failover (task 0311). A single
    /// element = the historical single-RPC behaviour.
    rpc_urls: Arc<Vec<String>>,
    /// IPFS gateway pool — round-robin + failover for `ipfs://` token_uris.
    ipfs_gateways: Arc<Vec<String>>,
    /// Round-robin start cursor — spreads each request's first pick across the
    /// pools so no single endpoint is hammered first (proactive 429 avoidance).
    cursor: Arc<AtomicUsize>,
    cache: FutureCache<String, Arc<Option<Value>>>,
}

/// Parse a comma-separated env var into a trimmed, non-empty list. `None` if
/// unset or all-empty.
fn env_list(key: &str) -> Option<Vec<String>> {
    let raw = std::env::var(key).ok()?;
    let list: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    (!list.is_empty()).then_some(list)
}

impl NftTokenUriFetcher {
    /// Production constructor. RPC pool from `SOROBAN_RPC_URLS` (comma-sep) →
    /// single `SOROBAN_RPC_URL` → SDF default; IPFS gateway pool from
    /// `IPFS_GATEWAY_BASES` (comma-sep) → [`DEFAULT_IPFS_GATEWAYS`]. With no env
    /// set, behaviour is identical to the historical single-SDF-RPC fetcher.
    pub fn new() -> Result<Self, reqwest::Error> {
        let rpc_urls = env_list("SOROBAN_RPC_URLS")
            .or_else(|| std::env::var("SOROBAN_RPC_URL").ok().map(|u| vec![u]))
            .unwrap_or_else(|| vec![DEFAULT_SOROBAN_RPC_URL.to_owned()]);
        let ipfs_gateways = env_list("IPFS_GATEWAY_BASES").unwrap_or_else(|| {
            DEFAULT_IPFS_GATEWAYS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });
        Self::build(rpc_urls, ipfs_gateways)
    }

    /// Test / advanced hook: a single RPC endpoint + the default IPFS gateways.
    pub fn with_rpc_url(rpc_url: String) -> Result<Self, reqwest::Error> {
        Self::build(
            vec![rpc_url],
            DEFAULT_IPFS_GATEWAYS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )
    }

    /// Test / advanced hook: explicit RPC pool + IPFS gateway pool.
    pub fn with_pools(
        rpc_urls: Vec<String>,
        ipfs_gateways: Vec<String>,
    ) -> Result<Self, reqwest::Error> {
        Self::build(rpc_urls, ipfs_gateways)
    }

    fn build(rpc_urls: Vec<String>, ipfs_gateways: Vec<String>) -> Result<Self, reqwest::Error> {
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
            rpc_urls: Arc::new(rpc_urls),
            ipfs_gateways: Arc::new(ipfs_gateways),
            cursor: Arc::new(AtomicUsize::new(0)),
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
        let rpc_urls = Arc::clone(&self.rpc_urls);
        let ipfs_gateways = Arc::clone(&self.ipfs_gateways);
        // One round-robin tick per request → spreads each request's first pick.
        let start = self.cursor.fetch_add(1, Ordering::Relaxed);
        let contract_id = contract_id.to_owned();
        let token_id = token_id.to_owned();

        let cached = self
            .cache
            .try_get_with(key, async move {
                let json = fetch_uncached(
                    &client,
                    &rpc_urls,
                    &ipfs_gateways,
                    start,
                    &contract_id,
                    &token_id,
                )
                .await?;
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
    rpc_urls: &[String],
    ipfs_gateways: &[String],
    start: usize,
    contract_id: &str,
    token_id: &str,
) -> Result<Value, NftTokenUriError> {
    let token_u32 = token_id
        .parse::<u32>()
        .map_err(|_| NftTokenUriError::MalformedInput {
            field: "token_id (not u32)",
            value: token_id.to_owned(),
        })?;

    let result_xdr_b64 =
        simulate_with_failover(client, rpc_urls, start, contract_id, token_u32).await?;
    let uri = decode_token_uri_result(&result_xdr_b64)?;

    validate_uri(&uri)?;
    debug!(uri = %uri, "nft token_uri resolved; fetching metadata");
    fetch_metadata_with_failover(client, &uri, ipfs_gateways, start).await
}

/// Try each RPC in the pool (round-robin from `start`) until one answers.
/// Advances on endpoint faults (429 / 5xx / timeout / connect / a rate-limit
/// JSON-RPC error); returns immediately on a deterministic contract/parse error
/// (identical on every endpoint) or success.
async fn simulate_with_failover(
    client: &reqwest::Client,
    rpc_urls: &[String],
    start: usize,
    contract_id: &str,
    token_id_u32: u32,
) -> Result<String, NftTokenUriError> {
    let n = rpc_urls.len();
    let mut last: Option<NftTokenUriError> = None;
    for k in 0..n {
        let url = &rpc_urls[(start + k) % n];
        match simulate_token_uri_with_fallback(client, url, contract_id, token_id_u32).await {
            Ok(xdr) => return Ok(xdr),
            Err(e) if is_endpoint_fault(&e) => {
                debug!(rpc = %url, error = %e, "rpc endpoint fault — failing over");
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.expect("rpc pool is non-empty, so the loop body ran"))
}

/// Resolve the `token_uri` value to metadata JSON, rotating IPFS gateways with
/// failover. An `ipfs://` URI has one candidate per gateway (content-addressed
/// → identical bytes); an `https://` URI has a single candidate. Advances only
/// on endpoint faults; a deterministic content error (unsupported type,
/// malformed JSON) repeats on every gateway, so it returns immediately.
async fn fetch_metadata_with_failover(
    client: &reqwest::Client,
    uri: &str,
    ipfs_gateways: &[String],
    start: usize,
) -> Result<Value, NftTokenUriError> {
    let candidates = ipfs_candidate_urls(uri, ipfs_gateways, start);
    let mut last: Option<NftTokenUriError> = None;
    for url in &candidates {
        match fetch_one_metadata(client, url).await {
            Ok(v) => return Ok(v),
            Err(e) if is_endpoint_fault(&e) => {
                debug!(gateway = %url, error = %e, "ipfs gateway fault — failing over");
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| NftTokenUriError::MalformedUri {
        uri: uri.to_owned(),
    }))
}

/// Ordered candidate URLs for a validated `token_uri` value. `ipfs://<rest>` →
/// one URL per gateway (round-robin from `start`); `https://…` → the single
/// direct URL (a specific host — no rotation).
fn ipfs_candidate_urls(uri: &str, gateways: &[String], start: usize) -> Vec<String> {
    match uri.strip_prefix("ipfs://") {
        Some(rest) if !gateways.is_empty() => {
            let n = gateways.len();
            (0..n)
                .map(|k| format!("{}{rest}", gateways[(start + k) % n]))
                .collect()
        }
        _ => vec![uri.to_owned()],
    }
}

/// Single metadata GET: status check (3xx → `HttpStatus`, never panics),
/// content-type branch (JSON vs direct-image), capped body.
async fn fetch_one_metadata(
    client: &reqwest::Client,
    url: &str,
) -> Result<Value, NftTokenUriError> {
    let host = host_of(url).unwrap_or_else(|| url.to_owned());
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|source| NftTokenUriError::Http {
            host: host.clone(),
            source,
        })?;
    let status = resp.status();
    if !status.is_success() {
        return Err(non_success_error(resp, status, host));
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

/// Map a non-2xx response to an error WITHOUT panicking on 3xx. `reqwest`'s
/// `error_for_status()` only errors on 4xx/5xx, so a 3xx (redirect — we run
/// `Policy::limited(0)`) would make a bare `.expect_err()` panic. 3xx →
/// `HttpStatus` (failover-worthy: the gateway redirects, try the next); 4xx/5xx
/// → `Http` (preserves the reqwest-error-carrying variant + its transient
/// classification).
fn non_success_error(
    resp: reqwest::Response,
    status: reqwest::StatusCode,
    host: String,
) -> NftTokenUriError {
    match resp.error_for_status() {
        // 3xx (and any other non-2xx reqwest declines to flag) → status-only.
        Ok(_redirect) => NftTokenUriError::HttpStatus {
            host,
            status: status.as_u16(),
        },
        Err(source) => NftTokenUriError::Http { host, source },
    }
}

/// Build base64-encoded `TransactionEnvelope` for `token_uri(token_id_u32)`
/// (SEP-50 / OpenZeppelin convention) or `token_uri()` (SEP-39 /
/// ERC-721 collection-wide convention) when `token_id_u32` is `None`.
/// Source account, fee, seq_num are dummy — simulate path ignores them.
fn build_simulate_envelope(
    contract_id: &str,
    token_id_u32: Option<u32>,
) -> Result<String, NftTokenUriError> {
    let contract = stellar_strkey::Contract::from_string(contract_id).map_err(|_| {
        NftTokenUriError::MalformedInput {
            field: "contract_id strkey",
            value: contract_id.to_owned(),
        }
    })?;
    let contract_address = ScAddress::Contract(ContractId(Hash(contract.0)));
    let function_name = ScSymbol(StringM::try_from(TOKEN_URI_FN.as_bytes().to_vec())?);
    let args: VecM<ScVal> = match token_id_u32 {
        Some(id) => vec![ScVal::U32(id)].try_into()?,
        None => VecM::default(),
    };

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

/// Invoke `token_uri` and return the raw base64 ScVal XDR.
///
/// Real-world Soroban NFTs split between two conventions for the
/// function signature:
///
/// - **SEP-50 / OpenZeppelin**: `token_uri(token_id) -> String` —
///   per-token URI. Most modern contracts.
/// - **SEP-39 / ERC-721 style**: `token_uri() -> String` —
///   collection-wide URI. Older contracts (e.g. the James Bachini
///   `SorobanNFT` contract found on pubnet during the 2026-05-13
///   audit, Bug #5).
///
/// Try the per-token form first; on
/// [`is_token_uri_arity_mismatch`] fall back to the zero-arg form.
/// Any other RPC error propagates unchanged.
///
/// See `docs/audits/2026-05-13-0197-step0/2026-05-13-pre-audit-finding-token-uri-signature-mismatch.md`
/// for the audit-time fixture + rationale.
///
/// TODO(audit-0197 follow-up): replace the try/fallback with
/// WASM-spec-driven dispatch — inspect the contract's interface
/// (in `wasm_interface_metadata.metadata` JSONB) to learn
/// `token_uri`'s arity ahead of time and call the right variant
/// directly. Saves one RPC round-trip per SEP-39 token (a SEP-39
/// collection with N tokens currently spends 2 × N RPC calls; with
/// spec dispatch it spends N). Prerequisites surfaced by 0197 Step 1:
///   1. `soroban_contracts.wasm_hash` is reliably populated for
///      non-SAC contracts — currently 99.9 % NULL (Step 1 Finding F9;
///      same root cause class as Bug #4 SAC-detection gap).
///   2. `wasm_interface_metadata.metadata` is populated with a real
///      `functions[]` array — locally 40 % of audited rows store
///      `{}` because the parser produced no spec from the WASM
///      bytecode (Step 1 Finding F8).
///   3. `xdr-parser::classification` exposes function arity, not
///      just presence-by-name.
///   4. Fallback retained for contracts where WASM bytecode is no
///      longer reachable via RPC (state-pruning past the retention
///      window).
///
/// Priority: low — fallback is functional. Optimisation, not
/// correctness.
async fn simulate_token_uri_with_fallback(
    client: &reqwest::Client,
    rpc_url: &str,
    contract_id: &str,
    token_id_u32: u32,
) -> Result<String, NftTokenUriError> {
    let per_token_envelope = build_simulate_envelope(contract_id, Some(token_id_u32))?;
    match simulate_transaction(client, rpc_url, &per_token_envelope).await {
        Ok(xdr) => Ok(xdr),
        Err(NftTokenUriError::SorobanRpc(msg)) if is_token_uri_arity_mismatch(&msg) => {
            debug!(
                contract_id = %contract_id,
                "token_uri(token_id) returned arity mismatch; retrying zero-arg token_uri() (SEP-39 contracts)"
            );
            let collection_envelope = build_simulate_envelope(contract_id, None)?;
            simulate_transaction(client, rpc_url, &collection_envelope).await
        }
        Err(other) => Err(other),
    }
}

/// Soroban VM signals "function exists but arity differs from the
/// caller" via `Func(MismatchingParameterLen)` inside the RPC
/// HostError. That is the signal we use to drop to the SEP-39
/// zero-arg variant. Other RPC errors do **not** trigger the
/// fallback — they propagate.
fn is_token_uri_arity_mismatch(rpc_error_msg: &str) -> bool {
    rpc_error_msg.contains("MismatchingParameterLen")
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
        return Err(non_success_error(resp, status, host));
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

/// `ipfs://...` → `https://<primary-gateway>/ipfs/...`; HTTPS passes through.
/// Single-gateway resolution (the primary of [`DEFAULT_IPFS_GATEWAYS`]) for the
/// `image`-field path in `enrich_and_persist::nft_token_uri`; the `token_uri`
/// metadata fetch itself rotates the full pool via `fetch_metadata_with_failover`.
pub(crate) fn resolve_ipfs_to_https(uri: &str) -> String {
    uri.strip_prefix("ipfs://")
        .map(|rest| format!("{}{rest}", DEFAULT_IPFS_GATEWAYS[0]))
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
        // Decode `%2e` (any case) → `.` first so mixed encodings like
        // `.%2e`, `%2e.`, `%2e/` collapse to literal-dot segments before
        // the per-segment match.
        let normalized = rest.to_ascii_lowercase().replace("%2e", ".");
        if normalized.split('/').any(|seg| seg == ".." || seg == ".") {
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
        // Mixed-encoding traversals — fully-encoded `%2e%2e`, partially-
        // encoded `.%2e` / `%2e.`, single-encoded `%2e` (literal dot).
        assert!(matches!(
            validate_uri("ipfs://QmFoo/.%2e/1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("ipfs://QmFoo/%2e./1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("ipfs://QmFoo/%2E%2E/1.json"),
            Err(NftTokenUriError::MalformedUri { .. })
        ));
        assert!(matches!(
            validate_uri("ipfs://QmFoo/%2e/1.json"),
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
            "https://ipfs.io/ipfs/QmFoo/1.json"
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
        let envelope_b64 = build_simulate_envelope(&contract, Some(4521)).expect("build ok");
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
        let err = super::fetch_uncached(
            &client,
            &["http://unused".to_owned()],
            &["https://gw/ipfs/".to_owned()],
            0,
            "C...",
            "not-a-number",
        )
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
        let err = super::fetch_uncached(
            &client,
            &["http://unused".to_owned()],
            &["https://gw/ipfs/".to_owned()],
            0,
            "not-a-strkey",
            "42",
        )
        .await
        .expect_err("malformed contract strkey must hard-fail");
        assert!(matches!(
            err,
            NftTokenUriError::MalformedInput { field, .. } if field.contains("contract_id")
        ));
    }

    // ---- task 0311: multi-provider RPC rotation + failover ----

    #[tokio::test]
    async fn simulate_failover_advances_past_429() {
        // First RPC 429s; the pool must fail over to the healthy second.
        let limited = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&limited)
            .await;
        let healthy = MockServer::start().await;
        let xdr = scval_string_b64("ipfs://QmGood/1.json");
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "latestLedger": 1, "results": [{ "auth": [], "xdr": xdr }] },
            })))
            .mount(&healthy)
            .await;

        let client = reqwest::Client::new();
        let contract = stellar_strkey::Contract([0xCD; 32]).to_string();
        // start=0 → tries `limited` (429) first, fails over to `healthy`.
        let got = super::simulate_with_failover(
            &client,
            &[limited.uri(), healthy.uri()],
            0,
            &contract,
            1,
        )
        .await
        .expect("must fail over from 429 to the healthy RPC");
        assert_eq!(
            decode_token_uri_result(&got).unwrap(),
            "ipfs://QmGood/1.json"
        );
    }

    #[tokio::test]
    async fn simulate_failover_all_429_surfaces_transient() {
        // Whole pool 429s → the exhausted error must classify transient (so the
        // worker requests an SQS retry rather than burning a permanent sentinel).
        let a = MockServer::start().await;
        let b = MockServer::start().await;
        for m in [&a, &b] {
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(429))
                .mount(m)
                .await;
        }
        let client = reqwest::Client::new();
        let contract = stellar_strkey::Contract([0x01; 32]).to_string();
        let err = super::simulate_with_failover(&client, &[a.uri(), b.uri()], 0, &contract, 1)
            .await
            .expect_err("a fully-429 pool must surface an error");
        assert!(
            super::super::errors::is_transient(&err),
            "exhausted-429 pool should be transient, got {err:?}"
        );
    }

    #[tokio::test]
    async fn simulate_failover_stops_on_deterministic_error() {
        // A contract-side revert is identical on every endpoint → must NOT fail
        // over (would waste the whole pool on a permanent fault).
        let first = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "latestLedger": 1, "error": "HostError: Error(Contract, #5)" },
            })))
            .mount(&first)
            .await;
        let never = MockServer::start().await;
        let xdr = scval_string_b64("ipfs://QmNever/1.json");
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "latestLedger": 1, "results": [{ "auth": [], "xdr": xdr }] },
            })))
            .mount(&never)
            .await;

        let client = reqwest::Client::new();
        let contract = stellar_strkey::Contract([0x02; 32]).to_string();
        let err =
            super::simulate_with_failover(&client, &[first.uri(), never.uri()], 0, &contract, 1)
                .await
                .expect_err("deterministic contract error must not fail over");
        assert!(matches!(err, NftTokenUriError::SorobanRpc(_)));
    }

    #[tokio::test]
    async fn metadata_3xx_surfaces_not_panics() {
        // A 3xx from a gateway must surface as a failover-worthy error WITHOUT
        // panicking (the old `error_for_status().expect_err()` panicked on 3xx).
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(301))
            .mount(&mock)
            .await;
        let client = reqwest::Client::builder()
            .redirect(Policy::limited(0))
            .build()
            .unwrap();
        let err = super::fetch_one_metadata(&client, &format!("{}/x", mock.uri()))
            .await
            .expect_err("3xx must surface as error, not panic");
        // Reaching here = no panic. The key property: a 3xx gateway is
        // failover-worthy so the pool advances to the next one.
        assert!(super::super::errors::is_endpoint_fault(&err), "got {err:?}");
    }

    #[test]
    fn ipfs_candidates_rotate_and_passthrough() {
        let gws = vec![
            "https://gw-a/ipfs/".to_owned(),
            "https://gw-b/ipfs/".to_owned(),
        ];
        // ipfs:// → one URL per gateway, order rotated by `start`.
        assert_eq!(
            super::ipfs_candidate_urls("ipfs://QmX/1.json", &gws, 0),
            vec![
                "https://gw-a/ipfs/QmX/1.json",
                "https://gw-b/ipfs/QmX/1.json"
            ]
        );
        assert_eq!(
            super::ipfs_candidate_urls("ipfs://QmX/1.json", &gws, 1),
            vec![
                "https://gw-b/ipfs/QmX/1.json",
                "https://gw-a/ipfs/QmX/1.json"
            ]
        );
        // https:// → single passthrough, no rotation.
        assert_eq!(
            super::ipfs_candidate_urls("https://host.example/1.json", &gws, 0),
            vec!["https://host.example/1.json"]
        );
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
