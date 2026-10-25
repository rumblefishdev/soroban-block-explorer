//! Source verification results from a verifier aggregator (task 0639).
//!
//! A contract author submits source to a verifier, which rebuilds the Wasm
//! in a pinned container and compares its hash with the code on chain. The
//! aggregator's Query API (`GET {base}/wasms/{wasm_hash}`) collects the
//! results of several verifiers per Wasm hash. This module reads that answer
//! and reduces it to one [`VerificationStatus`]; [`archive`] downloads and
//! unpacks the published source.
//!
//! Configuration, not code: the client exists only when
//! `SOURCE_VERIFICATION_API_URL` is set (testnet today — the service runs on
//! a staging host). Nothing is stored; responses are cacheable by header.
//! Every failure is an error the handler reports; an unknown per-verifier
//! `status` is logged and counts as neither verified nor mismatched.

pub mod archive;

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The Query API answered 511 of 511 calls within 2.9 s, median 1.2 s
/// (measured from Europe, 2026-10-14); 5 s leaves room for a slow day.
/// Archives come from a CDN (largest 376 KB in under 0.2 s); 10 s covers the
/// 10 MB cap.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const AGGREGATE_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// One aggregate is ~3 KB (measured); 1 MB means a broken or hostile answer.
const MAX_AGGREGATE_BYTES: usize = 1024 * 1024;

const USER_AGENT: &str = concat!("soroban-block-explorer/", env!("CARGO_PKG_VERSION"));

/// Env var naming the Query API base, e.g.
/// `https://staging.sv.blocksmith.co/api/query/v1`.
pub const API_URL_ENV: &str = "SOURCE_VERIFICATION_API_URL";

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("source verification API unreachable: {0}")]
    Http(#[from] reqwest::Error),
    #[error("source verification API answered HTTP {0}")]
    Status(u16),
    #[error("source verification API answered an unexpected document: {0}")]
    Shape(String),
    #[error("source verification API answer is larger than {0} bytes")]
    TooLarge(usize),
    /// A verifier could not be read, and the remaining answers decide
    /// nothing: the status is unknown, not `none` or `mismatched`.
    #[error("verifiers unavailable, status unknown: {0}")]
    Incomplete(String),
}

