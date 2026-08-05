//! Transport-level retry classification shared by every HTTP fetcher in
//! this crate.
//!
//! One rule in one place: the SEP-1 and NFT-metadata paths previously each
//! carried a hand-mirrored copy, and the copies disagreed on HTTP 429 —
//! transient on one path, permanent on the other — so a one-off issuer
//! throttle wrote a permanent sentinel on the SEP-1 side (task 0455 / I7).
//! Both `is_transient` classifiers now delegate their `reqwest::Error`
//! judgement here; per-path variants that never touch the network
//! (malformed input, blocked redirects, oversized bodies) stay classified
//! in their own modules.

use std::error::Error as StdError;

/// Transient = a retry may succeed. Permanent = deterministic for this URL.
///
/// - **HTTP status present**: 5xx and 429 are transient (server-side /
///   load-shedding); every other status is permanent — the origin answered
///   and will answer the same way again.
/// - **No HTTP status**: transport-layer failure (connect, TLS, reset,
///   timeout, truncated body) — transient, EXCEPT a DNS-resolution failure
///   (NXDOMAIN / dead domain), which no same-host retry resolves
///   (task 0335).
///
/// The SQS redrive policy bounds retries, so erring transient on an
/// ambiguous transport fault costs a few redeliveries; erring permanent
/// writes a sentinel that only an operator `--retry-sentinels` run repairs.
pub fn is_transient_reqwest(err: &reqwest::Error) -> bool {
    if is_dns_failure(err) {
        return false;
    }
    match err.status() {
        Some(s) => s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS,
        None => true,
    }
}

/// True when a `reqwest` error's source chain indicates DNS resolution failed
/// (NXDOMAIN / host does not resolve). Such a failure is **permanent** — the
/// domain is gone, no retry resolves it — so [`is_transient_reqwest`]
/// classifies it permanent and the enrich fns write the `''` sentinel instead
/// of 3×-retrying to the DLQ (task 0335).
///
/// Deliberately NOT consulted by `nft_token_uri::errors::is_endpoint_fault`:
/// a dead host in the RPC/IPFS pool should still fail over to a different
/// provider (a different host may resolve).
pub(crate) fn is_dns_failure(err: &reqwest::Error) -> bool {
    let mut src: Option<&(dyn StdError + 'static)> = Some(err);
    while let Some(e) = src {
        if is_dns_marker(&e.to_string()) {
            return true;
        }
        src = e.source();
    }
    false
}

/// Resolver-error text markers for a DNS NXDOMAIN / no-such-host failure.
/// Split from [`is_dns_failure`] so it is unit-testable without constructing a
/// `reqwest::Error` (which has no public constructor). Matching is
/// case-insensitive.
///
/// ponytail: string-match on resolver text — Linux `getaddrinfo` wording
/// covers prod (Lambda AL2 + Hetzner box); upgrade to an explicit
/// `tokio::net::lookup_host` pre-check if a resolver/platform changes phrasing.
fn is_dns_marker(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("failed to lookup address")        // glibc getaddrinfo (Linux)
        || m.contains("name or service not known") // EAI_NONAME (Linux)
        || m.contains("no such host")               // common cross-platform
        || m.contains("nodename nor servname")      // macOS EAI_NONAME (dev)
        || m.contains("dns error") // hickory/trust-dns wrapper
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a genuine `reqwest::Error` carrying the given status via
    /// `error_for_status` against a local mock — `reqwest::Error` has no
    /// public constructor.
    async fn status_error(status: u16) -> reqwest::Error {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(status))
            .mount(&server)
            .await;
        reqwest::get(server.uri())
            .await
            .expect("mock must answer")
            .error_for_status()
            .expect_err("status is an error status")
    }

    #[tokio::test]
    async fn http_429_is_transient() {
        // The drift this module exists to end: 429 was permanent on the
        // SEP-1 path, so a one-off throttle wrote a permanent sentinel.
        assert!(is_transient_reqwest(&status_error(429).await));
    }

    #[tokio::test]
    async fn http_5xx_is_transient() {
        for code in [500, 502, 503, 504] {
            assert!(
                is_transient_reqwest(&status_error(code).await),
                "{code} should be transient"
            );
        }
    }

    #[tokio::test]
    async fn http_4xx_other_than_429_is_permanent() {
        for code in [400, 403, 404, 410] {
            assert!(
                !is_transient_reqwest(&status_error(code).await),
                "{code} should be permanent"
            );
        }
    }

    #[test]
    fn dns_marker_flags_nxdomain_phrasings() {
        for s in [
            "error sending request for url (https://dead.example/): error trying to connect: \
             dns error: failed to lookup address information: Name or service not known",
            "failed to lookup address information",
            "No such host is known. (os error 11001)",
            "nodename nor servname provided, or not known",
        ] {
            assert!(is_dns_marker(s), "should flag DNS failure: {s}");
        }
    }

    #[test]
    fn dns_marker_ignores_transient_phrasings() {
        for s in [
            "connection refused (os error 111)",
            "operation timed out",
            "error trying to connect: tls handshake eof",
            "503 Service Unavailable",
        ] {
            assert!(!is_dns_marker(s), "should NOT flag (transient): {s}");
        }
    }

    /// Empirical guard: the DNS carve-out hinges on `is_dns_marker` matching
    /// the text reqwest actually emits for an unresolvable host AND on that
    /// text being reachable via the error's `source()` chain. `.invalid`
    /// (RFC 6761) never resolves → guaranteed NXDOMAIN, no real network
    /// egress. `#[ignore]` (needs a working resolver). Run:
    /// `cargo test -p enrichment-shared is_dns_failure_matches_real -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs a resolver; verifies is_dns_failure vs reqwest's real NXDOMAIN error"]
    async fn is_dns_failure_matches_real_reqwest_nxdomain() {
        let err = reqwest::Client::new()
            .get("https://nonexistent-host-0335.invalid/.well-known/stellar.toml")
            .send()
            .await
            .expect_err("an unresolvable host must error");
        eprintln!("top-level: {err}");
        let mut s: Option<&(dyn StdError + 'static)> = Some(&err);
        while let Some(e) = s {
            eprintln!("  source: {e}");
            s = e.source();
        }
        assert!(
            is_dns_failure(&err),
            "is_dns_failure must fire for a real NXDOMAIN reqwest error"
        );
    }
}
