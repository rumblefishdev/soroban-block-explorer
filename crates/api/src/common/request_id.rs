//! Per-request correlation id for load testing (task 0338).
//!
//! When armed (`AppConfig::load_testing`, wired in `main::app`), this
//! middleware captures an inbound `X-Request-Id` header into a tokio
//! task-local. [`crate::state::AppState::ch`] reads it and stamps every
//! ClickHouse query with `log_comment = <id>`, so a load-test run can JOIN
//! its client-side per-request CSV against `system.query_log.log_comment` to
//! attribute `read_rows`/`read_bytes` to the exact HTTP request that caused
//! them (the "B2" correlation in task 0338).
//!
//! Double-gated, so it is fully inert in normal production:
//!   1. the layer is added ONLY when `load_testing` is set (a load-test
//!      deploy — `LOAD_TESTING=true`, set by `compute-stack.ts` off the same
//!      `loadTesting` flag that lifts the API Gateway throttle/WAF), and
//!   2. the task-local is set ONLY when the request actually carries an
//!      `X-Request-Id`.
//!
//! The value is sanitised (ASCII alphanumeric + `-`, capped at [`MAX_LEN`])
//! before it can reach `log_comment`, so a caller cannot inject arbitrary
//! content into `system.query_log` even when the mechanism is armed.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Inbound header carrying the per-request correlation id. Lowercase:
/// `http::HeaderMap` lookups are case-insensitive and lowercase avoids a
/// runtime normalisation (mirrors `common::edge_lock`).
const REQUEST_ID_HEADER: &str = "x-request-id";

/// Max accepted id length. A uuid is 36 chars; 64 leaves slack for a
/// prefixed scheme while bounding what can land in `log_comment`.
const MAX_LEN: usize = 64;

tokio::task_local! {
    /// Sanitised `X-Request-Id` for the in-flight request, if one was set.
    static REQUEST_ID: String;
}

/// Keep only ASCII alphanumerics and `-` (covers uuids and hyphenated
/// schemes), capped at [`MAX_LEN`]. Drops everything else so nothing a
/// caller controls beyond `[A-Za-z0-9-]` reaches `system.query_log`.
fn sanitize(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(MAX_LEN)
        .collect()
}

/// Middleware (wired via `from_fn` in `main::app`, ONLY when `load_testing`):
/// run the request inside a task-local scope holding the sanitised
/// `X-Request-Id`, so `AppState::ch` can stamp CH queries with `log_comment`.
/// An absent / empty-after-sanitise header means no scope and no
/// `log_comment` — the request runs exactly as it would in normal production.
pub async fn capture(req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(sanitize)
        .filter(|s| !s.is_empty());

    match id {
        Some(id) => REQUEST_ID.scope(id, next.run(req)).await,
        None => next.run(req).await,
    }
}

/// The current request's sanitised id, if one was captured. `None` outside a
/// [`capture`] scope — i.e. normal production (the layer is unwired) or a
/// non-request context such as cold start — so callers then issue queries
/// with no `log_comment`.
pub fn current() -> Option<String> {
    REQUEST_ID.try_with(|id| id.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_uuid_and_drops_injection() {
        assert_eq!(
            sanitize("0191e2a4-7b3c-7c01-9f2e-2b9f1a2c3d4e"),
            "0191e2a4-7b3c-7c01-9f2e-2b9f1a2c3d4e"
        );
        // Spaces, quotes, and SQL-ish punctuation are stripped, not escaped.
        assert_eq!(sanitize("abc'; DROP TABLE x --"), "abcDROPTABLEx--");
    }

    #[test]
    fn sanitize_caps_length() {
        let long = "a".repeat(200);
        assert_eq!(sanitize(&long).len(), MAX_LEN);
    }

    #[test]
    fn current_is_none_outside_scope() {
        assert_eq!(current(), None);
    }

    #[tokio::test]
    async fn current_reads_value_inside_scope() {
        REQUEST_ID
            .scope("req-123".to_string(), async {
                assert_eq!(current(), Some("req-123".to_string()));
            })
            .await;
    }
}
