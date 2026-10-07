//! `NftTokenUriFetcher` — LRU-cached Soroban RPC + HTTP/IPFS client.
//!
//! Pipeline: `simulateTransaction(InvokeContract(token_uri, [ScVal::U32]))`
//! → ScVal::String → `validate_uri` → `ipfs://` to gateway → HTTP GET
//! → Content-Type branch. See `super::mod` for the side-by-side with
//! SEP-1, the rationale for source-naming, and the defensive-guard list.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use moka::future::Cache as FutureCache;
use serde_json::{Value, json};
use stellar_xdr::{
    ContractId, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, ReadXdr, ScAddress, ScString, ScSymbol,
    ScVal, SequenceNumber, StringM, Transaction, TransactionEnvelope, TransactionExt,
    TransactionV1Envelope, Uint256, VecM, WriteXdr,
};
use tracing::{debug, instrument};

use super::errors::{NftTokenUriError, is_endpoint_fault};
use super::validate_uri::validate_uri;

/// Body cap for NFT metadata JSON (typical files <10 KB).
pub(super) const MAX_BODY_BYTES: usize = 256 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_CAPACITY: u64 = 1024;
const USER_AGENT: &str = concat!("soroban-block-explorer/", env!("CARGO_PKG_VERSION"));
/// Default Soroban RPC pool — round-robin + failover-on-transient across all
/// four (task 0311; a single endpoint hits the per-IP 429 wall under
/// enrichment bursts, and every Lambda shares one NAT egress IP). Keyless,
/// in-sync endpoints from the 2026-06-22 box sieve. Lives in code, not env,
/// for the same reason as [`DEFAULT_IPFS_GATEWAYS`]: one good list every
/// consumer (worker, API, backfill CLI) gets by construction — previously
/// only the worker's env carried the pool and the other two silently ran on
/// the single SDF endpoint (lore-0455). `SOROBAN_RPC_URLS` env overrides for
/// ad-hoc runs.
pub(super) const DEFAULT_SOROBAN_RPC_URLS: &[&str] = &[
    "https://mainnet.sorobanrpc.com",
    "https://soroban-rpc.mainnet.stellar.gateway.fm/",
    "https://rpc.ankr.com/stellar_soroban",
    "https://stellar.api.onfinality.io/public",
];
/// Default IPFS gateways, tried in order with failover. Both serve path-style
/// `/ipfs/<CID>` with HTTP 200 in one hop (no redirect needed; since
/// lore-0455 the client follows same-registrable-domain https redirects
/// with a bounded budget, so a trailing-slash `301` no longer loses
/// content) and are reachable from the prod box
/// (task 0311 sieve, 2026-06-22). The prior single default
/// `cloudflare-ipfs.com` was sunset by Cloudflare → dead.
pub(super) const DEFAULT_IPFS_GATEWAYS: &[&str] = &[
    "https://ipfs.io/ipfs/",
    "https://gateway.pinata.cloud/ipfs/",
];

/// `token_uri` is the OpenZeppelin / ERC-721 metadata-extension
/// function name. Stellar Soroban NFT contracts copy the convention.
const TOKEN_URI_FN: &str = "token_uri";

/// SEP-50 contract-level `name()` — the collection name (task 0340). Used as a
/// FALLBACK only: the OZ NFT name lives in instance storage and is captured by
/// the parser into `soroban_contract_metadata` (#330) + served via COALESCE
/// (#331). `name()` covers the ledger-uncovered remainder — hand-rolled
/// contracts with empty instance storage but a WASM-baked `name()`.
const NAME_FN: &str = "name";

