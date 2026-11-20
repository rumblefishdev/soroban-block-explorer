//! Paid-API access layer (task 0277):
//!  - **free tier** — SPA solves Turnstile → `/auth/session` sets a short session
//!    JWT in the HttpOnly cookie `__Host-session` → the gate accepts that cookie,
//!    so one check serves every tab and reload of the browser (task 0677);
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
use serde::Deserialize;
use utoipa::ToSchema;

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
#[derive(Deserialize, ToSchema)]
pub struct SessionRequest {
    /// Turnstile token produced by the SPA widget.
    pub token: String,
}

/// Name of the session cookie. `__Host-` makes the browser keep it on the API
/// host only (no `Domain`), so mainnet and testnet sessions never mix.
const SESSION_COOKIE: &str = "__Host-session";

/// `Set-Cookie` value for a minted session: `HttpOnly` keeps it out of page
/// scripts, `SameSite=Strict` off cross-site requests, and it lives as long
/// as the JWT inside it.
fn session_cookie(token: &str) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Max-Age={}; Path=/; Secure; HttpOnly; SameSite=Strict",
        jwt::SESSION_TTL_SECS
    )
}

/// Every value of a session cookie the request carries. Cloudflare sets
/// cookies of its own on this host and HTTP/2 may split them over several
/// `Cookie` headers. Split as bytes, and keep every match: a cookie with bytes
/// outside ASCII, or a junk one of the same name set by a sibling host, must
/// not hide the valid session.
fn session_tokens(headers: &axum::http::HeaderMap) -> Vec<&[u8]> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .flat_map(|v| v.as_bytes().split(|b| *b == b';'))
        .filter_map(|c| c.trim_ascii_start().strip_prefix(SESSION_COOKIE.as_bytes()))
        .filter_map(|rest| rest.strip_prefix(b"="))
        .collect()
}

// In the spec through `ApiDoc`'s `paths(...)`, not `register_routes`: the
// route is mounted by `main::app` only when the layer is armed, and the spec
// must describe it either way (task 0510).
/// Verify a Turnstile token with Cloudflare, then mint a free-tier session JWT.
#[utoipa::path(
    post,
    path = "/auth/session",
    tag = "auth",
    // Pins the generated SDK name (`mintSession`); utoipa would name it `session`.
    operation_id = "mint_session",
    // Exempt from the gate (see `is_exempt`) — called to OBTAIN a session.
    security(()),
    request_body = SessionRequest,
    responses(
        (status = 204, description = "Session minted",
            headers(("Set-Cookie" = String, description = "`__Host-session`, HttpOnly, 15 min"))),
        (status = 403, description = "Turnstile verification failed",
            body = String, content_type = "text/plain"),
        (status = 500, description = "Token issue failed",
            body = String, content_type = "text/plain"),
        (status = 503, description = "Turnstile not configured",
            body = String, content_type = "text/plain"),
    ),
)]
pub async fn session(
    auth: AuthConfig,
    headers: axum::http::HeaderMap,
    Json(req): Json<SessionRequest>,
) -> Response {
    let Some(ts_secret) = auth.turnstile_secret.as_ref() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "turnstile not configured").into_response();
    };
    if !turnstile::verify(http_client(), ts_secret, &req.token).await {
        return (StatusCode::FORBIDDEN, "turnstile verification failed").into_response();
    }
    let Ok(token) = jwt::issue(&auth.jwt_secret, now_secs()) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "token issue failed").into_response();
    };
    // The browser drops the cookie when the JWT inside it expires, so a mint
    // normally comes with no cookie (`false`): a new visitor, an expired
    // session, or a browser that keeps no cookies. `true` means another tab
    // minted first, or the JWT was refused (e.g. a rotated secret).
    tracing::info!(
        cookie_present = !session_tokens(&headers).is_empty(),
        "session minted"
    );
    no_store(
        (
            [(header::SET_COOKIE, session_cookie(&token))],
            StatusCode::NO_CONTENT,
        )
            .into_response(),
    )
}

// ── auth gate middleware ───────────────────────────────────────────────
/// Accept a request iff it is exempt, OR carries a valid paid `X-API-Key`, OR a
/// valid free session JWT in the `__Host-session` cookie. Else `401`.
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

    // Free tier — valid session JWT in a session cookie.
    let tokens = session_tokens(req.headers());
    for token in &tokens {
        if let Ok(token) = std::str::from_utf8(token)
            && jwt::verify(&auth.jwt_secret, token)
        {
            return next.run(req).await;
        }
    }

    // `cookie_present=false` is every first read of a session (new visitor,
    // expired session) and every read of a browser that keeps no cookies;
    // `true` is a cookie the gate refused.
    tracing::info!(
        cookie_present = !tokens.is_empty(),
        path = req.uri().path(),
        "session rejected"
    );
    no_store((StatusCode::UNAUTHORIZED, "authentication required").into_response())
}

#[cfg(test)]
#[path = "tests/auth_tests.rs"]
mod tests;
