//! Conditional GET (`ETag` / `If-None-Match` → `304 Not Modified`) — task 0292.
//!
//! `ETag = latest_ledger_sequence` (the cheap chain head from [`super::head`]).
//! On a request whose `If-None-Match` already names the current head, the
//! handler returns `304 Not Modified` with an empty body **before** running its
//! heavy query: idle polls cost a head probe and nothing else, and external API
//! clients (task 0277) whose polling we do not control stop re-reading the
//! warehouse on every tick.
//!
//! ## Scope: head is a version marker only for head-keyed responses
//!
//! The chain head is a valid `ETag` for any response whose content is a pure
//! function of the latest ledger — `/v1/network/stats` and the **live first
//! page** of the transaction / ledger lists. Data only changes when a new
//! ledger lands, so "head unchanged" ⇒ "body unchanged" ⇒ a `304` is never
//! stale.
//!
//! Cursored (historical, immutable) pages are deliberately excluded: the head
//! advances every ledger while their body never changes, so a head-keyed
//! `ETag` would revalidate to `200` on every poll — no win. Those want a
//! content / long-lived `ETag` instead (future work, see task 0292 notes).
//!
//! ## Why `304` must bypass [`cache_control::enforce_no_store_on_errors`]
//!
//! `304` is not a 2xx, so the blanket "non-2xx ⇒ `no-store`" middleware would
//! otherwise stamp `no-store` on it — which contradicts the conditional-GET
//! contract (the client must keep revalidating its cached body). The middleware
//! exempts `304`; this module owns the `Cache-Control` the `304` carries (the
//! same `LIVE` tier the matching `200` would have).

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::cache_control;

/// Strong `ETag` value for a chain head, e.g. head `123` → `"123"` (quoted per
/// RFC 7232 §2.3). `head` is an `i64` decimal, always valid header bytes.
///
/// Use this only when the body is **byte-stable** for a given head — i.e. the
/// live lists, whose envelope is a pure function of the head + query. The
/// `/network/stats` body is NOT byte-stable (it carries a per-SELECT
/// `generated_at` wall-clock) and so must use [`weak_etag_for`].
pub fn etag_for(head: i64) -> HeaderValue {
    HeaderValue::from_str(&format!("\"{head}\"")).expect("decimal etag is valid ascii")
}

/// Weak `ETag` value for a chain head, e.g. head `123` → `W/"123"`.
///
/// A weak validator asserts "same head ⇒ semantically equivalent", not byte
/// equality (RFC 7232 §2.1). It is the correct tag for `/network/stats`: two
/// `200`s at the same head can differ byte-for-byte (the `generated_at`
/// timestamp, or a cache eviction / last-good-fallback recompute under an
/// unchanged head), which a strong tag must not allow. `If-None-Match` always
/// uses weak comparison, so the short-circuit works identically either way.
pub fn weak_etag_for(head: i64) -> HeaderValue {
    HeaderValue::from_str(&format!("W/\"{head}\"")).expect("decimal weak etag is valid ascii")
}

/// Does the request's `If-None-Match` already name the current `head`?
///
/// Implements the RFC 7232 §3.2 list match: any of `*`, `"<head>"`, or
/// `W/"<head>"` (weak comparison — the only comparison `If-None-Match` uses)
/// counts as a hit. Tolerant of surrounding whitespace and multiple
/// comma-separated tags. A malformed or absent header is "no match" → the
/// caller falls through to the heavy query (fails safe to a fresh `200`).
pub fn if_none_match_satisfied(headers: &HeaderMap, head: i64) -> bool {
    let Some(raw) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let current = head.to_string();
    raw.split(',').any(|tag| {
        let tag = tag.trim();
        if tag == "*" {
            return true;
        }
        // Weak comparison: drop an optional `W/` prefix, then the quotes.
        let tag = tag.strip_prefix("W/").unwrap_or(tag);
        tag.strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .is_some_and(|t| t == current)
    })
}

/// `304 Not Modified` carrying an explicit `ETag` value: empty body + the
/// `LIVE` `Cache-Control` (so the client keeps revalidating). A `304` must echo
/// the same validator the matching `200` would, per RFC 7232 §4.1 — so the
/// caller passes the *same* tag (strong for lists, weak for stats).
fn not_modified_with(etag: HeaderValue) -> Response {
    let mut resp = StatusCode::NOT_MODIFIED.into_response();
    let h = resp.headers_mut();
    h.insert(header::ETAG, etag);
    h.insert(header::CACHE_CONTROL, cache_control::LIVE);
    resp
}

