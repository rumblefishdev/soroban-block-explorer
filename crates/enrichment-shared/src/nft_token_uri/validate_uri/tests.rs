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