/// Read a response body, refusing it once it passes `max` bytes — before
/// buffering the rest.
pub(crate) async fn read_capped(
    mut resp: reqwest::Response,
    max: usize,
) -> Result<Vec<u8>, VerificationError> {
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if body.len().saturating_add(chunk.len()) > max {
            return Err(VerificationError::TooLarge(max));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

// ---------------------------------------------------------------------------
// Wire shapes — plain data, only the fields we read. Field names are the
// Query API's (OpenAPI `AggregatedWasmStatus`, `AggregatedRegistryVerification`).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Aggregate {
    pub wasm_hash: String,
    pub updated_at: String,
    // `agreement` is not read: their SEP calls it descriptive, never a
    // verdict, so the page's status comes from the verifiers' own answers.
    pub source_code_verifications: Vec<VerifierResult>,
    /// Verifiers the aggregator could not read for this answer.
    #[serde(default)]
    pub unavailable_verifiers: Vec<UnavailableVerifier>,
}

#[derive(Debug, Deserialize)]
pub struct UnavailableVerifier {
    pub url: String,
    /// `fetch_failed` | `invalid_response` | `timeout` | `refused`.
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifierResult {
    pub verifier: Verifier,
    pub status: String,
    pub bldimg: Option<String>,
    #[serde(default)]
    pub bldopt: Vec<String>,
    /// SEP-58 build arguments, in order (default `contract build`).
    #[serde(default)]
    pub bldarg: Vec<String>,
    pub source_sha256: Option<String>,
    pub processed_at: Option<String>,
    #[serde(default)]
    pub results_urls: Vec<Link>,
    #[serde(default)]
    pub verified_source_code_uris: Vec<Link>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verifier {
    pub url: String,
    pub name: Option<String>,
}

/// One published link: a report (`type: "report"`) or a source archive.
#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    pub url: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// Value of a `--key=value` build option (SEP-58 `bldopt`), e.g. the
/// `--package` the verifier built — one archive can hold several.
pub fn build_option(r: &VerifierResult, key: &str) -> Option<String> {
    let prefix = format!("--{key}=");
    for option in &r.bldopt {
        if let Some(value) = option.strip_prefix(&prefix) {
            return Some(value.to_owned());
        }
    }
    None
}

/// The verification report, when the verifier typed a link as `report`.
pub fn report_url(r: &VerifierResult) -> Option<String> {
    for link in &r.results_urls {
        if link.kind.as_deref() == Some("report") {
            return Some(link.url.clone());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Reduction to one status
// ---------------------------------------------------------------------------

/// What the contract page shows; on the wire as its snake_case name, listed
/// in the OpenAPI spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// A verifier rebuilt the on-chain Wasm and the source is published.
    Verified,
    /// A verifier rebuilt the on-chain Wasm; the source is not published.
    VerifiedPrivate,
    /// Source was submitted but rebuilt to a different Wasm.
    Mismatched,
    /// Verifiers disagree: at least one verified, at least one did not.
    Disputed,
    /// Results exist, none of them verified or mismatched.
    Unverified,
    /// No verifier holds a result for this Wasm.
    None,
    /// This network has no verifier service configured; nothing was asked.
    Unavailable,
}

/// The page's status, read from the verifiers' own answers, plus the
/// verified record whose recipe the page shows — only for a verified
/// status: source that did not reproduce the on-chain code is never shown.
pub fn reduce(
    agg: &Aggregate,
) -> Result<(VerificationStatus, Option<VerifierResult>), VerificationError> {
    let mut with_source = None;
    let mut any_verified = None;
    let mut any_mismatched = false;
    for r in &agg.source_code_verifications {
        match r.status.as_str() {
            "verified" => {
                if with_source.is_none() && !r.verified_source_code_uris.is_empty() {
                    with_source = Some(r.clone());
                }
                if any_verified.is_none() {
                    any_verified = Some(r.clone());
                }
            }
            "mismatched" => any_mismatched = true,
            "unverified" => {}
            other => tracing::warn!(
                wasm_hash = %agg.wasm_hash,
                verifier = %r.verifier.url,
                status = %other,
                "source verification: verifier status outside verified/mismatched/unverified"
            ),
        }
    }

    let (status, shown) = if any_verified.is_some() && any_mismatched {
        (VerificationStatus::Disputed, None)
    } else if with_source.is_some() {
        (VerificationStatus::Verified, with_source)
    } else if any_verified.is_some() {
        (VerificationStatus::VerifiedPrivate, any_verified)
    } else if any_mismatched {
        (VerificationStatus::Mismatched, None)
    } else if agg.source_code_verifications.is_empty() {
        (VerificationStatus::None, None)
    } else {
        (VerificationStatus::Unverified, None)
    };

    // A verifier that could not be read may hold the answer the others lack.
    // A verification found elsewhere stands, and so does a disagreement no
    // further answer can settle; anything else is unknown.
    if !agg.unavailable_verifiers.is_empty() {
        let mut down = Vec::new();
        for u in &agg.unavailable_verifiers {
            down.push(format!("{} ({})", u.url, u.reason));
        }
        let down = down.join(", ");
        let settled = matches!(
            status,
            VerificationStatus::Verified
                | VerificationStatus::VerifiedPrivate
                | VerificationStatus::Disputed
        );
        if !settled {
            return Err(VerificationError::Incomplete(down));
        }
        tracing::warn!(
            wasm_hash = %agg.wasm_hash,
            unavailable = %down,
            "source verification: settled, but some verifiers were unavailable"
        );
    }
    Ok((status, shown))
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// The HTTP client and the Query API base. Cheap to clone (`reqwest::Client`
/// is `Arc`-backed).
#[derive(Clone)]
pub struct SourceVerificationFetcher {
    pub client: reqwest::Client,
    pub base_url: String,
}

/// `None` when `SOURCE_VERIFICATION_API_URL` is unset or blank — the network
/// has no verifier service: `/verification` answers `unavailable`.
pub fn fetcher_from_env() -> Result<Option<SourceVerificationFetcher>, reqwest::Error> {
    fetcher_from_setting(&std::env::var(API_URL_ENV).unwrap_or_default())
}

/// The env value as read: empty or blank means no verifier service.
fn fetcher_from_setting(value: &str) -> Result<Option<SourceVerificationFetcher>, reqwest::Error> {
    let url = value.trim();
    if url.is_empty() {
        return Ok(None);
    }
    Ok(Some(new_fetcher(url)?))
}

pub fn new_fetcher(base_url: &str) -> Result<SourceVerificationFetcher, reqwest::Error> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        // The archive host is checked before each download; a redirect
        // would move the request past that check.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(USER_AGENT)
        .build()?;
    Ok(SourceVerificationFetcher {
        client,
        base_url: base_url.trim_end_matches('/').to_owned(),
    })
}

/// The aggregate for one Wasm hash (lowercase hex, 64 chars — the caller
/// passes the hash our own database holds).
pub async fn fetch_aggregate(
    f: &SourceVerificationFetcher,
    wasm_hash_hex: &str,
) -> Result<Aggregate, VerificationError> {
    let url = format!("{}/wasms/{wasm_hash_hex}", f.base_url);
    let resp = f.client.get(&url).timeout(AGGREGATE_TIMEOUT).send().await?;
    if !resp.status().is_success() {
        return Err(VerificationError::Status(resp.status().as_u16()));
    }
    let body = read_capped(resp, MAX_AGGREGATE_BYTES).await?;
    let agg: Aggregate =
        serde_json::from_slice(&body).map_err(|e| VerificationError::Shape(e.to_string()))?;
    // The aggregator keys by hash; an answer for another hash is a bug on
    // one side and must not be shown as this contract's result.
    if !agg.wasm_hash.eq_ignore_ascii_case(wasm_hash_hex) {
        return Err(VerificationError::Shape(format!(
            "asked for {wasm_hash_hex}, answered for {}",
            agg.wasm_hash
        )));
    }
    Ok(agg)
}

#[cfg(test)]
mod tests;
