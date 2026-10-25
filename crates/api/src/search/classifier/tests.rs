use super::*;

const FULL_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT";
const FULL_C: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";

#[test]
fn classifies_64_hex_as_hash_bytes() {
    let q = "a".repeat(64);
    let out = classify(&q);
    assert_eq!(out.hash_bytes.as_ref().map(Vec::len), Some(32));
    assert!(out.strkey_prefix.is_none());
}

#[test]
fn classifies_a_w_strkey_as_its_contract() {
    // `CDL74…IGWA` with sub-account id 42 (CAP-0084), typed in lower case.
    const REAL_C: &str = "CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA";
    const REAL_W: &str = "WDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIAAAAAAAAAAAFLR56";
    let out = classify(&REAL_W.to_ascii_lowercase());
    assert_eq!(out.strkey_prefix.as_deref(), Some(REAL_C));
    assert!(out.hash_bytes.is_none());
}

#[test]
fn classifies_full_g_strkey() {
    let out = classify(FULL_G);
    assert_eq!(out.strkey_prefix.as_deref(), Some(FULL_G));
    assert!(out.hash_bytes.is_none());
}

#[test]
fn classifies_full_c_strkey() {
    let out = classify(FULL_C);
    assert_eq!(out.strkey_prefix.as_deref(), Some(FULL_C));
}

#[test]
fn classifies_strkey_prefix_lowercase_input() {
    // Lowercase G prefix should normalise to upper-case; alphabet
    // check happens after normalisation.
    let q = "gaaa";
    let out = classify(q);
    assert_eq!(out.strkey_prefix.as_deref(), Some("GAAA"));
}

#[test]
fn rejects_garbage_text() {
    let out = classify("hello world");
    assert!(out.hash_bytes.is_none());
    assert!(out.strkey_prefix.is_none());
}

#[test]
fn classifies_full_l_strkey_as_pool_hash_bytes() {
    // Full L-strkey decodes to the same 32-byte hash the pool CTE
    // matches on, so it must populate `hash_bytes` (not
    // `strkey_prefix`) so `pool_hits` exact-match fires.
    let strkey = stellar_strkey::LiquidityPool([0u8; 32]).to_string();
    let out = classify(&strkey);
    assert_eq!(out.hash_bytes.as_deref(), Some([0u8; 32].as_slice()));
    assert!(out.strkey_prefix.is_none());
}

#[test]
fn rejects_partial_l_strkey() {
    // Partial L-prefix is not a valid SEP-23 strkey (no CRC payload).
    // Falls through to no classification — broad search has no pool
    // text column to scan against (CH-era follow-up).
    let out = classify("LAB");
    assert!(out.hash_bytes.is_none());
    assert!(out.strkey_prefix.is_none());
}

/// A CRC-valid account StrKey. `FULL_G` above is shape-only — the prefix arm
/// never checks the checksum — but the `CODE:ISSUER` arm does, so it needs a
/// key the decoder actually accepts.
fn valid_g() -> String {
    // `format!` (Display), not the inherent `to_string()` — that one returns
    // a `heapless::String`, the same trap `common::strkey` documents.
    format!("{}", stellar_strkey::ed25519::PublicKey([0u8; 32]))
}

#[test]
fn classifies_code_issuer_on_both_separators() {
    // `:` is the canonical SEP / SDK form; `-` is what our own
    // `/assets/:id` routes emit, so users paste it straight back.
    let g = valid_g();
    for q in [format!("USDC:{g}"), format!("USDC-{g}")] {
        let out = classify(&q);
        assert_eq!(
            out.code_issuer,
            Some(("USDC".to_string(), g.clone())),
            "failed for {q:?}"
        );
        // Must not also fire the substring / prefix arms.
        assert!(out.strkey_prefix.is_none());
        assert!(out.hash_bytes.is_none());
    }
}

#[test]
fn code_issuer_keeps_code_case_and_uppercases_the_strkey() {
    let g = valid_g();
    let out = classify(&format!("uSdC:{}", g.to_lowercase()));
    assert_eq!(out.code_issuer, Some(("uSdC".to_string(), g)));
}

#[test]
fn code_issuer_rejects_a_bad_issuer_checksum() {
    // Last character flipped — valid base32, wrong CRC. Falls through to the
    // ordinary search rather than answering with a confident empty page.
    let g = valid_g();
    let flipped = if g.ends_with('A') { 'B' } else { 'A' };
    let bad = format!("{}{flipped}", &g[..g.len() - 1]);
    let out = classify(&format!("USDC:{bad}"));
    assert!(out.code_issuer.is_none());
    assert!(out.strkey_prefix.is_none());
}

#[test]
fn code_issuer_rejects_a_code_longer_than_alphanum12() {
    let out = classify(&format!("THIRTEENCHARS:{}", valid_g()));
    assert!(out.code_issuer.is_none());
}

#[test]
fn code_issuer_rejects_a_non_alphanumeric_code() {
    // A hyphenated word ahead of the key is not an asset code; splitting on
    // the LAST separator keeps this from being read as one.
    let out = classify(&format!("not_a_code:{}", valid_g()));
    assert!(out.code_issuer.is_none());
}

#[test]
fn a_bare_strkey_is_still_a_prefix_not_a_code_issuer() {
    // Regression guard: no separator, so the new arm must not shadow the
    // account/contract prefix classification.
    let g = valid_g();
    let out = classify(&g);
    assert!(out.code_issuer.is_none());
    assert_eq!(out.strkey_prefix.as_deref(), Some(g.as_str()));
}

#[test]
fn short_strkey_prefix_under_two_chars_rejected() {
    // Single-char "G" is too narrow — would force a full-table
    // index range scan. Reject as garbage.
    let out = classify("G");
    assert!(out.strkey_prefix.is_none());
}

#[test]
fn classifies_an_m_strkey_as_its_account() {
    const REAL_G: &str = "GD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2FTL";
    const REAL_M: &str = "MD5SL5RIC5STHGDDOJGSIIZHZZQPA4HIYFEPIQ3FF7M3H2F5VQ7K2AAAAAANF5TODJEYA";
    let out = classify(REAL_M);
    assert_eq!(out.strkey_prefix.as_deref(), Some(REAL_G));
}
