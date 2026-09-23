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
    let raw = raw.trim();
    // Both forms are accepted because both are real pool ids: a protocol pool
    // is addressed by its SEP-23 `L…` strkey, a soroban pool by the contract
    // it IS. They cannot be confused — the prefix byte differs — so trying one
    // then the other is a total parse, not a guess.
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

/// The wire identifier for a pool ROW, from its raw stored discriminant.
///
/// The drift fallback lives HERE and nowhere else. A discriminant outside
/// [`domain::PoolKind`] is schema drift, and every surface has to answer the
/// same way about it: `pool_id` is a required field, so there is no "no
/// identifier" to return, and classic is the form all but ~1.3% of pools take.
/// The pools handler and the search row used to spell this
/// `unwrap_or(PoolKind::Classic)` separately, each under a comment claiming the
/// classic encoding was used "only when the row says classic" — which is
/// exactly what a fallback is not.
pub(crate) fn pool_identifier(pool_id_hex: &str, raw_kind: i16) -> String {
    pool_id_hex_to_strkey(
        pool_id_hex,
        domain::PoolKind::try_from(raw_kind).unwrap_or(domain::PoolKind::Classic),
    )
}

/// The wire form of a pool's 32 bytes, chosen by its KIND.
///
/// The bytes cannot say which encoding is right, and both encodings accept any
/// 32 bytes — so rendering a soroban pool's contract payload as a SEP-23 `L…`
/// yields a **well-formed strkey for a pool that does not exist**: no panic, no
/// error, a wrong id on the page and a link that answers nothing. The kind is
/// therefore a parameter, not a default.
pub(crate) fn pool_id_hex_to_strkey(hex_str: &str, kind: domain::PoolKind) -> String {
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
    // Double `.to_string()` is intentional: the inherent `to_string` returns
    // `heapless::String<56>` (no_std); the second (via `Display`) bridges to
    // `std::String`.
    match kind {
        domain::PoolKind::Classic => stellar_strkey::LiquidityPool(payload)
            .to_string()
            .to_string(),
        domain::PoolKind::Soroban => stellar_strkey::Contract(payload).to_string().to_string(),
    }
}

#[cfg(test)]
mod tests;
