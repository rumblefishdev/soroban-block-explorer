//! Runtime configuration for the API service.
//!
//! All environment variable reads happen in [`AppConfig::from_env`] so
//! that `fn app(&AppConfig) -> Router` stays pure — tests construct
//! their own `AppConfig` without touching `std::env`.

use crate::common::datasource::DataSource;

/// Application-wide runtime configuration.
///
/// The `version` advertised in the OpenAPI spec is sourced from
/// `env!("CARGO_PKG_VERSION")` directly at the `ApiDoc` derive site,
/// so it does not need to live on this struct.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Fully-qualified base URL advertised to OpenAPI clients in the
    /// `servers` block. In production this is the API Gateway custom
    /// domain (e.g. `https://api.staging.sorobanscan.rumblefish.dev`);
    /// locally it falls back to `http://localhost:9000`.
    pub base_url: String,
    /// True when at least one handler module is configured to read
    /// from ClickHouse via `API_DATASOURCE_<MODULE>=ch`. Drives the
    /// cold-start decision in `main.rs` to build (or skip) the mTLS
    /// CH client; PG-only deploys keep `false` and never touch the
    /// Secrets Lambda Extension.
    pub ch_enabled: bool,
    /// Shared secret that Cloudflare injects as the `X-Edge-Secret` header on
    /// every request it forwards to the origin (origin lock, task 0277 / ADR
    /// 0048). When `Some`, the [`crate::common::edge_lock`] middleware rejects
    /// any request (except `/health`) that lacks a matching header — i.e. any
    /// request that reached the origin WITHOUT passing through Cloudflare.
    ///
    /// `None` (env unset or empty) = no-op, so the lock deploys "dark" and is
    /// armed only AFTER the Cloudflare Transform Rule injects the matching
    /// value. The value never lives in git — it comes from `EDGE_SECRET`, set
    /// by the Lambda from Secrets Manager.
    pub edge_secret: Option<String>,
    /// HS256 signing key for free-tier **session JWTs** (paid-API access layer,
    /// task 0277). From `JWT_SECRET`. `None` = the auth gate and `/auth/session`
    /// are disabled (no-op), so the access layer deploys "dark". Its presence is
    /// what ARMS the auth gate (see [`crate::auth`]).
    pub jwt_secret: Option<String>,
    /// Cloudflare **Turnstile** secret key for `siteverify`. From
    /// `TURNSTILE_SECRET`. Required (with `jwt_secret`) for `/auth/session` to
    /// mint a session; `None` makes that endpoint reject.
    pub turnstile_secret: Option<String>,
    /// Valid **paid-tier API keys** (comma-separated in `API_KEYS`). A request
    /// whose `X-API-Key` matches one (constant-time) is the paid tier and skips
    /// the Turnstile/JWT free-tier check. Empty = no paid keys configured.
    pub api_keys: Vec<String>,
    /// Allowed CORS origin for the cross-origin SPA (from `CORS_ALLOW_ORIGIN`,
    /// e.g. `https://sorobanscan.rumblefish.dev`). API Gateway answers only the
    /// OPTIONS preflight; the actual GET/POST responses come from this Lambda
    /// and need `Access-Control-Allow-Origin` for the browser to read them.
    /// `None` (env unset/empty) = no CORS layer (same-origin / non-browser use).
    pub cors_allow_origin: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("API_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
            ch_enabled: DataSource::any_ch_enabled(),
            // Treat empty OR whitespace-only as unset (no-op) — never arm the
            // lock on a near-empty/low-entropy value. The real value is kept
            // verbatim (the CDK secret is alnum, so nothing to trim away).
            edge_secret: std::env::var("EDGE_SECRET")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            jwt_secret: std::env::var("JWT_SECRET")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            turnstile_secret: std::env::var("TURNSTILE_SECRET")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            api_keys: std::env::var("API_KEYS")
                .ok()
                .map(|s| {
                    s.split(',')
                        .map(|k| k.trim().to_string())
                        .filter(|k| !k.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            cors_allow_origin: std::env::var("CORS_ALLOW_ORIGIN")
                .ok()
                .filter(|s| !s.trim().is_empty()),
        }
    }
}
