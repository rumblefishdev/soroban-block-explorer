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
        x402: None,
    }
}

/// Build a test app. The CH client is unconnected — these spec / health
/// tests never issue a query.
fn test_app() -> Router {
    test_app_with(&test_config())
}

fn test_app_with(config: &AppConfig) -> Router {
    let ch = clickhouse::Client::default();
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: StellarArchiveFetcher::new(
            runtime_enrichment::stellar_archive::test_client(),
        ),
        // Real SEP-1 fetcher with a stub HTTP client. The spec / health
        // tests below never reach get_asset, so the client never makes a
        // real request.
        sep1: Sep1Fetcher::new().expect("build sep1 fetcher"),
        nft_token_uri: runtime_enrichment::nft_token_uri::NftTokenUriFetcher::with_rpc_url(
            "http://unused".to_owned(),
        )
        .expect("build nft_token_uri fetcher"),
        wasm_code: runtime_enrichment::wasm_code::WasmCodeFetcher::with_rpc_urls(vec![
            "http://unused".to_owned(),
        ])
        .expect("build wasm_code fetcher"),
        account_sponsors:
            runtime_enrichment::account_sponsors::AccountSponsorsFetcher::with_rpc_urls(vec![
                "http://unused".to_owned(),
            ])
            .expect("build account_sponsors fetcher"),
        source_verification: None,
    };
    app(config, AppState::for_tests(ch, runtime_enrichment))
}

/// `/auth/session` is advertised by `ApiDoc` `paths(...)` but mounted by
/// hand when armed, so nothing ties the two together except this test
/// (task 0510). Armed with no Turnstile secret, the route answers 503 —
/// a 404 would mean the spec advertises a path the app does not serve.
#[tokio::test]
async fn armed_app_serves_the_advertised_session_path() {
    let config = AppConfig {
        jwt_secret: Some("test-secret".to_string()),
        ..test_config()
    };
    let app = test_app_with(&config);

    let spec_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api-docs-json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = body::to_bytes(spec_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        spec["paths"]["/auth/session"]["post"].is_object(),
        "spec missing POST /auth/session: {spec}"
    );
    // Task 0677: the mint answers 204 with the session cookie, and the gate no
    // longer reads `Authorization: Bearer`, so the spec must not offer it.
    assert!(
        spec["paths"]["/auth/session"]["post"]["responses"]["204"].is_object(),
        "mint must answer 204: {spec}"
    );
    assert!(
        spec["components"]["securitySchemes"]["bearer_jwt"].is_null(),
        "spec still advertises bearer_jwt: {spec}"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/session")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"token":"t"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

/// Both SPA hosts while the old one still serves (task 0663): each listed
/// origin is echoed back, an unlisted one gets no CORS header.
#[tokio::test]
async fn cors_allows_every_listed_origin() {
    let config = AppConfig {
        cors_allow_origin: Some("https://sorobanscan.com, https://old.example".to_string()),
        ..test_config()
    };
    for (origin, allowed) in [
        ("https://sorobanscan.com", true),
        ("https://old.example", true),
        ("https://evil.example", false),
    ] {
        let response = test_app_with(&config)
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("origin", origin)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let echoed = response.headers().get("access-control-allow-origin");
        assert_eq!(
            echoed.map(|v| v.to_str().unwrap()),
            allowed.then_some(origin)
        );
    }
}

/// The session lives in a cookie (task 0677): a browser exposes a
/// credentialed cross-origin response to the SPA only with this header.
#[tokio::test]
async fn cors_allows_credentials_for_a_listed_origin() {
    let config = AppConfig {
        cors_allow_origin: Some("https://sorobanscan.com".to_string()),
        ..test_config()
    };
    let response = test_app_with(&config)
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("origin", "https://sorobanscan.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .map(|v| v.to_str().unwrap()),
        Some("true")
    );
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
        "/v1/liquidity-pools/{pool_id}/activity",
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

// ── The paid routes under `/x402/v1/` (task 0682) ──────────────────────────
// The CH client is unconnected: none of these reaches a handler, which is the
// point of most of them. The answers that need data are checked on the
// deployed testnet API, by stellar-x402's evidence run.

const PAID_TRANSACTION: &str =
    "/x402/v1/transactions/6bd72a6ef6a77a7dc08e501bd4849b6c28ad88cceabb51a4fd105673949412a2";
const PAY_TO: &str = "GAE63ENKLHRYHJP4V3PYYSPIAKW7DLZ3JOYGS6TQSSJGDH5MGQFVZFVC";

/// A facilitator that settles the `exact` scheme on testnet.
async fn facilitator() -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::path("/supported"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "kinds": [{
                    "x402Version": 2,
                    "scheme": "exact",
                    "network": "stellar:testnet",
                    "extra": { "areFeesSponsored": true }
                }]
            })),
        )
        .mount(&server)
        .await;
    server
}

