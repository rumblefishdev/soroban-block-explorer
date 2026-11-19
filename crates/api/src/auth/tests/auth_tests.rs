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

// Decision 2 (task 0677): the free-tier session travels in an HttpOnly cookie.
// Cloudflare sets cookies of its own on the API host, so ours sits among them.
#[tokio::test]
async fn session_cookie_among_other_cookies_passes() {
    let token = jwt::issue("test-secret", now_secs()).unwrap();
    assert_eq!(
        status(
            "/v1/data",
            Some(("cookie", &format!("__cf_bm=abc; __Host-session={token}")))
        )
        .await,
        StatusCode::OK
    );
}

// HTTP/2 may split cookies over several `Cookie` headers.
#[tokio::test]
async fn session_cookie_in_a_second_cookie_header_passes() {
    let token = jwt::issue("test-secret", now_secs()).unwrap();
    let resp = gated_app()
        .oneshot(
            HttpRequest::builder()
                .uri("/v1/data")
                .header("cookie", "a=1")
                .header("cookie", format!("__Host-session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn invalid_session_cookie_is_401() {
    assert_eq!(
        status("/v1/data", Some(("cookie", "__Host-session=not.a.jwt"))).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn cookie_with_a_longer_name_is_not_the_session() {
    let token = jwt::issue("test-secret", now_secs()).unwrap();
    assert_eq!(
        status(
            "/v1/data",
            Some(("cookie", &format!("__Host-session2={token}")))
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
}

// Decision 6: one cut-over, the gate no longer reads `Authorization: Bearer`.
#[tokio::test]
async fn bearer_jwt_is_401() {
    let token = jwt::issue("test-secret", now_secs()).unwrap();
    assert_eq!(
        status(
            "/v1/data",
            Some(("authorization", &format!("Bearer {token}")))
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
}

// Decision 9: the attributes the browser needs to keep the cookie host-only,
// out of page scripts, and away from cross-site requests.
#[test]
fn session_cookie_has_the_decided_attributes() {
    assert_eq!(
        session_cookie("TOK"),
        "__Host-session=TOK; Max-Age=900; Path=/; Secure; HttpOnly; SameSite=Strict"
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
        axum::http::HeaderMap::new(),
        Json(SessionRequest {
            token: "x".to_string(),
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}