/// Char cap for a fetched collection name — a generous bound that only
/// sentinels pathological multi-KB values (mirrors the worker's caps on the
/// `token_uri` JSON fields; the CH column is unbounded `Nullable(String)`).
const MAX_COLLECTION_NAME_CHARS: usize = 4096;

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
    /// Per-CONTRACT cache for the SEP-50 `name()` collection name (task 0340).
    /// Keyed by contract StrKey — `name()` is parameterless, so a full drain
    /// costs one RPC per collection, never per token. `None` = the contract
    /// exports no usable name (a PERMANENT fact, cached to stop a big
    /// collection from re-asking once per token).
    name_cache: FutureCache<String, Arc<Option<String>>>,
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
    /// Production constructor. RPC pool from `SOROBAN_RPC_URLS` (comma-sep,
    /// ad-hoc override) → [`DEFAULT_SOROBAN_RPC_URLS`]; IPFS gateway pool from
    /// `IPFS_GATEWAY_BASES` (comma-sep) → [`DEFAULT_IPFS_GATEWAYS`].
    pub fn new() -> Result<Self, reqwest::Error> {
        let rpc_urls = env_list("SOROBAN_RPC_URLS").unwrap_or_else(|| {
            DEFAULT_SOROBAN_RPC_URLS
                .iter()
                .map(|s| s.to_string())
                .collect()
        });
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
            // Same-registrable-domain redirect policy, shared with SEP-1
            // (lore-0455; measured 2026-08-18). `Policy::limited(0)` was the
            // original SSRF guard, but it also refused a directory CID's
            // harmless `301` to the same host with a trailing slash, losing
            // the content. This policy follows https-only, same-eTLD+1,
            // bounded-hop redirects — recovering that case — and still stops
            // the off-host and per-CID-subdomain shapes the guard exists to
            // block (PSL-listed gateway domains resolve per-CID subdomains to
            // different registrable domains). A stopped redirect surfaces as
            // the 3xx response, which `fetch_one_metadata` already maps to a
            // failover-worthy `HttpStatus`.
            .redirect(crate::sep1::same_etld1_redirect_policy())
            .user_agent(USER_AGENT)
            .build()?;
        let cache = FutureCache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(CACHE_CAPACITY)
            .build();
        let name_cache = FutureCache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(CACHE_CAPACITY)
            .build();
        Ok(Self {
            client,
            rpc_urls: Arc::new(rpc_urls),
            ipfs_gateways: Arc::new(ipfs_gateways),
            cursor: Arc::new(AtomicUsize::new(0)),
            cache,
            name_cache,
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

    /// Resolve the contract-level SEP-50 `name()` → collection name (task 0340).
    ///
    /// Failure model differs from [`Self::resolve`] on purpose: a PERMANENT
    /// "no usable name" (function missing, non-String return, empty/oversize
    /// value) folds to `Ok(None)` — a stable fact about the contract, so it IS
    /// cached, and a 10k-token collection without `name()` costs one RPC per
    /// run instead of one per token. Only transient RPC faults surface as
    /// `Err` (uncached → retried), so the caller's `Err` arm is
    /// retry-classification-free: every `Err` is transient.
    #[instrument(skip(self), fields(contract_id = %contract_id))]
    pub async fn resolve_collection_name(
        &self,
        contract_id: &str,
    ) -> Result<Option<String>, Arc<NftTokenUriError>> {
        let client = self.client.clone();
        let rpc_urls = Arc::clone(&self.rpc_urls);
        let start = self.cursor.fetch_add(1, Ordering::Relaxed);
        let contract = contract_id.to_owned();

        let cached = self
            .name_cache
            .try_get_with(contract_id.to_owned(), async move {
                match simulate_name_with_failover(&client, &rpc_urls, start, &contract).await {
                    // A successful simulate whose retval does not decode to a
                    // usable String — non-String return or malformed XDR — is a
                    // PERMANENT contract fact, not a transient fault. Fold it to
                    // a cached `None` (like the permanent cases below) so a
                    // contract that will never yield a name is not re-fetched and
                    // retried on every token / every run. `decode_string_result`'s
                    // error must NOT `?` out here: the caller treats every `Err`
                    // as transient.
                    Ok(xdr) => match decode_string_result(&xdr, NAME_FN) {
                        Ok(name) => Ok(Arc::new(usable_collection_name(&name))),
                        Err(e) => {
                            debug!(error = %e, "name() returned no usable String — caching 'no name'");
                            Ok(Arc::new(None))
                        }
                    },
                    Err(e) if super::errors::is_transient(&e) => Err(e),
                    Err(e) => {
                        debug!(error = %e, "name() permanent fail — caching 'no name'");
                        Ok(Arc::new(None))
                    }
                }
            })
            .await?;
        Ok((*cached).clone())
    }
}

/// Trim + bound a decoded `name()` value; empty or pathologically long → `None`.
fn usable_collection_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty() && trimmed.chars().count() <= MAX_COLLECTION_NAME_CHARS)
        .then(|| trimmed.to_owned())
}

