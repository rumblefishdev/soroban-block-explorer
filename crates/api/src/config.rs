//! Runtime configuration for the API service.
//!
//! All environment variable reads happen in [`AppConfig::from_env`] so
//! that `fn app(&AppConfig) -> Router` stays pure — tests construct
//! their own `AppConfig` without touching `std::env`.

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
    /// Load-test correlation switch (task 0338). `true` (env `LOAD_TESTING=true`,
    /// set by `compute-stack.ts` only when `config.loadTesting` — the SAME flag
    /// that lifts the API Gateway throttle/WAF) arms the
    /// [`crate::common::request_id`] middleware so CH queries stamp
    /// `system.query_log.log_comment` with the inbound `X-Request-Id` (B2).
    /// `false` (default) leaves the mechanism fully inert.
    pub load_testing: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            base_url: std::env::var("API_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
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
            // Exact `"true"` only — any other value (unset, "false", "1") leaves
            // the load-test correlation off. Matches the `'true'` literal set by
            // `compute-stack.ts`.
            load_testing: std::env::var("LOAD_TESTING").as_deref() == Ok("true"),
        }
    }
}
