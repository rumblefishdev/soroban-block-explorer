use super::*;

#[test]
fn validate_uri_accepts_https() {
    assert!(validate_uri("https://example.com/123.json").is_ok());
    assert!(validate_uri("https://gateway.pinata.cloud/ipfs/Qm.../1.json").is_ok());
}

#[test]
fn validate_uri_accepts_ipfs() {
    assert!(validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG").is_ok());
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
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/../etc/passwd"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/../../1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/%2e%2e/1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/./1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    // Mixed-encoding traversals — fully-encoded `%2e%2e`, partially-
    // encoded `.%2e` / `%2e.`, single-encoded `%2e` (literal dot).
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/.%2e/1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/%2e./1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/%2E%2E/1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    assert!(matches!(
        validate_uri("ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/%2e/1.json"),
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
fn validate_uri_rejects_a_cid_that_cannot_decode() {
    // Production, token 15 of CAMOZBTH…N67X: the contract glued the token id
    // onto the CID, so no gateway can ever serve it.
    assert!(matches!(
        validate_uri("ipfs://bafkreib4534l4wdqxysgj5rrqtqtzwawkqfhfmoopwmvdvmkepnh5lnffi15"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    // CIDv0 is always `Qm` + 44 base58 characters.
    assert!(matches!(
        validate_uri("ipfs://QmShort/1.json"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
    // The gateway path form carries the CID in its second segment.
    assert!(matches!(
        validate_uri("ipfs://ipfs/bafkreib4534l4wdqxysgj5rrqtqtzwawkqfhfmoopwmvdvmkepnh5lnffi15"),
        Err(NftTokenUriError::MalformedUri { .. })
    ));
}

#[test]
fn validate_uri_accepts_real_cids_with_and_without_a_path() {
    for uri in [
        "ipfs://bafkreib4534l4wdqxysgj5rrqtqtzwawkqfhfmoopwmvdvmkepnh5lnffi",
        "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi/15.json",
        "ipfs://QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG/readme",
        "ipfs://ipfs/QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG",
        // Upper-case base32, an identity-hash CID, a percent-encoded path.
        "ipfs://BAFKREIB4534L4WDQXYSGJ5RRQTQTZWAWKQFHFMOOPWMVDVMKEPNH5LNFFI",
        "ipfs://bafkqaaa",
        "ipfs://bafkreib4534l4wdqxysgj5rrqtqtzwawkqfhfmoopwmvdvmkepnh5lnffi%2F1.json",
        // Encodings this check does not decode are left to the gateway.
        "ipfs://zdj7WWeQ43G6JJvLWQWZpyHuAMq6uYWRjkBXFad11vE2LHhQ7",
    ] {
        assert!(validate_uri(uri).is_ok(), "{uri}");
    }
}