/// [`simulate_with_failover`]'s zero-arg sibling for `name()` — same endpoint
/// rotation, no arity fallback (`name()` is parameterless in every convention).
async fn simulate_name_with_failover(
    client: &reqwest::Client,
    rpc_urls: &[String],
    start: usize,
    contract_id: &str,
) -> Result<String, NftTokenUriError> {
    let envelope = build_simulate_envelope(contract_id, NAME_FN, None)?;
    let n = rpc_urls.len();
    let mut last: Option<NftTokenUriError> = None;
    for k in 0..n {
        let url = &rpc_urls[(start + k) % n];
        match simulate_transaction(client, url, &envelope).await {
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
    let uri = decode_string_result(&result_xdr_b64, TOKEN_URI_FN)?;

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
/// `error_for_status()` only errors on 4xx/5xx, so a 3xx (a redirect the
/// same-eTLD+1 policy refused to follow) would make a bare `.expect_err()`
/// panic. 3xx →
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

/// Build base64-encoded `TransactionEnvelope` invoking `function_name` on the
/// contract — `token_uri(token_id_u32)` (SEP-50 / OpenZeppelin convention),
/// zero-arg `token_uri()` (SEP-39 / ERC-721 collection-wide convention) or
/// zero-arg `name()` (SEP-50 collection name, task 0340) when `token_id_u32`
/// is `None`. Source account, fee, seq_num are dummy — simulate path ignores
/// them.
fn build_simulate_envelope(
    contract_id: &str,
    function_name: &str,
    token_id_u32: Option<u32>,
) -> Result<String, NftTokenUriError> {
    let contract = stellar_strkey::Contract::from_string(contract_id).map_err(|_| {
        NftTokenUriError::MalformedInput {
            field: "contract_id strkey",
            value: contract_id.to_owned(),
        }
    })?;
    let contract_address = ScAddress::Contract(ContractId(Hash(contract.0)));
    let function_name = ScSymbol(StringM::try_from(function_name.as_bytes().to_vec())?);
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
/// (in `wasm_programs.metadata` JSONB) to learn
/// `token_uri`'s arity ahead of time and call the right variant
/// directly. Saves one RPC round-trip per SEP-39 token (a SEP-39
/// collection with N tokens currently spends 2 × N RPC calls; with
/// spec dispatch it spends N). Prerequisites surfaced by 0197 Step 1:
///   1. `soroban_contracts.wasm_hash` is reliably populated for
///      non-SAC contracts — currently 99.9 % NULL (Step 1 Finding F9;
///      same root cause class as Bug #4 SAC-detection gap).
///   2. `wasm_programs.metadata` is populated with a real
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
    let per_token_envelope =
        build_simulate_envelope(contract_id, TOKEN_URI_FN, Some(token_id_u32))?;
    match simulate_transaction(client, rpc_url, &per_token_envelope).await {
        Ok(xdr) => Ok(xdr),
        Err(NftTokenUriError::SorobanRpc(msg)) if is_token_uri_arity_mismatch(&msg) => {
            debug!(
                contract_id = %contract_id,
                "token_uri(token_id) returned arity mismatch; retrying zero-arg token_uri() (SEP-39 contracts)"
            );
            let collection_envelope = build_simulate_envelope(contract_id, TOKEN_URI_FN, None)?;
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

/// Decode base64 ScVal XDR into a string (`function_name` only labels the
/// error). Only `ScVal::String` is accepted: `ScSymbol` is limited to 32 bytes
/// in XDR and cannot hold a realistic URI, and any other variant is a
/// producer-side contract bug. Shared by `token_uri()` and `name()` — both
/// return `String` in the OpenZeppelin / SEP conventions.
fn decode_string_result(xdr_b64: &str, function_name: &str) -> Result<String, NftTokenUriError> {
    let raw = BASE64
        .decode(xdr_b64)
        .map_err(|e| NftTokenUriError::MalformedRpcResponse(format!("xdr base64: {e}")))?;
    let bytes = match ScVal::from_xdr(&raw, Limits::none())? {
        ScVal::String(ScString(s)) => s.into_vec(),
        other => {
            return Err(NftTokenUriError::MalformedRpcResponse(format!(
                "{function_name} returned non-String ScVal: {other:?}"
            )));
        }
    };
    String::from_utf8(bytes).map_err(|e| {
        NftTokenUriError::MalformedRpcResponse(format!("{function_name} not UTF-8: {e}"))
    })
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
mod tests;
