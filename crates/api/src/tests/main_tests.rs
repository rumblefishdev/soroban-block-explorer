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
        nft_token_uri: runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
            .expect("build nft_token_uri fetcher"),
        wasm_code: runtime_enrichment::wasm_code::WasmCodeFetcher::new()
            .expect("build wasm_code fetcher"),
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
