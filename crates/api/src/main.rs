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
    // the live router cannot advertise different endpoints. We then stamp
    // the runtime `servers` block (resolved from AppConfig.base_url) onto
    // the registered spec.
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
    let ch_fut = db_clickhouse::mtls::client_from_lambda_env(db_clickhouse::PROD_DATABASE);
    let aws_config_fut = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .no_credentials()
        .region(aws_sdk_s3::config::Region::new("us-east-2"))
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
    };

    let passphrase = std::env::var("STELLAR_NETWORK_PASSPHRASE").unwrap_or_else(|_| {
        panic!(
            "STELLAR_NETWORK_PASSPHRASE env not set; required to align tx_set \
             envelopes with apply-order tx_processing when re-extracting \
             heavy fields. Expected the full Stellar passphrase string \
             (e.g. \"Public Global Stellar Network ; September 2015\")."
        )
    });
    let network_id = xdr_parser::network_id(&passphrase);
    let state = AppState::new(ch, runtime_enrichment, network_id);
    let app = app(&config, state);

    lambda_http::run(app).await.expect("failed to run Lambda");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{self, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    fn test_config() -> AppConfig {
        AppConfig {
            base_url: "http://localhost:9000".to_string(),
            edge_secret: None,
            jwt_secret: None,
            turnstile_secret: None,
            api_keys: Vec::new(),
            cors_allow_origin: None,
            load_testing: false,
        }
    }

    /// Build a test app. The CH client is unconnected — these spec / health
    /// tests never issue a query.
    fn test_app() -> Router {
        let ch = clickhouse::Client::default();
        let runtime_enrichment = RuntimeEnrichment {
            stellar_archive: StellarArchiveFetcher::new(
                runtime_enrichment::stellar_archive::test_client(),
            ),
            // Real SEP-1 fetcher with a stub HTTP client. The spec / health
            // tests below never reach get_asset, so the client never makes a
            // real request.
            sep1: Sep1Fetcher::new().expect("build sep1 fetcher"),
            nft_token_uri: runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
                .expect("build nft_token_uri fetcher"),
            wasm_code: runtime_enrichment::wasm_code::WasmCodeFetcher::new()
                .expect("build wasm_code fetcher"),
        };
        app(&test_config(), AppState::for_tests(ch, runtime_enrichment))
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_docs_json_contains_health_path() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(spec["info"]["title"], "Soroban Block Explorer API");
        assert_eq!(spec["info"]["version"], env!("CARGO_PKG_VERSION"));
        assert!(
            spec["paths"]["/health"].is_object(),
            "spec missing /health path: {spec}"
        );
        assert_eq!(spec["servers"][0]["url"], "http://localhost:9000");
    }

    #[tokio::test]
    async fn api_docs_json_has_error_envelope_component() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            spec["components"]["schemas"]["ErrorEnvelope"].is_object(),
            "spec missing ErrorEnvelope component: {spec}"
        );
        assert!(
            spec["components"]["schemas"]["PageInfo"].is_object(),
            "spec missing PageInfo component: {spec}"
        );
    }

    #[tokio::test]
    async fn api_docs_json_contains_contracts_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        for path in [
            "/v1/contracts/{contract_id}",
            "/v1/contracts/{contract_id}/interface",
            "/v1/contracts/{contract_id}/decompiled",
            "/v1/contracts/{contract_id}/invocations",
            "/v1/contracts/{contract_id}/events",
        ] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing {path} path: {spec}"
            );
        }
    }

    #[tokio::test]
    async fn api_docs_json_contains_assets_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        for path in [
            "/v1/assets",
            "/v1/assets/{id}",
            "/v1/assets/{id}/transactions",
        ] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing {path} path: {spec}"
            );
        }
    }

    #[tokio::test]
    async fn api_docs_json_contains_ledgers_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        for path in ["/v1/ledgers", "/v1/ledgers/{sequence}"] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing {path} path: {spec}"
            );
        }
        assert!(
            spec["components"]["schemas"]["LedgerListItem"].is_object(),
            "spec missing LedgerListItem component: {spec}"
        );
        assert!(
            spec["components"]["schemas"]["LedgerDetailResponse"].is_object(),
            "spec missing LedgerDetailResponse component: {spec}"
        );
    }

    #[tokio::test]
    async fn api_docs_json_contains_transactions_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            spec["paths"]["/v1/transactions"].is_object(),
            "spec missing /v1/transactions path: {spec}"
        );
        assert!(
            spec["paths"]["/v1/transactions/{hash}"].is_object(),
            "spec missing /v1/transactions/{{hash}} path: {spec}"
        );
    }

    #[tokio::test]
    async fn api_docs_json_contains_nfts_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        // Per task 0264 Phase 8a, the NFT detail / transfers routes are
        // keyed by the `(contract_id, token_id)` composite rather than
        // by the internal `nfts.id i32` surrogate.
        for path in [
            "/v1/nfts",
            "/v1/nfts/{contract_id}/{token_id}",
            "/v1/nfts/{contract_id}/{token_id}/transfers",
        ] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing {path} path: {spec}"
            );
        }
    }

    #[tokio::test]
    async fn api_docs_json_contains_liquidity_pools_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        for path in [
            "/v1/liquidity-pools",
            "/v1/liquidity-pools/{pool_id}",
            "/v1/liquidity-pools/{pool_id}/transactions",
            "/v1/liquidity-pools/{pool_id}/chart",
            "/v1/liquidity-pools/{pool_id}/participants",
        ] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing {path} path: {spec}"
            );
        }
    }

    #[tokio::test]
    async fn api_docs_json_contains_network_stats_path() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            spec["paths"]["/v1/network/stats"].is_object(),
            "spec missing /v1/network/stats path: {spec}"
        );
        assert!(
            spec["components"]["schemas"]["NetworkStats"].is_object(),
            "spec missing NetworkStats component: {spec}"
        );
    }

    #[tokio::test]
    async fn api_docs_json_contains_accounts_paths() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        for path in [
            "/v1/accounts/{account_id}",
            "/v1/accounts/{account_id}/transactions",
        ] {
            assert!(
                spec["paths"][path].is_object(),
                "spec missing {path} path: {spec}"
            );
        }
        for component in [
            "AccountDetailResponse",
            "AccountBalance",
            "AccountTransactionItem",
        ] {
            assert!(
                spec["components"]["schemas"][component].is_object(),
                "spec missing {component} component: {spec}"
            );
        }
    }

    #[tokio::test]
    async fn api_docs_json_contains_search_path() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs-json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let spec: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            spec["paths"]["/v1/search"].is_object(),
            "spec missing /v1/search path: {spec}"
        );
        for component in ["SearchResults", "SearchGroups", "SearchHit", "EntityType"] {
            assert!(
                spec["components"]["schemas"][component].is_object(),
                "spec missing {component} component: {spec}"
            );
        }
    }

    #[cfg(feature = "swagger-ui")]
    #[tokio::test]
    async fn swagger_ui_mounted_when_feature_enabled() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            response.status().is_success() || response.status().is_redirection(),
            "expected 2xx/3xx for /api-docs/, got {}",
            response.status()
        );
    }

    #[cfg(not(feature = "swagger-ui"))]
    #[tokio::test]
    async fn swagger_ui_absent_without_feature() {
        let app = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api-docs/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