/// A config that sells through the facilitator at `url`.
fn selling(url: &str) -> AppConfig {
    AppConfig {
        base_url: "https://api-testnet.example.com".to_string(),
        x402: Some(x402::Settings {
            pay_to: PAY_TO.to_string(),
            network: stellar_x402::Network::Testnet,
            facilitator_url: url.to_string(),
            price_usd: "0.001".to_string(),
        }),
        ..test_config()
    }
}

async fn answer_to(app: &Router, request: Request<Body>) -> axum::response::Response {
    app.clone().oneshot(request).await.unwrap()
}

async fn status_of(app: &Router, uri: &str) -> StatusCode {
    let request = Request::builder().uri(uri).body(Body::empty()).unwrap();
    answer_to(app, request).await.status()
}

#[tokio::test]
async fn a_paid_route_answers_its_terms_with_no_key_and_no_session() {
    use base64::Engine;
    let facilitator = facilitator().await;
    // The auth gate is armed, and the request carries neither a key nor a session.
    let config = AppConfig {
        jwt_secret: Some("test-secret".to_string()),
        ..selling(&facilitator.uri())
    };
    let app = test_app_with(&config);

    let request = Request::builder()
        .uri(PAID_TRANSACTION)
        .body(Body::empty())
        .unwrap();
    let response = answer_to(&app, request).await;

    assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    let header = response.headers()["payment-required"].as_bytes();
    let terms: Value = serde_json::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(header)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        terms["resource"]["url"],
        format!("https://api-testnet.example.com{PAID_TRANSACTION}")
    );
    assert_eq!(terms["accepts"][0]["network"], "stellar:testnet");
    assert_eq!(terms["accepts"][0]["payTo"], PAY_TO);
    // 0.001 USDC in its 7 decimals.
    assert_eq!(terms["accepts"][0]["amount"], "10000");
    // One catalog listing for every transaction, not one per hash.
    assert_eq!(
        terms["extensions"]["bazaar"]["routeTemplate"],
        "/x402/v1/transactions/:hash"
    );

    for paid in [
        "/x402/v1/contracts/CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA/events",
        "/x402/v1/contracts/CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA/interface",
    ] {
        assert_eq!(status_of(&app, paid).await, StatusCode::PAYMENT_REQUIRED);
    }
    // Only the three routes are keyless: their keyed twin, another path under
    // the prefix and an unknown path still meet the auth gate.
    for gated in [
        &PAID_TRANSACTION.replace("/x402", ""),
        "/x402/v1/transactions",
        "/no/such/path",
    ] {
        assert_eq!(
            status_of(&app, gated).await,
            StatusCode::UNAUTHORIZED,
            "{gated}"
        );
    }
}

#[tokio::test]
async fn a_paid_route_stays_behind_the_edge_lock() {
    let facilitator = facilitator().await;
    let config = AppConfig {
        edge_secret: Some("edge-secret".to_string()),
        ..selling(&facilitator.uri())
    };
    let app = test_app_with(&config);

    let through_cloudflare = Request::builder()
        .uri(PAID_TRANSACTION)
        .header("x-edge-secret", "edge-secret")
        .body(Body::empty())
        .unwrap();

    assert_eq!(
        status_of(&app, PAID_TRANSACTION).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        answer_to(&app, through_cloudflare).await.status(),
        StatusCode::PAYMENT_REQUIRED
    );
}

#[tokio::test]
async fn without_settings_the_paid_paths_do_not_exist() {
    let facilitator = facilitator().await;
    let wrong_account = AppConfig {
        x402: selling(&facilitator.uri())
            .x402
            .map(|settings| x402::Settings {
                pay_to: "GABC".to_string(),
                ..settings
            }),
        ..test_config()
    };

    for config in [test_config(), wrong_account] {
        let app = test_app_with(&config);
        assert_eq!(
            status_of(&app, PAID_TRANSACTION).await,
            status_of(&app, "/no/such/path").await
        );
        assert_eq!(
            status_of(&app, PAID_TRANSACTION).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(status_of(&app, "/health").await, StatusCode::OK);
    }
}

#[tokio::test]
async fn a_facilitator_outage_costs_the_paid_routes_only() {
    // Nothing is mounted: this facilitator answers 404 to `/supported`.
    let facilitator = wiremock::MockServer::start().await;
    let app = test_app_with(&selling(&facilitator.uri()));

    // The crate answers as `@x402/express` does for a facilitator it could not read.
    assert_eq!(
        status_of(&app, PAID_TRANSACTION).await,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(status_of(&app, "/health").await, StatusCode::OK);
}
