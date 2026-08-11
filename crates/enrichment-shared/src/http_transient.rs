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

/// Transient = a retry may succeed. Permanent = deterministic for this URL.
///
/// - **HTTP status present**: 5xx and 429 are transient (server-side /
///   load-shedding); every other status is permanent — the origin answered
///   and will answer the same way again.
/// - **No HTTP status, connect-level** (DNS failure, connection
///   refused/unreachable, TLS handshake): **permanent** — the signature of a
///   dead issuer domain. Measured before this rule (2026-08-11, lore-0455):
///   30 days of worker "transient" retries were 100% this class across 6
///   keys and 0% genuine blips (zero 429/5xx/timeouts), including one dead
///   domain retried 668 times in 83 minutes. A retry against a dead host
///   buys nothing; the `''` sentinel closes the case and an operator
///   `--retry-sentinels` run repairs the rare host that comes back.
///   (Supersedes the narrower DNS-only carve-out from task 0335.)
/// - **No HTTP status, past connect** (timeout mid-request, reset, truncated
///   body): transient — the host exists and was answering; measured zero
///   occurrences, kept retryable because these ARE plausible one-off blips.
///   If timeouts ever show up as a dead-host signature, move them across.
pub fn is_transient_reqwest(err: &reqwest::Error) -> bool {
    match err.status() {
        Some(s) => s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS,
        None => !err.is_connect(),
    }
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

    /// The 2026-08-11 rule: a real connection-refused error (no HTTP
    /// response) must classify PERMANENT — this is the dead-issuer-domain
    /// signature that used to cycle 3× into the DLQ. Port 9 (discard) on
    /// loopback refuses instantly; no network egress.
    #[tokio::test]
    async fn connect_failure_is_permanent() {
        let err = reqwest::Client::new()
            .get("http://127.0.0.1:9/.well-known/stellar.toml")
            .send()
            .await
            .expect_err("nothing listens on the discard port");
        assert!(err.is_connect(), "precondition: a connect-level error");
        assert!(
            !is_transient_reqwest(&err),
            "connect failure must be permanent (dead-domain signature)"
        );
    }

    /// A timeout PAST connect stays transient: the host exists and was
    /// answering (wiremock accepted the connection); only the response was
    /// slow. Distinguishes the kept-retryable class from the connect rule
    /// above.
    #[tokio::test]
    async fn slow_response_timeout_is_transient() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_millis(200)),
            )
            .mount(&server)
            .await;
        let err = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(20))
            .build()
            .expect("client")
            .get(server.uri())
            .send()
            .await
            .expect_err("20 ms budget vs 200 ms delay must time out");
        assert!(err.is_timeout(), "precondition: a timeout error");
        assert!(!err.is_connect(), "precondition: connect succeeded");
        assert!(
            is_transient_reqwest(&err),
            "post-connect timeout must stay transient"
        );
    }

    /// A dead-but-DNS-resolving domain and an NXDOMAIN both surface as
    /// connect-level errors in reqwest, so the single `is_connect` rule
    /// subsumes the old DNS-marker string-matching (task 0335) — this test
    /// pins the NXDOMAIN half. `.invalid` (RFC 6761) never resolves; no real
    /// egress. `#[ignore]` (needs a working resolver). Run:
    /// `cargo test -p enrichment-shared nxdomain -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs a resolver; verifies NXDOMAIN classifies permanent via is_connect"]
    async fn nxdomain_is_permanent_via_connect_rule() {
        let err = reqwest::Client::new()
            .get("https://nonexistent-host-0335.invalid/.well-known/stellar.toml")
            .send()
            .await
            .expect_err("an unresolvable host must error");
        eprintln!("top-level: {err}");
        assert!(
            !is_transient_reqwest(&err),
            "NXDOMAIN must classify permanent"
        );
    }
}
