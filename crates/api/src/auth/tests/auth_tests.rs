use super::*;
use axum::Router;
use axum::body::Body;
use axum::http::Request as HttpRequest;
use axum::routing::get;
use tower::ServiceExt;

fn test_auth() -> AuthConfig {
    AuthConfig {
        jwt_secret: Arc::new("test-secret".to_string()),
        turnstile_secret: None,
        api_keys: Arc::new(vec!["valid-key".to_string()]),
    }
}

fn gated_app() -> Router {
    Router::new()
        .route("/v1/data", get(|| async { "data" }))
        .route("/health", get(|| async { "ok" }))
        .route("/api-docs", get(|| async { "docs" }))
        .layer(axum::middleware::from_fn_with_state(
            test_auth(),
            require_auth,
        ))
}

async fn status(uri: &str, header: Option<(&str, &str)>) -> StatusCode {
    let mut b = HttpRequest::builder().uri(uri);
    if let Some((k, v)) = header {
        b = b.header(k, v);
    }
    gated_app()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[test]
fn exempt_paths() {
    assert!(is_exempt("/health"));
    assert!(is_exempt("/auth/session"));
    assert!(is_exempt("/api-docs"));
    assert!(is_exempt("/api-docs/openapi.json"));
    assert!(is_exempt("/api-docs-json"));
    assert!(!is_exempt("/v1/ledgers"));
}

#[tokio::test]
async fn exempt_routes_pass_without_auth() {
    assert_eq!(status("/health", None).await, StatusCode::OK);
    assert_eq!(status("/api-docs", None).await, StatusCode::OK);
}

#[tokio::test]
async fn data_without_auth_is_401() {
    assert_eq!(status("/v1/data", None).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn valid_api_key_passes() {
    assert_eq!(
        status("/v1/data", Some(("x-api-key", "valid-key"))).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn wrong_api_key_is_401() {
    assert_eq!(
        status("/v1/data", Some(("x-api-key", "nope"))).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn valid_bearer_jwt_passes() {
    let token = jwt::issue("test-secret", now_secs()).unwrap();
    assert_eq!(
        status(
            "/v1/data",
            Some(("authorization", &format!("Bearer {token}")))
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn lowercase_bearer_scheme_passes() {
    let token = jwt::issue("test-secret", now_secs()).unwrap();
    assert_eq!(
        status(
            "/v1/data",
            Some(("authorization", &format!("bearer {token}")))
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn invalid_bearer_is_401() {
    assert_eq!(
        status("/v1/data", Some(("authorization", "Bearer not.a.jwt"))).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn unauthorized_carries_no_store() {
    let resp = gated_app()
        .oneshot(
            HttpRequest::builder()
                .uri("/v1/data")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
}

#[tokio::test]
async fn session_without_turnstile_secret_is_503() {
    // `turnstile_secret: None` → the handler rejects before any siteverify
    // network call, so this is hermetic.
    let resp = session(
        test_auth(),
        Json(SessionRequest {
            token: "x".to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
