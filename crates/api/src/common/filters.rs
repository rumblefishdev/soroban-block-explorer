//! Validators for typed `filter[key]` query parameters.
//!
//! The `filter[key]=value` convention is project-wide (see ADR 0008 and
//! task 0043) — every list endpoint exposes some subset of keys. The
//! *parsing* of those keys is already handled cleanly by `serde` renames
//! on the endpoint's own `ListParams` DTO; what handlers actually
//! duplicate is the *validation* of each value (StrKey shape, enum name
//! recognition) and the mapping of failures to the canonical
//! `ErrorEnvelope { code: "invalid_filter" }`.
//!
//! This module provides those validators. Each returns `Ok(T)` on success
//! or a fully built 400 [`Response`] on failure, so handlers can
//! `?`-propagate into their list endpoint bodies.

#![allow(clippy::result_large_err)]

use std::str::FromStr;

use axum::response::Response;
use chrono::{DateTime, Utc};

use super::errors;

/// Shape-validate a Stellar StrKey: required prefix character + 56 total
/// chars in the RFC 4648 base32 alphabet (`A-Z` + `2-7`).
///
/// CRC validation is deliberately skipped — the shape check this function
/// performs IS the validation, not a fast path before a stricter DB check.
/// Per ADR 0037, `accounts.account_id` and `soroban_contracts.contract_id`
/// are `VARCHAR(56) NOT NULL UNIQUE` columns matched by plain string
/// equality; a wrong-CRC StrKey that passes the shape check will simply
/// not match any row, producing an empty result set with the same UX as
/// a non-existent address — acceptable for a read-only API. The benefit
/// of the shape check is catching the common typo / wrong-prefix cases
/// loudly with a 400 envelope instead of silently returning `[]`.
pub fn strkey(value: &str, prefix: char, filter_key: &str) -> Result<(), Response> {
    if super::strkey::is_strkey_shape(value, prefix) {
        Ok(())
    } else {
        Err(errors::bad_request_with_details(
            errors::INVALID_FILTER,
            format!(
                "filter[{filter_key}] is not a valid Stellar StrKey (prefix {prefix}, 56 chars, base32)"
            ),
            serde_json::json!({ "filter": filter_key, "received": value, "expected_prefix": prefix.to_string() }),
        ))
    }
}

/// Validate a StrKey only when present.
///
/// Common handler pattern — `Option<String>` filter param, validate only
/// when client supplied a value. Saves the `if let Some(v) = ... && let
/// Err(resp) = ...` let-chain at every call site.
pub fn strkey_opt(value: Option<&str>, prefix: char, filter_key: &str) -> Result<(), Response> {
    match value {
        Some(v) => strkey(v, prefix, filter_key),
        None => Ok(()),
    }
}

/// `filter[source_account]` when present: a `G…`, or an `M…` (the same
/// account with a sub-account id) resolved to its `G…`.
pub fn account_id_opt(value: Option<&str>) -> Result<Option<String>, Response> {
    let Some(v) = value else {
        return Ok(None);
    };
    if let Some(account) = super::strkey::account_of_muxed(v) {
        return Ok(Some(account));
    }
    strkey(v, 'G', "source_account")?;
    Ok(Some(v.to_string()))
}

/// `filter[contract_id]` when present: a `C…`, or a `W…` (the same contract
/// with a sub-account id, CAP-0084) resolved to its `C…`.
pub fn contract_id_opt(value: Option<&str>) -> Result<Option<String>, Response> {
    let Some(v) = value else {
        return Ok(None);
    };
    if let Some(contract) = super::strkey::contract_of_muxed(v) {
        return Ok(Some(contract));
    }
    strkey(v, 'C', "contract_id")?;
    Ok(Some(v.to_string()))
}

