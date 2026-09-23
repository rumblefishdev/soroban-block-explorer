//! Validators for typed path parameters (`/v1/<resource>/:id`).
//!
//! The `:id` placeholder takes different shapes per resource:
//!
//!   | Resource          | Shape                          | Helper                  |
//!   | ----------------- | ------------------------------ | ----------------------- |
//!   | `transactions`    | 64-char hex                    | [`parse_hash`]          |
//!   | `contracts`       | StrKey, prefix `C`             | [`strkey`] with `'C'`   |
//!   | `accounts`        | StrKey, prefix `G`             | [`strkey`] with `'G'`   |
//!   | `liquidity-pools` | StrKey, prefix `L` (SEP-23)    | [`pool_id_strkey`]      |
//!   | `ledgers`         | numeric `u32`                  | [`sequence`]            |
//!
//! Each helper short-circuits the handler before any DB / S3 call —
//! malformed input maps to a flat ADR 0008 `ErrorEnvelope` with one of
//! the canonical `INVALID_HASH` / `INVALID_CONTRACT_ID` /
//! `INVALID_ACCOUNT_ID` / `INVALID_POOL_ID` / `INVALID_SEQUENCE` codes
//! (see [`crate::common::errors`]).
//!
//! Why not just reuse [`crate::common::filters::strkey`]? `filters::*`
//! emits `invalid_filter` and assumes the value came from a `filter[key]=`
//! query parameter (the error `details` carry a `"filter"` field).
//! Path params have a different code surface and a different `details`
//! shape (`"param"` not `"filter"`) — the helpers below mirror the
//! `filters::*` validation logic but emit path-appropriate envelopes so
//! a client reading the `code` knows immediately whether the bad value
//! came from the URL path or a query string.

#![allow(clippy::result_large_err)]

use axum::response::Response;

use super::errors;
use super::strkey::is_strkey_shape;

// ---------------------------------------------------------------------------
// Hash (transactions)
// ---------------------------------------------------------------------------

/// Validate a transaction-hash path parameter and return the lowercase
/// canonical form for downstream DB / archive lookup.
///
/// Stellar transaction hashes are SHA-256 outputs serialised as 64
/// lowercase or uppercase hex characters. The validator accepts either
/// case so clients can use whatever their indexer / explorer surfaced,
/// and returns the lowercase form on success — the `transactions::hash`
/// column stores lowercase, and downstream archive matching is
/// case-sensitive, so coupling normalisation with validation keeps a
/// future caller from silently 404-ing on uppercase input.
pub fn parse_hash(value: &str) -> Result<String, Response> {
    if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err(errors::bad_request_with_details(
            errors::INVALID_HASH,
            "hash must be a 64-character hexadecimal string",
            serde_json::json!({ "param": "hash", "received": value }),
        ))
    }
}

// ---------------------------------------------------------------------------
// StrKey (contracts, accounts)
// ---------------------------------------------------------------------------

/// Validate a Stellar StrKey path parameter (account `G…`, contract `C…`).
///
/// Same shape rule as [`crate::common::filters::strkey`] (RFC 4648 base32,
/// 56 chars, required prefix), but the failure envelope carries the
/// path-specific code (`INVALID_CONTRACT_ID` for `'C'`, `INVALID_ACCOUNT_ID`
/// for `'G'`) and a `"param"` field in `details` instead of `"filter"`.
///
/// CRC validation is skipped — wrong-CRC StrKey passes shape and falls
/// through to DB lookup, which returns `Ok(None)` → 404 `not_found`. UX
/// is identical to a non-existent resource (`/contracts/CCAB...XYZ`
/// where the address is well-formed but never indexed). The shape check
/// catches the common case of a typo / wrong prefix / wrong alphabet
/// loudly with a 400 envelope instead of silently returning 404 on a
/// junk address.
pub fn strkey(value: &str, prefix: char, param: &str) -> Result<(), Response> {
    let code = match prefix {
        'C' => errors::INVALID_CONTRACT_ID,
        'G' => errors::INVALID_ACCOUNT_ID,
        // Future-proofing: any other prefix (M for muxed, T for pre-auth, …)
        // is a path-parameter validation failure, so route through
        // `INVALID_ID` rather than `INVALID_FILTER` (which is reserved for
        // `filter[...]` query params). Add a dedicated const here if a
        // real consumer appears.
        _ => errors::INVALID_ID,
    };

    if is_strkey_shape(value, prefix) {
        Ok(())
    } else {
        Err(errors::bad_request_with_details(
            code,
            format!(
                "{param} must be a 56-character Stellar StrKey starting with '{prefix}' (RFC 4648 base32)"
            ),
            serde_json::json!({ "param": param, "received": value, "expected_prefix": prefix.to_string() }),
        ))
    }
}

