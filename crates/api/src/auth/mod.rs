//! Paid-API access layer (task 0277, see `docs/paid-api/plan-platne-api.md`):
//!  - **free tier** — SPA solves Turnstile → `/auth/session` mints a short session
//!    JWT → the gate accepts `Authorization: Bearer <jwt>`;
//!  - **paid tier** — `X-API-Key` in the configured allowlist (skips Turnstile);
//!  - anything else on a data route → `401`.
//!
//! GATED: wired in `main::app` only when `AppConfig.jwt_secret` is set; unset =
//! the gate and `/auth/session` are absent (no-op), so it deploys "dark". Sits
//! INSIDE the edge-secret lock (which runs first), so only Cloudflare traffic
//! ever reaches the auth gate.

pub mod jwt;
pub mod turnstile;

use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

/// Everything the access layer needs at runtime. Built only when armed.
#[derive(Clone)]
pub struct AuthConfig {
    pub jwt_secret: Arc<String>,
    pub turnstile_secret: Option<Arc<String>>,
    pub api_keys: Arc<Vec<String>>,
}

/// Paths always allowed through the gate: liveness; the session-minting route
/// itself (called precisely to OBTAIN a session); and the OpenAPI docs/spec,
/// which are public for a public block explorer (Swagger UI + assets live under
/// `/api-docs`, and the always-on spec is `/api-docs-json` — both covered by the
/// `/api-docs` prefix). The gated surface is the `/v1/*` data API.
fn is_exempt(path: &str) -> bool {
    path == "/health" || path == "/auth/session" || path.starts_with("/api-docs")
}

/// Process-wide reqwest client (connection pool), reused across invocations.
/// Carries an explicit timeout so a stalled Cloudflare siteverify cannot pin a
/// Lambda invocation open (the `/auth/session` mint awaits this call); without
/// it `reqwest` would wait indefinitely. Falls back to a default client only if
/// the builder somehow fails.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Constant-time byte compare (no early exit on first diff). Length may leak —
/// API keys are fixed-length, high-entropy.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Tag a response `Cache-Control: no-store`. Auth/token responses (the session
/// JWT, and the 401) must never be cached by intermediaries or clients.
fn no_store(mut resp: Response) -> Response {
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

// ── POST /auth/session ─────────────────────────────────────────────────
#[derive(Deserialize)]
pub struct SessionRequest {
    /// Turnstile token produced by the SPA widget.
    pub token: String,
}

#[derive(Serialize)]
struct SessionResponse {
    token: String,
    expires_in: u64,
}

/// Verify a Turnstile token with Cloudflare, then mint a free-tier session JWT.
pub async fn session(auth: AuthConfig, Json(req): Json<SessionRequest>) -> Response {
    let Some(ts_secret) = auth.turnstile_secret.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "turnstile not configured").into_response();
    };
    if !turnstile::verify(http_client(), ts_secret, &req.token).await {
        return (StatusCode::FORBIDDEN, "turnstile verification failed").into_response();
    }
    match jwt::issue(&auth.jwt_secret, now_secs()) {
        Ok(token) => no_store(
            Json(SessionResponse {
                token,
                expires_in: jwt::SESSION_TTL_SECS,
            })
            .into_response(),
        ),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "token issue failed").into_response(),
    }
}

// ── auth gate middleware ───────────────────────────────────────────────
/// Accept a request iff it is exempt, OR carries a valid paid `X-API-Key`, OR a
/// valid free session JWT (`Authorization: Bearer …`). Else `401`.
pub async fn require_auth(State(auth): State<AuthConfig>, req: Request, next: Next) -> Response {
    if is_exempt(req.uri().path()) {
        return next.run(req).await;
    }

    // Paid tier — X-API-Key in the allowlist (computed before any move of `req`).
    let paid = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|key| {
            auth.api_keys
                .iter()
                .any(|valid| ct_eq(valid.as_bytes(), key.as_bytes()))
        })
        .unwrap_or(false);
    if paid {
        return next.run(req).await;
    }

    // Free tier — valid session JWT in `Authorization: Bearer …`. The scheme is
    // case-insensitive (RFC 7235), so accept any casing of "bearer" + trim.
    let free = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, tok)| jwt::verify(&auth.jwt_secret, tok.trim()))
        .unwrap_or(false);
    if free {
        return next.run(req).await;
    }

    no_store((StatusCode::UNAUTHORIZED, "authentication required").into_response())
}

#[cfg(test)]
mod tests {
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
}
