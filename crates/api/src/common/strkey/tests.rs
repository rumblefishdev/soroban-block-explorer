use super::*;

// Synthetic shape-valid placeholders (no CRC), 56 chars each.
const VALID_C: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";
const VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT";

#[test]
fn accepts_correct_prefix() {
    assert!(is_strkey_shape(VALID_C, 'C'));
    assert!(is_strkey_shape(VALID_G, 'G'));
}

#[test]
fn rejects_wrong_prefix() {
    // Value is well-formed StrKey but for the OTHER prefix — must reject.
    // This is the security-critical branch: without it, an account
    // address would slip through a contract-id check.
    assert!(!is_strkey_shape(VALID_G, 'C'));
    assert!(!is_strkey_shape(VALID_C, 'G'));
}

#[test]
fn rejects_wrong_length() {
    assert!(!is_strkey_shape("CAAA", 'C'));
    let too_long = format!("C{}", "A".repeat(60)); // 61 chars
    assert!(!is_strkey_shape(&too_long, 'C'));
    assert!(!is_strkey_shape("", 'C'));
}

#[test]
fn rejects_invalid_alphabet() {
    // Contains `0` (not in base32). Length 56, prefix C — only alphabet fails.
    let bad = "C00000000000000000000000000000000000000000000000000000A";
    assert!(!is_strkey_shape(bad, 'C'));
}

#[test]
fn rejects_lowercase() {
    // Lowercase 'a' is outside the uppercase-only base32 subset Stellar uses.
    let bad = "Caaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert!(!is_strkey_shape(bad, 'C'));
}

// -----------------------------------------------------------------------
// pool_id_hex_to_strkey — round-trip with stellar_strkey::LiquidityPool
// -----------------------------------------------------------------------

#[test]
fn pool_id_hex_to_strkey_round_trip_zero() {
    let hex = "0".repeat(64);
    let strkey = pool_id_hex_to_strkey(&hex, domain::PoolKind::Classic);
    assert!(strkey.starts_with('L'));
    assert_eq!(strkey.len(), 56);
    let decoded = stellar_strkey::LiquidityPool::from_string(&strkey).unwrap();
    let mut round = String::with_capacity(64);
    for b in &decoded.0 {
        use core::fmt::Write;
        let _ = write!(&mut round, "{b:02x}");
    }
    assert_eq!(round, hex);
}

#[test]
fn pool_id_hex_to_strkey_round_trip_mixed_bytes() {
    // Pattern exercises both nibbles of each byte and the full hex alphabet.
    let hex = "0123456789abcdef".repeat(4);
    assert_eq!(hex.len(), 64);
    let strkey = pool_id_hex_to_strkey(&hex, domain::PoolKind::Classic);
    let decoded = stellar_strkey::LiquidityPool::from_string(&strkey).unwrap();
    let mut round = String::with_capacity(64);
    for b in &decoded.0 {
        use core::fmt::Write;
        let _ = write!(&mut round, "{b:02x}");
    }
    assert_eq!(round, hex);
}

#[test]
#[should_panic(expected = "pool_id hex must be exactly 64 chars")]
fn pool_id_hex_to_strkey_panics_on_short_input() {
    let _ = pool_id_hex_to_strkey("abc", domain::PoolKind::Classic);
}

#[test]
fn an_asset_code_is_not_mistaken_for_an_identifier() {
    // The whole point of the split: these must fall through to the code
    // matcher, not become a point seek that finds nothing.
    assert!(pool_id_from_text("XLM").is_none());
    assert!(pool_id_from_text("xlm/kale").is_none());
    assert!(pool_id_from_text("").is_none());
    // 64 chars but not a StrKey.
    assert!(pool_id_from_text(&"z".repeat(64)).is_none());
    // Hex-looking asset codes stay asset codes. With a hex branch these
    // parsed as identifiers and searched for a pool that does not exist.
    for code in ["FACE", "BEEF", "CAFE", "DEAD"] {
        assert!(
            pool_id_from_text(code).is_none(),
            "{code} must stay an asset code"
        );
    }
    // A strkey of the wrong type — accounts are not pools.
    assert!(
        pool_id_from_text("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN").is_none()
    );
}

#[test]
fn pool_identifier_is_recognised_as_a_strkey() {
    // Round-trip rather than a hand-typed constant: the invariant is that
    // the StrKey resolves to the hex of the SAME 32 bytes. A literal typed
    // by hand would only test whether the literal was right.
    let bytes: [u8; 32] = core::array::from_fn(|i| i as u8);
    let hex = hex::encode(bytes);
    let strkey = stellar_strkey::LiquidityPool(bytes).to_string();
    assert!(
        strkey.starts_with('L'),
        "expected an L-strkey, got {strkey}"
    );

    assert_eq!(pool_id_from_text(&strkey).as_deref(), Some(hex.as_str()));
    // Pasting from a terminal or a chat window brings whitespace along.
    assert_eq!(
        pool_id_from_text(&format!("  {strkey} ")).as_deref(),
        Some(hex.as_str())
    );
    // Hex is NOT an identifier here — 0264 made StrKey the only accepted
    // form, and `path::pool_id_strkey` rejects hex on the detail route.
    assert!(pool_id_from_text(&hex).is_none());
    assert!(pool_id_from_text(&hex.to_uppercase()).is_none());
}

/// The same 32 bytes, two encodings, both well-formed — which is exactly
/// why the kind is a parameter and not a default. A soroban pool rendered
/// as `L…` would be a valid strkey for a pool that does not exist.
#[test]
fn the_same_bytes_render_differently_per_kind() {
    let hex = "0123456789abcdef".repeat(4);
    let classic = pool_id_hex_to_strkey(&hex, domain::PoolKind::Classic);
    let soroban = pool_id_hex_to_strkey(&hex, domain::PoolKind::Soroban);
    assert!(classic.starts_with('L'), "{classic}");
    assert!(soroban.starts_with('C'), "{soroban}");
    assert_ne!(classic, soroban);
    // Both round-trip to the same payload — the bytes never changed, only
    // the claim about what they identify.
    assert_eq!(
        stellar_strkey::LiquidityPool::from_string(&classic)
            .unwrap()
            .0,
        stellar_strkey::Contract::from_string(&soroban).unwrap().0,
    );
}

/// Both forms are accepted on the way in, for the same reason.
#[test]
fn a_contract_address_is_a_pool_id_too() {
    let hex = "0123456789abcdef".repeat(4);
    let soroban = pool_id_hex_to_strkey(&hex, domain::PoolKind::Soroban);
    assert_eq!(pool_id_from_text(&soroban).as_deref(), Some(hex.as_str()));
}
