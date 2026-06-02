//! Query classifier: maps raw `q` to the `(hash_bytes, strkey_prefix)`
//! pair consumed by `22_get_search.sql`.
//!
//! Two derived inputs only — the SQL itself decides which CTE branches
//! fire based on which input is non-NULL. Keeping the classifier this
//! narrow means there is no per-entity dispatch logic in Rust to drift
//! from the SQL contract.

/// Classifier output. `None` means "this branch should not fire".
#[derive(Debug, Default, Clone)]
pub struct Classified {
    /// 32 bytes if `q` parses as 64-char hex or as a full CAP-38/SEP-23
    /// `L…` pool strkey; drives the `transaction` and `pool` exact-match
    /// CTEs (same BYTEA(32) shape on the wire).
    pub hash_bytes: Option<Vec<u8>>,
    /// Upper-cased StrKey or its prefix when `q` matches Stellar StrKey
    /// shape (full 56 chars or any prefix of `G…` / `C…`); drives the
    /// `account` and `contract` prefix CTEs.
    pub strkey_prefix: Option<String>,
}

/// Classify a trimmed, non-empty `q`.
pub fn classify(q: &str) -> Classified {
    let mut out = Classified::default();

    // 32-byte hex (64 chars). Try this first — it is the highest-
    // information shape and unambiguous.
    if q.len() == 64
        && let Ok(bytes) = hex::decode(q)
    {
        out.hash_bytes = Some(bytes);
        return out;
    }

    // Full CAP-38 / SEP-23 L-strkey (liquidity pool). Decodes to the
    // same 32-byte hash the pool CTE matches on, so feeding `hash_bytes`
    // dispatches to the existing pool lookup without an extra SQL branch.
    //
    // Partial L-prefix is intentionally NOT classified: pool storage is
    // raw `BYTEA(32)` with no text mirror column, so a `LIKE 'L%'`
    // prefix scan has nothing to scan. Partial-L prefix matching is
    // tracked in backlog task 0271.
    let upper = q.to_ascii_uppercase();
    if let Ok(stellar_strkey::LiquidityPool(bytes)) =
        stellar_strkey::LiquidityPool::from_string(&upper)
    {
        out.hash_bytes = Some(bytes.to_vec());
        return out;
    }

    // StrKey shape (full or prefix of G… / C…). The DB index is
    // `text_pattern_ops` so prefix `LIKE 'PREFIX%'` is the served
    // branch — both the full StrKey and any non-empty prefix work
    // identically against the index. Full vs partial is read off the
    // string length by `Classified::is_fully_typed`.
    if is_strkey_prefix(&upper, 'G') || is_strkey_prefix(&upper, 'C') {
        out.strkey_prefix = Some(upper);
        return out;
    }

    out
}

/// Returns true when `s` could be a prefix of a StrKey starting with
/// `prefix`: it begins with `prefix`, every byte is in the StrKey
/// base32 alphabet (`A-Z` and `2-7`), and length ∈ [2, 56].
///
/// Cheap checks (length + prefix byte) come first so the alphabet scan
/// only runs on candidates that already passed the shape gate.
fn is_strkey_prefix(s: &str, prefix: char) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if !(2..=56).contains(&len) || bytes[0] != prefix as u8 {
        return false;
    }
    bytes.iter().all(|b| matches!(b, b'A'..=b'Z' | b'2'..=b'7'))
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn short_strkey_prefix_under_two_chars_rejected() {
        // Single-char "G" is too narrow — would force a full-table
        // index range scan. Reject as garbage.
        let out = classify("G");
        assert!(out.strkey_prefix.is_none());
    }
}
