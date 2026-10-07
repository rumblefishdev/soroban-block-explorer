//! REST API Lambda handler for the Soroban block explorer.

mod accounts;
mod assets;
mod auth;
mod cache;
mod common;
mod config;
mod contracts;
mod ledgers;
mod liquidity_pools;
mod network;
mod nfts;
mod openapi;
mod ops;
mod search;
pub mod state;
mod transactions;
// Runtime details enrichment — S3 archive reread + HTTP stellar.toml fetch.
// stellar_archive submodule drives E3 (`/transactions/:hash`) and E14
// (`/contracts/:id/events`); sep1 submodule drives E9 (`/assets/:id`).
// Exposed as module so future handlers can call the extractors without
// further wiring.
mod runtime_enrichment;

use axum::{
    Json, Router,
    routing::{get, post},
};
use utoipa::openapi::OpenApi as OpenApiSpec;

use crate::config::AppConfig;
use crate::runtime_enrichment::RuntimeEnrichment;
use crate::runtime_enrichment::sep1::Sep1Fetcher;
use crate::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
use crate::state::AppState;

/// Build the application router from an explicit [`AppConfig`] and [`AppState`].
///
/// Kept pure (no `std::env` reads) so tests can construct their own
/// config and state without mutating process state.
fn app(config: &AppConfig, state: AppState) -> Router {
    // Shared `openapi::register_routes` builds the same chain that the
    // `extract_openapi` build-time binary uses, so the codegen spec and
    // the live router cannot advertise different endpoints — with one
    // exception: `/auth/session` is in the spec through `ApiDoc` `paths(...)`
    // and mounted below only when the auth layer is armed (task 0510). We
    // then stamp the runtime `servers` block (resolved from
    // AppConfig.base_url) onto the registered spec.
    let (router, mut spec) = openapi::register_routes()
        .with_state(state)
        .split_for_parts();
    spec.servers = Some(vec![utoipa::openapi::server::Server::new(&config.base_url)]);

    // Share the spec behind an Arc so `/api-docs-json` only clones
    // a reference count per request instead of the full document.
    let spec_arc = std::sync::Arc::new(spec);
    let spec_for_json = spec_arc.clone();
    let router = router.route(
        "/api-docs-json",
        get(move || {
            let spec = spec_for_json.clone();
            async move { Json(spec) }
        }),
    );

    let router = mount_swagger_ui(router, spec_arc.as_ref());

    // Load-test correlation (task 0338): capture `X-Request-Id` into a
    // task-local so CH queries stamp `system.query_log.log_comment` with it
    // (B2). Added ONLY when armed (`load_testing`) — double-gated with the
    // header-presence check inside the middleware, so normal production never
    // sets a `log_comment`. Innermost of the security layers (it only needs to
    // wrap the handlers, where CH queries run).
    let router = if config.load_testing {
        router.layer(axum::middleware::from_fn(common::request_id::capture))
    } else {
        router
    };

    // ── Access layer (task 0277 paid-API; docs/paid-api/plan-platne-api.md):
    // free tier via Turnstile → session JWT, paid tier via X-API-Key. Built only
    // when ARMED (jwt_secret set) so it deploys "dark"; sits INSIDE the edge-secret
    // lock (which runs first), so only Cloudflare traffic reaches the auth gate.
    let auth_config = config
        .jwt_secret
        .as_ref()
        .map(|jwt_secret| auth::AuthConfig {
            jwt_secret: std::sync::Arc::new(jwt_secret.clone()),
            turnstile_secret: config
                .turnstile_secret
                .as_ref()
                .map(|s| std::sync::Arc::new(s.clone())),
            api_keys: std::sync::Arc::new(config.api_keys.clone()),
        });

    // `/auth/session` — verify a Turnstile token, mint a free-tier session JWT.
    // Exempt from the gate (it is called precisely to OBTAIN a session).
    let router = match auth_config.clone() {
        Some(a) => router.route(
            "/auth/session",
            post(move |body: Json<auth::SessionRequest>| {
                let a = a.clone();
                async move { auth::session(a, body).await }
            }),
        ),
        None => router,
    };

    // Auth gate (inner of the edge lock): require a valid paid key OR free session.
    let router = match auth_config {
        Some(a) => router.layer(axum::middleware::from_fn_with_state(a, auth::require_auth)),
        None => router,
    };

    // Origin lock (task 0277 Step 2, ADR 0048): reject any request that did NOT
    // arrive through the Cloudflare edge — i.e. that carries no matching
    // `X-Edge-Secret` (injected by Cloudflare toward the origin). Wraps the auth
    // gate (runs before it). No-op when `EDGE_SECRET` is unset. See
    // `common::edge_lock`.
    let router = match &config.edge_secret {
        Some(secret) => router.layer(axum::middleware::from_fn_with_state(
            std::sync::Arc::new(secret.clone()),
            common::edge_lock::require_edge_secret,
        )),
        None => router,
    };

    // CORS (OUTERMOST): the cross-origin SPA reads the actual GET/POST responses
    // from this Lambda. API Gateway's `defaultCorsPreflightOptions` answers only
    // the OPTIONS preflight (MOCK integration); the real responses are produced
    // here and must carry `Access-Control-Allow-Origin` themselves or the browser
    // blocks the read. Outermost so even 401/403 responses get the header.
    // `None` (CORS_ALLOW_ORIGIN unset) = no CORS layer (same-origin/non-browser).
    match config
        .cors_allow_origin
        .as_deref()
        .and_then(|o| axum::http::HeaderValue::from_str(o).ok())
    {
        Some(origin) => router.layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers([
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::ACCEPT,
                    axum::http::HeaderName::from_static("x-api-key"),
                ]),
        ),
        None => router,
    }
}

