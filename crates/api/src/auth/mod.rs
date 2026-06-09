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
        Ok(token) => Json(SessionResponse {
            token,
            expires_in: jwt::SESSION_TTL_SECS,
        })
        .into_response(),
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

    // Free tier — valid session JWT in `Authorization: Bearer …`.
    let free = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|tok| jwt::verify(&auth.jwt_secret, tok))
        .unwrap_or(false);
    if free {
        return next.run(req).await;
    }

    let mut resp = (StatusCode::UNAUTHORIZED, "authentication required").into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static("no-store"));
    resp
}
