//! Unit tests for `validate_host`. The full HTTP path (`fetch_uncached`,
//! `capped_body`, error mapping, cache wrap) is intentionally not covered
//! by automated tests in-tree — see task 0188 §"Out of Scope" for the
//! rationale. A real-issuer smoke test against e.g. `ultrastellar.com`
//! is deferred to a follow-up.

use super::*;

#[test]
fn validate_host_accepts_normal_dns_names() {
    assert!(validate_host("ultrastellar.com").is_ok());
    assert!(validate_host("api.example.co.uk").is_ok());
    assert!(validate_host("issuer-2.example.com").is_ok());
}

#[test]
fn validate_host_rejects_empty() {
    assert!(matches!(
        validate_host(""),
        Err(Sep1Error::MalformedHomeDomain { .. })
    ));
}

#[test]
fn validate_host_rejects_ipv4_literal() {
    for ip in ["192.168.1.1", "127.0.0.1", "169.254.169.254"] {
        assert!(
            matches!(
                validate_host(ip),
                Err(Sep1Error::MalformedHomeDomain { .. })
            ),
            "expected rejection for {ip}",
        );
    }
}

#[test]
fn validate_host_rejects_ipv6_literal() {
    // The `:` makes these fail the byte-set check before IP parsing
    // even kicks in, but both gates should reject them.
    for ip in ["::1", "fe80::1"] {
        assert!(
            matches!(
                validate_host(ip),
                Err(Sep1Error::MalformedHomeDomain { .. })
            ),
            "expected rejection for {ip}",
        );
    }
}

#[test]
fn validate_host_rejects_url_smuggling() {
    // Anything containing `/`, `:`, `@`, `?`, `#`, space, or upper
    // bytes >127 fails the alphanumeric+.+- check.
    for bad in [
        "evil.com/path",
        "evil.com:8080",
        "user@evil.com",
        "evil.com?x=y",
        "evil.com#frag",
        "evil .com",
    ] {
        assert!(
            matches!(
                validate_host(bad),
                Err(Sep1Error::MalformedHomeDomain { .. })
            ),
            "expected rejection for {bad}",
        );
    }
}

// --- redirect policy (task 0200): `redirect_allowed` = same eTLD+1 + safe host ---

#[test]
fn redirect_allowed_follows_apex_and_www() {
    assert!(redirect_allowed("circle.com", "www.circle.com"));
    assert!(redirect_allowed("www.circle.com", "circle.com"));
    assert!(redirect_allowed("circle.com", "toml.circle.com"));
}

#[test]
fn redirect_allowed_uses_public_suffix_not_naive_strip() {
    // `co.uk` is a public suffix, so apex↔www within it is same-domain,
    // but two different second-level labels are DIFFERENT registrable domains.
    assert!(redirect_allowed("example.co.uk", "www.example.co.uk"));
    assert!(!redirect_allowed("a.co.uk", "b.co.uk"));
}

#[test]
fn redirect_allowed_blocks_cross_domain() {
    assert!(!redirect_allowed("circle.com", "evil.com"));
    // suffix-smuggling: the eTLD+1 of `circle.com.evil.com` is `evil.com`
    assert!(!redirect_allowed("circle.com", "circle.com.evil.com"));
}

#[test]
fn redirect_allowed_blocks_unsafe_targets() {
    for bad in [
        "127.0.0.1",
        "169.254.169.254",
        "192.168.1.1",
        "evil.com:8080",
    ] {
        assert!(
            !redirect_allowed("circle.com", bad),
            "expected redirect to {bad} to be blocked",
        );
    }
}
