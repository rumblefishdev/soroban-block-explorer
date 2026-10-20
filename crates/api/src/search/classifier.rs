//! Query classifier: maps raw `q` to the `(hash_bytes, strkey_prefix)`
//! pair consumed by `search::queries`.
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
    /// `(asset_code, issuer G-StrKey)` when `q` is a fully-qualified asset —
    /// `CODE:ISSUER` (the canonical SEP / SDK form) or `CODE-ISSUER` (the shape
    /// our own `/assets/:id` routes emit, so users paste it back). Drives an
    /// exact asset lookup instead of the code substring scan.
    pub code_issuer: Option<(String, String)>,
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

    // Fully-qualified asset, `CODE:ISSUER` or `CODE-ISSUER`. This is the most
    // precise thing a user can type and it used to classify as nothing at all:
    // the asset arm then hunted a 60+ character needle through ≤12 character
    // codes (provably empty) and the account arm never fired because the string
    // does not start with `G`. The most specific query returned a blank page
    // while the vaguest one returned impostors (task 0485).
    if let Some(pair) = split_code_issuer(q) {
        out.code_issuer = Some(pair);
        return out;
    }

    // An address with a sub-account id is its base address: an `M…` its
    // account, a `W…` (CAP-0084) its contract. Only the full key: a partial
    // one cannot be turned into its base.
    if let Some(base) = crate::common::strkey::account_of_muxed(&upper)
        .or_else(|| crate::common::strkey::contract_of_muxed(&upper))
    {
        out.strkey_prefix = Some(base);
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

/// Split a fully-qualified asset into `(code, issuer)`.
///
/// Splits on the LAST `:` or `-`, which is unambiguous: a Stellar asset code is
/// `alphanum4` / `alphanum12`, so neither separator can occur inside one, and a
/// G-StrKey is base32 (`A-Z`, `2-7`) so neither can occur in the issuer either.
/// The issuer is validated in full — `from_string` checks the CRC — so a typo'd
/// key falls through to the ordinary substring search rather than returning a
/// confidently empty page.
fn split_code_issuer(q: &str) -> Option<(String, String)> {
    let (code, issuer) = q.rsplit_once([':', '-'])?;
    if code.is_empty() || code.len() > 12 || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    // Codes are case-sensitive on-chain and matched case-insensitively here, so
    // the original casing is kept; only the StrKey is normalised.
    let issuer = issuer.to_ascii_uppercase();
    stellar_strkey::ed25519::PublicKey::from_string(&issuer).ok()?;
    Some((code.to_string(), issuer))
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
mod tests;
