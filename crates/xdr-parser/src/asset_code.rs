//! One canonical asset-code normalizer (task 0359, devils-advocate concern 3).
//!
//! A classic 4/12-byte asset code is arbitrary chain bytes, zero-padded.
//! Historically three incompatible normalizations coexisted across the pipelines
//! (strict `from_utf8` with a literal `<invalid>` fallback in op details / the
//! `asset_id` surrogate; `from_utf8_lossy` U+FFFD in trustline & balance changes;
//! cut-at-first-NUL then lossy in SAC identity). The same malformed code therefore
//! produced a different string down each path, hence a different `asset_id`
//! surrogate for one asset (identity fracture), while every distinct invalid code
//! from one issuer collapsed onto a single `<invalid>:ISSUER` id (silent
//! cross-asset aggregation on the asset page).
//!
//! Injective policy: valid UTF-8 with trailing NULs stripped maps to that string;
//! anything else maps to `0x` + lowercase hex of the raw bytes — never a shared
//! sentinel, so distinct byte sequences never collide. Conformant codes are
//! byte-identical to the old strict path, so no surrogate-id churn; only malformed
//! codes re-key.

use stellar_xdr::AssetCode;

/// Normalize raw asset-code bytes to the canonical string used everywhere an
/// `asset_id` surrogate (or a user-facing asset code) is derived.
pub fn asset_code_str(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_end_matches('\0').to_string(),
        Err(_) => format!("0x{}", hex::encode(bytes)),
    }
}

/// The raw code bytes of an `AssetCode` (the issuer-less `allow_trust` code).
pub fn asset_code_bytes(code: &AssetCode) -> &[u8] {
    match code {
        AssetCode::CreditAlphanum4(c) => c.as_slice(),
        AssetCode::CreditAlphanum12(c) => c.as_slice(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_code_is_unchanged_trailing_nuls_stripped() {
        assert_eq!(asset_code_str(b"USDC"), "USDC");
        assert_eq!(asset_code_str(b"USD\0"), "USD"); // 4-byte zero-padded
        assert_eq!(asset_code_str(b"USDC\0\0\0\0\0\0\0\0"), "USDC"); // 12-byte
        assert_eq!(asset_code_str(b""), "");
    }

    #[test]
    fn interior_nul_is_preserved() {
        // An interior NUL is part of the code — only TRAILING padding is stripped.
        assert_eq!(asset_code_str(b"US\0C"), "US\0C");
    }

    #[test]
    fn invalid_utf8_is_hex_not_a_shared_sentinel() {
        // Two DISTINCT malformed codes must yield DISTINCT strings (no collision).
        let a = asset_code_str(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let b = asset_code_str(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(a, "0xdeadbeef");
        assert_eq!(b, "0xffffffff");
        assert_ne!(a, b);
    }
}