// ---------------------------------------------------------------------------
// StrKey (liquidity pools) — SEP-23 `L...`
// ---------------------------------------------------------------------------

/// Validate a `pool_id`-shaped path parameter (SEP-23 strkey `L...`) and
/// return the 64-char lowercase-hex internal form for DB lookup.
///
/// LP `pool_id` is `BYTEA(32)` in the DB per ADR 0024; the canonical
/// user-facing form (per CAP-38 / SEP-23) is a 56-char strkey starting
/// with `L`. Stellar Lab, stellar.expert, and Horizon all display the
/// strkey form. Hex form is no longer accepted on input — clients must
/// supply the strkey returned by `/v1/liquidity-pools` or shown in
/// external explorers.
///
/// On success returns the 64-char lowercase-hex payload (32 bytes
/// formatted as hex) for downstream DB lookup. The strkey decode
/// implicitly validates the version byte, base32 alphabet, length, and
/// CRC16 checksum — wrong-CRC values are rejected here with 400 rather
/// than falling through to a 404 on DB miss (different UX from the
/// `strkey` helper for accounts/contracts: pool decode is CRC-strict
/// because the internal DB form is the hash, not the strkey itself).
pub fn pool_id_strkey(value: &str, param: &str) -> Result<String, Response> {
    match stellar_strkey::LiquidityPool::from_string(value) {
        Ok(stellar_strkey::LiquidityPool(bytes)) => Ok(hex::encode(bytes)),
        Err(_) => Err(errors::bad_request_with_details(
            errors::INVALID_POOL_ID,
            format!(
                "{param} must be a 56-character Stellar StrKey starting with 'L' (SEP-23 canonical form)"
            ),
            serde_json::json!({
                "param": param,
                "received": value,
                "expected_prefix": "L",
                "hint": "use the strkey (L...) returned by /v1/liquidity-pools or shown in stellar.expert; hex form is no longer accepted",
            }),
        )),
    }
}

// ---------------------------------------------------------------------------
// Sequence (ledgers)
// ---------------------------------------------------------------------------

/// Validate a ledger-sequence path parameter.
///
/// Stellar ledger sequences are monotonically-increasing `u32` values
/// starting at 1 (ledger 0 does not exist in Stellar — genesis is
/// sequence 1; see Stellar Core's `LedgerHeader.ledgerSeq: uint32`).
/// The network has been below 2^32 since genesis and is expected to
/// stay there for the lifetime of this codebase. Zero / negative /
/// non-numeric / overflow inputs map to 400 `INVALID_SEQUENCE`.
///
/// Used by the `/v1/ledgers/:sequence` endpoint (task 0047); declared
/// here alongside the other path validators so the canonical set lives
/// in one module.
pub fn sequence(value: &str) -> Result<u32, Response> {
    let invalid = || {
        errors::bad_request_with_details(
            errors::INVALID_SEQUENCE,
            "sequence must be a positive integer that fits in 32 bits",
            serde_json::json!({ "param": "sequence", "received": value }),
        )
    };
    let n = value.parse::<u32>().map_err(|_| invalid())?;
    (n != 0).then_some(n).ok_or_else(invalid)
}

#[cfg(test)]
mod tests;
