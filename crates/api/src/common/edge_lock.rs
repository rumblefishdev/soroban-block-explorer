//! Origin lock — reject requests that did not arrive through the Cloudflare
//! edge (task 0277 / ADR 0048, secret-header variant).
//!
//! Cloudflare injects a shared secret header (`X-Edge-Secret`) on every request
//! it forwards to the origin — a Transform Rule owned by the `rumblefishdev.com`
//! zone, scoped to the API host. Requests that reach the origin WITHOUT passing
//! through Cloudflare (the raw `execute-api` URL, or the REGIONAL custom domain
//! hit directly) carry no such header and are rejected with `403`.
//!
//! This is the foundation the paid-API tiering builds on (see
//! `docs/paid-api/plan-platne-api.md`): once every request is guaranteed to
//! arrive through the edge, the edge becomes the single chokepoint where free
//! (Turnstile) vs paid (API key) access can later be distinguished.
//!
//! GATED: enforced only when `EDGE_SECRET` is set (`AppConfig.edge_secret` is
//! `Some`). Unset = no-op, so the lock deploys "dark" and is armed only AFTER
//! the Cloudflare Transform Rule injects the matching value. `/health` is always
//! exempt so direct liveness probes (Lambda / monitoring, which do not go
//! through Cloudflare) keep working.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Header Cloudflare injects toward the origin. Lowercase: `http::HeaderMap`
/// lookups are case-insensitive, and lowercase avoids a runtime normalisation.
const EDGE_SECRET_HEADER: &str = "x-edge-secret";

/// Liveness path, always allowed through (probes hit the origin directly).
const HEALTH_PATH: &str = "/health";

/// Best-effort constant-time content comparison: equal-length inputs are
/// XOR-accumulated with NO early exit on the first differing byte, so a wrong
/// secret cannot be recovered byte-by-byte from response timing. The length
/// check is an early return — it leaks only the secret's length, never its
/// bytes, which is fine for the fixed-length, high-entropy CDK-generated secret.
fn secret_matches(provided: &[u8], expected: &[u8]) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in provided.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Middleware (wired via `from_fn_with_state` in `main::app`): allow `/health`
/// unconditionally, otherwise require a matching `X-Edge-Secret`. Absent or
/// wrong header → `403 Forbidden`.
pub async fn require_edge_secret(
    State(expected): State<Arc<String>>,
    req: Request,
    next: Next,
) -> Response {
    // `/health` exempt so direct liveness probes (Lambda / monitoring, not via
    // Cloudflare) keep working. Exact match relies on the Lambda seeing the
    // unprefixed path — `compute-stack` sets AWS_LAMBDA_HTTP_IGNORE_STAGE_IN_PATH=true,
    // stripping the `/{stage}` prefix. If that ever changes the path becomes
    // `/{stage}/health` and this check must follow (fail-closed: probes 403, no
    // security hole).
    if req.uri().path() == HEALTH_PATH {
        return next.run(req).await;
    }

    let ok = req
        .headers()
        .get(EDGE_SECRET_HEADER)
        .map(|v| secret_matches(v.as_bytes(), expected.as_bytes()))
        .unwrap_or(false);

    if ok {
        next.run(req).await
    } else {
        // 403 carries no-store for parity with the app-wide no-store-on-errors
        // policy (this layer sits OUTSIDE that middleware).
        let mut resp = (StatusCode::FORBIDDEN, "forbidden").into_response();
        resp.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::get;
    use tower::ServiceExt;

    fn locked_app(secret: &str) -> Router {
        Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/v1/data", get(|| async { "data" }))
            .layer(axum::middleware::from_fn_with_state(
                Arc::new(secret.to_string()),
                require_edge_secret,
            ))
    }

    async fn status(app: Router, uri: &str, header: Option<&str>) -> StatusCode {
        let mut b = HttpRequest::builder().uri(uri);
        if let Some(h) = header {
            b = b.header(EDGE_SECRET_HEADER, h);
        }
        app.oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn rejects_without_header() {
        assert_eq!(
            status(locked_app("s3cr3t"), "/v1/data", None).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn rejects_wrong_header() {
        assert_eq!(
            status(locked_app("s3cr3t"), "/v1/data", Some("nope")).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn allows_correct_header() {
        assert_eq!(
            status(locked_app("s3cr3t"), "/v1/data", Some("s3cr3t")).await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn health_exempt_without_header() {
        assert_eq!(
            status(locked_app("s3cr3t"), "/health", None).await,
            StatusCode::OK
        );
    }

    #[test]
    fn constant_time_compare() {
        assert!(secret_matches(b"abc", b"abc"));
        assert!(!secret_matches(b"abc", b"abd"));
        assert!(!secret_matches(b"abc", b"abcd"));
        assert!(!secret_matches(b"", b"x"));
    }
}
