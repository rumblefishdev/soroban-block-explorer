//! `NftTokenUriFetcher` — fail-soft LRU-cached Soroban RPC + HTTP client.
//!
//! Construct once at Lambda cold start (or api `AppState` build); reuse
//! across invocations. Cheap to clone — both inner client and cache are
//! `Arc`-backed.
//!
//! ### Defensive guards (mirrored from `sep1::client`)
//!
//! - **Redirect policy**: `Policy::limited(0)`. Any 3xx becomes a
//!   `reqwest::Error::TooManyRedirects` so a malicious contract cannot
//!   bypass [`validate_uri`] by bouncing us to a private-IP host.
//! - **`validate_uri`**: rejects URI schemes other than `https://` and
//!   `ipfs://`, rejects empty / IP-literal / RFC-1035-malformed
//!   hostnames before any network attempt. Applied to the URI returned
//!   by `token_uri()` *and* to the inner `image` field if the metadata
//!   JSON points to one.
//! - **`MAX_BODY_BYTES`** body cap (256 KB) — NFT metadata files are
//!   small (a few KB typically); cap leaves headroom for collections
//!   with large `attributes` arrays without inviting a memory bomb if
//!   the gateway streams an unrelated huge file. Applied via a streamed
//!   chunk loop (same shape as `sep1::client::capped_body`).
//!
//! ### STUB STATUS — Soroban RPC not yet wired
//!
//! The fetcher's surface is finalised so callers (worker module +
//! api detail handler) route through it today. `resolve()` currently
//! returns `Ok(None)` in every code path (warn-logged once per call)
//! because the workspace has no Soroban RPC / XDR-builder client yet.
//!
//! The shape that lands when the RPC piece arrives:
//!
//! 1. `simulate_token_uri(contract_id, token_id)` → URI string.
//! 2. `validate_uri(&uri)` (rejects unsafe schemes / IP literals /
//!    malformed hostnames before any fetch).
//! 3. `fetch_uri(uri)` → bytes + Content-Type, body capped at
//!    `MAX_BODY_BYTES`.
//! 4. Branch on Content-Type:
//!    - `application/json` → parse → return the full JSON blob.
//!    - `image/*` → return a synthesised `{ "image": "<url>" }` JSON
//!      so the api detail handler has a uniform shape.
//! 5. `ipfs://...` URLs are resolved through the configured gateway
//!    (default Cloudflare) before HTTP fetch; persisted `media_url`
//!    is always the resolved HTTPS form so the frontend can `<img src>`
//!    without a second resolve pass.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache as FutureCache;
use reqwest::redirect::Policy;
use serde_json::Value;
use tracing::instrument;

use super::errors::NftTokenUriError;

/// NFT metadata JSON cap. Typical files are <10 KB; 256 KB leaves
/// headroom for collections with very large `attributes` arrays
/// without exposing the worker to memory bombs from misconfigured
/// gateways. Streamed chunk-by-chunk (see [`capped_body`]) so we don't
/// buffer the body before checking the limit.
pub(super) const MAX_BODY_BYTES: usize = 256 * 1024;

/// Per-host TCP connect timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Total per-request budget. Looser than SEP-1 because IPFS gateways
/// have higher tail latency on cold CIDs.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Cache TTL. NFT metadata for static collections is immutable once
/// pinned (IPFS); 24h cap matches the SEP-1 fetcher convention and
/// gives mutable / dynamic NFT collections a fresh-fetch budget every
/// 24h. Warm cache survives only inside one Lambda container.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Max distinct (contract_id, token_id) tuples held warm per container.
/// 1024 ≈ same order as the SEP-1 fetcher; tune from prod metrics.
const CACHE_CAPACITY: u64 = 1024;

const USER_AGENT: &str = concat!("soroban-block-explorer/", env!("CARGO_PKG_VERSION"));

/// Fetcher for the per-NFT `token_uri()` JSON metadata pipeline
/// (Soroban RPC + HTTP / IPFS gateway).
///
/// `cache_key` is `"{contract_id}:{token_id}"`.
#[derive(Clone)]
pub struct NftTokenUriFetcher {
    #[allow(dead_code)]
    client: reqwest::Client,
    cache: FutureCache<String, Arc<Option<Value>>>,
}

