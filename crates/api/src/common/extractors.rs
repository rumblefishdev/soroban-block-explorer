//! axum extractors for the standard `?limit=&cursor=` query parameters.
//!
//! [`Pagination<P>`] is a handler argument extractor that reads `limit`
//! and `cursor` from the query string (tolerating unknown fields so it
//! composes with a sibling `Query<FilterParams>`), validates them, and
//! returns a `limit: u32` plus an optional decoded cursor payload.
//!
//! Validation failures surface the canonical `ErrorEnvelope` codes
//! (`invalid_limit`, `invalid_cursor`).

#![allow(clippy::result_large_err)]

use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use axum::response::Response;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::cursor::{self, CursorError, Direction};
use super::errors;

/// Default page size when the client omits `?limit=` (matches ADR 0008
/// guidance and current spec for every list endpoint).
const DEFAULT_LIMIT: u32 = 20;

/// Hard ceiling on `?limit=` values across every list endpoint.
const MAX_LIMIT: u32 = 100;

/// Raw deserialisation target for the two standard query parameters.
///
/// Uses `Option<String>` for `limit` (not `Option<u32>`) so non-numeric
/// values fall into our validator with an `INVALID_LIMIT` response rather
/// than being rejected by serde with a generic 422.
#[derive(Debug, Default, Deserialize)]
struct PaginationRaw {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

/// Validated pagination parameters with a decoded cursor payload.
///
/// Generic over `P` — the resource-specific cursor payload. Use
/// [`cursor::TsIdCursor`](super::cursor::TsIdCursor) for the common
/// `(created_at, id)` case.
///
/// `direction` is extracted from the cursor envelope (or
/// [`Direction::Next`] when the client did not send a cursor — first-page
/// requests are forward by definition). Handlers branch their SQL on this
/// field to walk forward (DESC) or backward (ASC + in-memory reverse).
#[derive(Debug)]
pub struct Pagination<P> {
    pub limit: u32,
    pub cursor: Option<P>,
    pub direction: Direction,
}

impl<P> Pagination<P> {
    /// `true` when the request carried a `?cursor=`. Used by
    /// [`common::pagination::finalize_page`] to decide whether to emit a
    /// `prev_cursor` on a forward walk (first-page requests have no
    /// predecessor).
    pub fn has_predecessor(&self) -> bool {
        self.cursor.is_some()
    }

    /// `pagination.limit + 1` as `i64` — the "fetch one extra row for
    /// peek" idiom every paginated query uses to detect a further page.
    pub fn fetch_limit(&self) -> i64 {
        i64::from(self.limit) + 1
    }
}

impl<P: DeserializeOwned> Pagination<P> {
    /// Validate a raw `?limit=&cursor=` pair using the project-default
    /// limit policy ([`DEFAULT_LIMIT`] / [`MAX_LIMIT`]).
    fn resolve_default(limit: Option<&str>, cursor: Option<&str>) -> Result<Self, Response> {
        let limit = validate_limit(limit)?;
        let (direction, cursor) = decode_cursor::<P>(cursor)?;
        Ok(Pagination {
            limit,
            cursor,
            direction,
        })
    }
}

// ---------------------------------------------------------------------------
// Validation primitives (also used by the FromRequestParts impl)
// ---------------------------------------------------------------------------

fn validate_limit(raw: Option<&str>) -> Result<u32, Response> {
    let Some(s) = raw else {
        return Ok(DEFAULT_LIMIT);
    };

    let parsed: u32 = s.parse().map_err(|_| {
        errors::bad_request_with_details(
            errors::INVALID_LIMIT,
            format!("limit must be an integer between 1 and {MAX_LIMIT}"),
            serde_json::json!({ "min": 1, "max": MAX_LIMIT, "received": s }),
        )
    })?;

    if parsed == 0 || parsed > MAX_LIMIT {
        return Err(errors::bad_request_with_details(
            errors::INVALID_LIMIT,
            format!("limit must be between 1 and {MAX_LIMIT}"),
            serde_json::json!({ "min": 1, "max": MAX_LIMIT, "received": parsed }),
        ));
    }

    Ok(parsed)
}

fn decode_cursor<P: DeserializeOwned>(
    raw: Option<&str>,
) -> Result<(Direction, Option<P>), Response> {
    let Some(s) = raw else {
        // First-page requests carry no cursor → forward direction by
        // definition.
        return Ok((Direction::Next, None));
    };

    match cursor::decode::<P>(s) {
        Ok((dir, p)) => Ok((dir, Some(p))),
        Err(CursorError::InvalidBase64) | Err(CursorError::InvalidPayload) => Err(
            errors::bad_request(errors::INVALID_CURSOR, "cursor is malformed or expired"),
        ),
    }
}

// ---------------------------------------------------------------------------
// FromRequestParts impl
// ---------------------------------------------------------------------------

/// Extractor impl uses the project-default limit policy
/// ([`DEFAULT_LIMIT`] / [`MAX_LIMIT`]).
///
/// Internally delegates to `axum::extract::Query<PaginationRaw>`, which
/// tolerates unknown fields in the query string — so a handler can pair
/// this extractor with a sibling `Query<FilterParams>` carrying the
/// `filter[...]` entries without conflict.
impl<S, P> FromRequestParts<S> for Pagination<P>
where
    S: Send + Sync,
    P: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Failure here means the query string itself is malformed (bad
        // percent-encoding, duplicate keys, …) — surface as INVALID_QUERY,
        // not INVALID_LIMIT, since the failure may have nothing to do with
        // the `limit` parameter.
        let Query(raw) = Query::<PaginationRaw>::from_request_parts(parts, state)
            .await
            .map_err(|e| {
                errors::bad_request(
                    errors::INVALID_QUERY,
                    format!("could not parse query parameters: {e}"),
                )
            })?;
        Pagination::<P>::resolve_default(raw.limit.as_deref(), raw.cursor.as_deref())
    }
}

#[cfg(test)]
mod tests;