/// Parse a `filter[key]` string into an enum type via [`FromStr`].
///
/// Wraps the enum's parse error in the canonical `invalid_filter`
/// envelope so handlers do not hand-craft the response. Suitable for any
/// type whose `FromStr` implementation returns an error for unknown
/// variant names — e.g. `domain::OperationType`, `domain::TokenType`.
///
/// `kind_hint` lets the call site tighten the error message with a
/// type-specific noun (e.g. `Some("operation type")` →
/// *"filter[operation_type] is not a recognized operation type"*).
/// Pass `None` for the generic *"is not a recognized value"* phrasing.
pub fn parse_enum<T>(value: &str, filter_key: &str, kind_hint: Option<&str>) -> Result<T, Response>
where
    T: FromStr,
{
    T::from_str(value).map_err(|_| {
        let what = kind_hint.unwrap_or("value");
        errors::bad_request_with_details(
            errors::INVALID_FILTER,
            format!("filter[{filter_key}] is not a recognized {what}"),
            serde_json::json!({ "filter": filter_key, "received": value }),
        )
    })
}

/// Parse a `filter[key]` enum only when present. See [`strkey_opt`] for the
/// rationale — symmetric helper for the enum case.
pub fn parse_enum_opt<T>(
    value: Option<&str>,
    filter_key: &str,
    kind_hint: Option<&str>,
) -> Result<Option<T>, Response>
where
    T: FromStr,
{
    value
        .map(|s| parse_enum::<T>(s, filter_key, kind_hint))
        .transpose()
}

/// Reject literal SQL wildcard characters (`%`, `_`) in a `filter[key]`
/// value used for trigram / `ILIKE` substring search.
///
/// The handler-side wraps such filters as `'%' || $N || '%'` server-side
/// so the client passes a plain substring. A literal `%` or `_` from the
/// caller would silently change `LIKE` semantics — `%` matches "anything",
/// `_` matches "any one char" — and the resulting result set is
/// over-broad without explanation. Not an injection risk (values are
/// always bound, never concatenated), purely a UX guard against silent
/// over-match. The 400 envelope tells the caller exactly which characters
/// are reserved instead of returning an unexplained large result set.
pub fn reject_sql_wildcards(value: &str, filter_key: &str) -> Result<(), Response> {
    if value.bytes().any(|b| b == b'%' || b == b'_') {
        Err(errors::bad_request_with_details(
            errors::INVALID_FILTER,
            format!("filter[{filter_key}] must not contain `%` or `_` (SQL wildcard literals)"),
            serde_json::json!({ "filter": filter_key, "received": value }),
        ))
    } else {
        Ok(())
    }
}

/// Reject literal SQL wildcards only when present. See [`strkey_opt`] for
/// the rationale — symmetric helper for the wildcard-reject case.
pub fn reject_sql_wildcards_opt(value: Option<&str>, filter_key: &str) -> Result<(), Response> {
    match value {
        Some(v) => reject_sql_wildcards(v, filter_key),
        None => Ok(()),
    }
}

/// Parse a `filter[key]` (or any query-string param) value as an ISO 8601
/// / RFC 3339 timestamp into `DateTime<Utc>`.
///
/// `chrono::DateTime::parse_from_rfc3339` accepts the strict subset of
/// ISO 8601 the explorer API documents (`2026-04-30T12:00:00Z`,
/// `2026-04-30T12:00:00+02:00`, etc.). Naive timestamps without a
/// timezone are rejected — the snapshot streams are TIMESTAMPTZ so the
/// API stays unambiguous. Failure surfaces as `INVALID_FILTER` with the
/// param name in `details`, matching the rest of `filters::*`.
pub fn parse_iso8601(value: &str, filter_key: &str) -> Result<DateTime<Utc>, Response> {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| {
            errors::bad_request_with_details(
                errors::INVALID_FILTER,
                format!("{filter_key} must be a valid ISO 8601 / RFC 3339 timestamp"),
                serde_json::json!({ "filter": filter_key, "received": value }),
            )
        })
}

#[cfg(test)]
mod tests;