impl NftTokenUriFetcher {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // No redirect-following: a 30x to a loopback / link-local /
            // RFC1918 host would bypass `validate_uri`. `limited(0)` makes
            // reqwest surface 3xx as `reqwest::Error::TooManyRedirects`.
            .redirect(Policy::limited(0))
            .user_agent(USER_AGENT)
            .build()?;
        let cache = FutureCache::builder()
            .time_to_live(CACHE_TTL)
            .max_capacity(CACHE_CAPACITY)
            .build();
        Ok(Self { client, cache })
    }

    /// Resolve `(contract_id, token_id)` → optional NFT metadata JSON.
    ///
    /// **STUB — hard-fail until Phase E.** Single chokepoint that
    /// panics: worker calls bubble up as Lambda failure → SQS retry →
    /// DLQ → DepthAlarm; api detail handler calls crash the request
    /// → 502 to the client. Both paths surface the missing fetcher
    /// loudly, no silent NULL fallback. Real impl returns `None` only
    /// for legitimate empty cases (e.g. `image/*` Content-Type) —
    /// caller continues to treat `None` as "no metadata".
    ///
    /// Replace the panic with the real RPC + HTTP pipeline; all call
    /// sites stay unchanged.
    #[instrument(skip(self), fields(contract_id = %contract_id, token_id = %token_id))]
    pub async fn resolve(&self, contract_id: &str, token_id: &str) -> Option<Value> {
        let _ = (contract_id, token_id, &self.cache, &self.client);
        unimplemented!("nft token_uri fetcher: Soroban RPC client lands in 0195 §2d Phase E");
    }
}

/// URI safety check before any network attempt.
///
/// Accepts `https://...` (post-IPFS-resolve) and `ipfs://...` (pre-resolve).
/// Rejects anything else: `http://` (no TLS), `file://`, `data:`,
/// `javascript:`, IP-literal hostnames, malformed hosts.
///
/// Applied to (a) the URI returned by `token_uri()` and (b) the
/// `image` field inside the JSON metadata when the worker writes it
/// to `media_url`. Worker also re-checks at write time via
/// `is_safe_media_url` — defence in depth in case the fetcher path
/// ever changes.
#[allow(dead_code)]
pub(super) fn validate_uri(uri: &str) -> Result<(), NftTokenUriError> {
    let uri = uri.trim();
    if uri.is_empty() {
        return Err(NftTokenUriError::MalformedUri {
            uri: uri.to_owned(),
        });
    }

    // Allowed schemes only.
    let host = if let Some(rest) = uri.strip_prefix("https://") {
        rest
    } else if let Some(rest) = uri.strip_prefix("ipfs://") {
        // For ipfs:// the path-after-scheme is the CID (+ optional path).
        // No host/port — return Ok early once we've confirmed it's
        // non-empty and printable ASCII (CIDs are base58 / base32).
        if rest.is_empty() {
            return Err(NftTokenUriError::MalformedUri {
                uri: uri.to_owned(),
            });
        }
        return Ok(());
    } else {
        return Err(NftTokenUriError::UnsafeScheme {
            uri: uri.to_owned(),
        });
    };

    // Strip path / query / fragment to leave bare authority.
    let authority = host.split(['/', '?', '#']).next().unwrap_or("");
    // Drop userinfo (we never expect any; reject if present so a
    // `https://user:pass@evil/...` form can't mask the host check).
    if authority.contains('@') {
        return Err(NftTokenUriError::MalformedUri {
            uri: uri.to_owned(),
        });
    }
    // Strip port.
    let host_only = authority.split(':').next().unwrap_or("");
    if host_only.is_empty() {
        return Err(NftTokenUriError::MalformedUri {
            uri: uri.to_owned(),
        });
    }
    if !host_only
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        return Err(NftTokenUriError::MalformedUri {
            uri: uri.to_owned(),
        });
    }
    if host_only.parse::<IpAddr>().is_ok() {
        return Err(NftTokenUriError::MalformedUri {
            uri: uri.to_owned(),
        });
    }
    Ok(())
}

/// Stream the body chunk-by-chunk; bail out if the running total crosses
/// `MAX_BODY_BYTES` before fully buffering. Mirrors `sep1::client::capped_body`.
#[allow(dead_code)]
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
}
