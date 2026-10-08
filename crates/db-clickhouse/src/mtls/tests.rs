use super::*;

/// Three concatenated PEM-encoded certs from the same on-disk
/// well-formed test bundle would be ideal here; we only verify the
/// parser shape (zero / non-zero result) since the actual TLS path
/// is exercised end-to-end by the cutover smoke test, not in unit
/// tests.
#[test]
fn parse_certs_rejects_empty_pem() {
    assert!(parse_certs("").map(|v| v.is_empty()).unwrap_or(true));
}

#[test]
fn require_env_errors_when_unset() {
    let r = require_env("DEFINITELY_NOT_SET_VAR_FOR_TEST_0241");
    assert!(matches!(r, Err(MtlsError::MissingEnv(_))));
}

#[test]
fn bundle_debug_redacts_pem_material() {
    let b = MtlsBundle {
        cert_pem: "SECRET-CERT".into(),
        key_pem: "SECRET-KEY".into(),
        ca_pem: "SECRET-CA".into(),
    };
    let rendered = format!("{b:?}");
    assert!(!rendered.contains("SECRET-CERT"));
    assert!(!rendered.contains("SECRET-KEY"));
    assert!(!rendered.contains("SECRET-CA"));
}