/// `304` with a **strong** head `ETag` — for the live lists (byte-stable body).
pub fn not_modified(head: i64) -> Response {
    not_modified_with(etag_for(head))
}

/// `304` with a **weak** head `ETag` — for `/network/stats` (see
/// [`weak_etag_for`]).
pub fn not_modified_weak(head: i64) -> Response {
    not_modified_with(weak_etag_for(head))
}

/// Attach a **strong** head `ETag` to an already-built `200` (live lists). The
/// caller is expected to have already set `Cache-Control` (`LIVE`).
pub fn attach_etag(resp: &mut Response, head: i64) {
    resp.headers_mut().insert(header::ETAG, etag_for(head));
}

/// Attach a **weak** head `ETag` to an already-built `200` (`/network/stats`).
pub fn attach_weak_etag(resp: &mut Response, head: i64) {
    resp.headers_mut().insert(header::ETAG, weak_etag_for(head));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_inm(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::IF_NONE_MATCH, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn etag_is_quoted_decimal() {
        assert_eq!(etag_for(123).to_str().unwrap(), "\"123\"");
        assert_eq!(etag_for(0).to_str().unwrap(), "\"0\"");
    }

    #[test]
    fn weak_etag_has_w_prefix() {
        assert_eq!(weak_etag_for(123).to_str().unwrap(), "W/\"123\"");
    }

    #[test]
    fn weak_etag_is_accepted_by_if_none_match() {
        // A client echoing our weak stats tag must still short-circuit.
        assert!(if_none_match_satisfied(&headers_with_inm("W/\"123\""), 123));
    }

    #[test]
    fn not_modified_weak_carries_weak_etag_and_live() {
        let resp = not_modified_weak(456);
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            resp.headers().get(header::ETAG).unwrap().to_str().unwrap(),
            "W/\"456\""
        );
        assert_eq!(
            resp.headers()
                .get(header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "public, max-age=0, must-revalidate"
        );
    }

    #[test]
    fn exact_strong_tag_matches() {
        assert!(if_none_match_satisfied(&headers_with_inm("\"123\""), 123));
    }

    #[test]
    fn weak_tag_matches() {
        assert!(if_none_match_satisfied(&headers_with_inm("W/\"123\""), 123));
    }

    #[test]
    fn star_matches_any_head() {
        assert!(if_none_match_satisfied(&headers_with_inm("*"), 123));
        assert!(if_none_match_satisfied(&headers_with_inm("*"), 0));
    }

    #[test]
    fn tag_in_a_list_matches() {
        assert!(if_none_match_satisfied(
            &headers_with_inm("\"1\", \"123\", W/\"7\""),
            123
        ));
    }

    #[test]
    fn different_head_does_not_match() {
        assert!(!if_none_match_satisfied(&headers_with_inm("\"999\""), 123));
        assert!(!if_none_match_satisfied(
            &headers_with_inm("\"1\", \"2\""),
            123
        ));
    }

    #[test]
    fn absent_header_does_not_match() {
        assert!(!if_none_match_satisfied(&HeaderMap::new(), 123));
    }

    #[test]
    fn unquoted_or_malformed_does_not_match() {
        // Bare token without quotes is not a valid entity-tag — never a hit.
        assert!(!if_none_match_satisfied(&headers_with_inm("123"), 123));
        assert!(!if_none_match_satisfied(&headers_with_inm("\"123"), 123));
    }

    #[test]
    fn not_modified_has_status_etag_and_live_cache_control() {
        let resp = not_modified(456);
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            resp.headers().get(header::ETAG).unwrap().to_str().unwrap(),
            "\"456\""
        );
        assert_eq!(
            resp.headers()
                .get(header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "public, max-age=0, must-revalidate"
        );
    }

    #[test]
    fn attach_etag_sets_header_on_200() {
        let mut resp = "ok".into_response();
        attach_etag(&mut resp, 789);
        assert_eq!(
            resp.headers().get(header::ETAG).unwrap().to_str().unwrap(),
            "\"789\""
        );
    }
}
