//! Centralised `Cache-Control` policy per task 0055.
//!
//! Four tiers map to the per-endpoint TTL strategy that the API Gateway
//! stage cache will consume (CDK config landed by task 0097):
//!
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
pub async fn enforce_no_store_on_errors(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    if !resp.status().is_success() {
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
