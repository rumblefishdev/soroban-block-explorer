//! Centralised `Cache-Control` policy per task 0055.
//!
//! Five tiers map to the per-endpoint TTL strategy that the API Gateway
//! stage cache will consume (CDK config landed by task 0097):
//!
//! - `LIVE` — max-age=0 for the live-polled feeds (home ledgers /
//!   transactions lists, network stats). The frontend polls these on the
//!   ledger cadence (~5.8s) and anchors its adaptive schedule on the
//!   newest row's close time; any browser-cache TTL ≥ the cadence makes
//!   polls re-read a stale payload and the feed advance 2-3 ledgers per
//!   visible update. `max-age=0, must-revalidate` forces every poll to
//!   consult the origin.
//! - `SHORT` — 10s for lists / frequently-mutating detail (matches API
//!   Gateway `apiGatewayCacheTtlMutable`).
//! - `MEDIUM` — 60s for slowly-changing metadata (asset/contract/nft
//!   detail, LP chart).
//! - `LONG` — 300s for immutable resources (closed ledgers, finalized
//!   transactions with full archive overlay).
//! - `NO_STORE` — search + every error response.
//!
//! Errors get `no-store` via the [`enforce_no_store_on_errors`] tower
//! middleware in [`crate::openapi::register_routes`], so handlers don't
//! need to special-case error paths.

use axum::extract::Request;
use axum::http::{HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;

pub const LIVE: HeaderValue = HeaderValue::from_static("public, max-age=0, must-revalidate");
pub const SHORT: HeaderValue = HeaderValue::from_static("public, max-age=10");
pub const MEDIUM: HeaderValue = HeaderValue::from_static("public, max-age=60");
pub const LONG: HeaderValue = HeaderValue::from_static("public, max-age=300");
pub const NO_STORE: HeaderValue = HeaderValue::from_static("no-store");

/// Set `Cache-Control` on a built response. Replaces any existing value.
pub fn attach(resp: &mut Response, value: HeaderValue) {
    resp.headers_mut().insert(header::CACHE_CONTROL, value);
}

/// Tower middleware: forces `Cache-Control: no-store` on every non-2xx
/// response. Catches handler-side errors (`errors::*`), extractor
/// rejections, and axum's bare 404-on-unmatched-route — none of which
/// should ever be cached.
///
/// `304 Not Modified` is exempt: it is a successful conditional-GET response
/// (task 0292), not an error, and carries its own `Cache-Control` (the `LIVE`
/// tier of the matching `200`). Stamping `no-store` on it would contradict the
/// conditional-GET contract — the client must keep its cached body and keep
/// revalidating. See [`crate::common::conditional`].
pub async fn enforce_no_store_on_errors(req: Request, next: Next) -> Response {
    use axum::http::StatusCode;
    let mut resp = next.run(req).await;
    if !resp.status().is_success() && resp.status() != StatusCode::NOT_MODIFIED {
        resp.headers_mut().insert(header::CACHE_CONTROL, NO_STORE);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use tower::ServiceExt;

    fn cc(resp: &Response) -> Option<String> {
        resp.headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    }

    #[tokio::test]
    async fn middleware_forces_no_store_on_500() {
        let app = Router::new()
            .route(
                "/boom",
                get(|| async {
                    let mut r = (StatusCode::INTERNAL_SERVER_ERROR, "x").into_response();
                    attach(&mut r, LONG);
                    r
                }),
            )
            .layer(axum::middleware::from_fn(enforce_no_store_on_errors));

        let resp = app
            .oneshot(Request::builder().uri("/boom").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(cc(&resp).as_deref(), Some("no-store"));
    }

    #[tokio::test]
    async fn middleware_passes_through_200() {
        let app = Router::new()
            .route(
                "/ok",
                get(|| async {
                    let mut r = "ok".into_response();
                    attach(&mut r, SHORT);
                    r
                }),
            )
            .layer(axum::middleware::from_fn(enforce_no_store_on_errors));

        let resp = app
            .oneshot(Request::builder().uri("/ok").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(cc(&resp).as_deref(), Some("public, max-age=10"));
    }

    #[tokio::test]
    async fn middleware_exempts_304_from_no_store() {
        // A conditional-GET 304 carries its own LIVE cache-control; the
        // no-store sweep must leave it intact (task 0292).
        let app = Router::new()
            .route(
                "/cond",
                get(|| async {
                    let mut r = StatusCode::NOT_MODIFIED.into_response();
                    attach(&mut r, LIVE);
                    r
                }),
            )
            .layer(axum::middleware::from_fn(enforce_no_store_on_errors));

        let resp = app
            .oneshot(Request::builder().uri("/cond").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            cc(&resp).as_deref(),
            Some("public, max-age=0, must-revalidate")
        );
    }

    #[tokio::test]
    async fn middleware_sets_no_store_on_unmatched_route() {
        let app = Router::new()
            .route("/found", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(enforce_no_store_on_errors));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(cc(&resp).as_deref(), Some("no-store"));
    }
}