#[cfg(feature = "swagger-ui")]
fn mount_swagger_ui(router: Router, spec: &OpenApiSpec) -> Router {
    use utoipa_swagger_ui::SwaggerUi;
    // `SwaggerUi::url` mounts its own handler for the spec JSON under
    // the passed path, so we give it a dedicated internal path to
    // avoid colliding with the always-on `/api-docs-json` public
    // endpoint registered above.
    router.merge(SwaggerUi::new("/api-docs").url("/api-docs/openapi.json", spec.clone()))
}

#[cfg(not(feature = "swagger-ui"))]
fn mount_swagger_ui(router: Router, _spec: &OpenApiSpec) -> Router {
    router
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

    let config = AppConfig::from_env();
    tracing::info!("api cold start");

    // Overlap the two independent cold-start network fetches: the mTLS bundle
    // fetch from the Secrets Lambda Extension and the AWS SDK config load. Both
    // are tens to hundreds of milliseconds; running them sequentially would
    // double the cold-start budget.
    let database = db_clickhouse::database_from_env();
    let ch_fut = db_clickhouse::mtls::client_from_lambda_env(&database);
    let aws_config_fut = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .no_credentials()
        .region(aws_sdk_s3::config::Region::new(
            xdr_parser::public_archive::PUBLIC_BUCKET_REGION,
        ))
        .timeout_config(runtime_enrichment::stellar_archive::default_timeout_config())
        .load();
    let (ch, aws_config) = tokio::join!(ch_fut, aws_config_fut);
    let ch = ch.expect("failed to build mTLS ClickHouse client");

    let s3_client = aws_sdk_s3::Client::new(&aws_config);
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: StellarArchiveFetcher::new(s3_client),
        sep1: Sep1Fetcher::new().expect("failed to build SEP-1 stellar.toml HTTP client"),
        nft_token_uri: runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
            .expect("failed to build NFT token_uri HTTP client"),
        wasm_code: runtime_enrichment::wasm_code::WasmCodeFetcher::new()
            .expect("failed to build wasm-code RPC client"),
        account_sponsors: runtime_enrichment::account_sponsors::AccountSponsorsFetcher::new()
            .expect("account sponsors fetcher"),
    };

    let raw_passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE").unwrap_or_else(|_| {
        panic!(
            "STELLAR_NETWORK_PASSPHRASE env not set; required to align tx_set \
             envelopes with apply-order tx_processing when re-extracting \
             heavy fields. Expected the full Stellar passphrase string \
             (e.g. \"Public Global Stellar Network ; September 2015\")."
        )
    });
    // Trimmed once, as the indexer does: the archive guard and the network id
    // must see the same passphrase.
    let passphrase = raw_passphrase.trim();
    // Heavy fields come from the public data lake; a folder of the other
    // network would decode fine and match nothing (lore-0553).
    xdr_parser::public_archive::check_archive_network(
        &xdr_parser::public_archive::public_archive_prefix(),
        passphrase,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let network_id = xdr_parser::network_id(passphrase);
    let state = AppState::new(ch, runtime_enrichment, network_id);
    let app = app(&config, state);

    lambda_http::run(app).await.expect("failed to run Lambda");
}

#[cfg(test)]
#[path = "tests/main_tests.rs"]
mod tests;
