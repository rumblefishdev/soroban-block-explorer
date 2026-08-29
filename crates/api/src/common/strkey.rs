//! Shared primitive: Stellar StrKey shape check.
//!
//! Lives at the `common::*` layer (not under `filters` or `path`) because
//! both consumers — query-string filter validators ([`crate::common::filters`])
//! and URL path validators ([`crate::common::path`]) — need the same check
//! but emit different envelope codes (`invalid_filter` vs
//! `invalid_contract_id` / `invalid_account_id`). Keeping the shape check
//! here avoids a peer module depending on another peer module purely for
//! a five-line helper.
//!
//! The shape rule is a verbatim port of the Stellar StrKey grammar
//! restricted to the prefix + body + length checks (CRC validation is
//! intentionally omitted — see [`is_strkey_shape`] doc).

/// Returns `true` iff `value` is exactly 56 characters, starts with
/// `prefix`, and every byte is in the RFC 4648 base32 alphabet
/// (`A-Z` and `2-7`).
///
/// `bytes()` (not `chars()`) — base32 is ASCII-only, so byte iteration
/// is safe and skips the UTF-8 decode.
///
/// `prefix` is enforced strictly: a value that passes the alphabet +
/// length checks but starts with the wrong prefix character is rejected.
/// This is what stops a `G…` account StrKey from sneaking through a
/// contract-id validator (the alphabet check alone would accept it
/// because `G` is in `A-Z`).
///
/// **CRC is not validated** — the shape check IS the validation, not a
/// fast path before a stricter step. Per ADR 0037 the relevant DB
/// columns (`accounts.account_id`, `soroban_contracts.contract_id`) are
/// `VARCHAR(56) NOT NULL UNIQUE` matched by plain string equality; a
/// wrong-CRC StrKey that passes the shape check simply fails to match
/// any row, producing the same UX as a non-existent address. The shape
/// check exists to catch the common typo / wrong-prefix / wrong-alphabet
/// cases loudly with a 400 envelope instead of silently returning `[]`
/// or 404 on a junk address.
pub(crate) fn is_strkey_shape(value: &str, prefix: char) -> bool {
    value.len() == 56
        && value.starts_with(prefix)
        && value
            .bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'2'..=b'7'))
}

/// The same free-text box also has to accept a pool's own identifier.
///
/// Without this, pasting `LD7K…` into the pools filter runs it through
/// `normalize_asset_codes` and matches it as a substring of an asset CODE —
/// which finds nothing, so the page answers "no such pool" about a pool that
/// exists. A confident wrong answer, not a missing convenience (task 0470).
///
/// **StrKey only.** Task 0264 made the `L…` SEP-23 form canonical across every
/// surface and dropped hex deliberately — the project was pre-deploy, so there
/// were no hex bookmarks to preserve — and `path::pool_id_strkey` still tells
/// callers that "hex form is no longer accepted".
///
/// `search::classifier` does accept 64-char hex, but that is the one endpoint
/// 0264 explicitly deferred, not a precedent. Copying it here would spread a
/// known exception into a third place.
///
/// Dropping hex also removes a trap: `hex::decode` accepts any even-length
/// string over `[0-9a-f]`, so `FACE`, `BEEF` and `CAFE` — all valid asset
/// codes — would parse as identifiers and search for a pool that does not
/// exist.
///
/// Returns the lowercase hex the `pool_id` column stores.
pub(crate) fn pool_id_from_text(raw: &str) -> Option<String> {
    // `C...` accepted since task 0374: soroban pools are addressed by their
    // contract strkey, and pasting one into the same box must find the pool.
    // A contract strkey that is NOT a pool simply selects nothing — same as
    // an unknown `L...`.
    let raw = raw.trim();
    if let Ok(stellar_strkey::LiquidityPool(bytes)) =
        stellar_strkey::LiquidityPool::from_string(raw)
    {
        return Some(hex::encode(bytes));
    }
    match stellar_strkey::Contract::from_string(raw) {
        Ok(stellar_strkey::Contract(bytes)) => Some(hex::encode(bytes)),
        Err(_) => None,
    }
}

pub(crate) fn pool_id_hex_to_strkey(hex_str: &str) -> String {
    assert_eq!(
        hex_str.len(),
        64,
        "pool_id hex must be exactly 64 chars (got {})",
        hex_str.len()
    );
    let bytes = hex::decode(hex_str)
        .unwrap_or_else(|_| panic!("pool_id hex contains non-hex chars: {hex_str}"));
    let payload: [u8; 32] = bytes
        .try_into()
        .expect("32 bytes — guaranteed by 64-char length assert above");
    // Double `.to_string()` is intentional: the inherent
    // `LiquidityPool::to_string` returns `heapless::String<56>` (no_std);
    // the second `.to_string()` (via `Display`) bridges to `std::String`.
    stellar_strkey::LiquidityPool(payload)
        .to_string()
        .to_string()
}

/// 64-char hex → `C...` contract strkey. The id form for SOROBAN pools
/// (task 0374): their 32 stored bytes are a contract-address payload, and
/// an `L...` render of the same bytes would be a well-formed WRONG key.
pub(crate) fn contract_hex_to_strkey(hex_str: &str) -> String {
    assert_eq!(
        hex_str.len(),
        64,
        "contract id hex must be exactly 64 chars (got {})",
        hex_str.len()
    );
    let bytes = hex::decode(hex_str)
        .unwrap_or_else(|_| panic!("contract id hex contains non-hex chars: {hex_str}"));
    let payload: [u8; 32] = bytes
        .try_into()
        .expect("32 bytes — guaranteed by 64-char length assert above");
    // Same heapless→std double `.to_string()` bridge as above.
    stellar_strkey::Contract(payload).to_string().to_string()
}

#[cfg(test)]
mod tests {
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
        let strkey = pool_id_hex_to_strkey(&hex);
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
        let strkey = pool_id_hex_to_strkey(&hex);
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
        let _ = pool_id_hex_to_strkey("abc");
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
}
