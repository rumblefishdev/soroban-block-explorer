//! End-to-end shape check for `/v1/network/stats`.
//!
//! `CH_URL`-gated — runs only when the env var is set and reachable,
//! skips cleanly otherwise so `cargo test` is green on a workstation
//! without a ClickHouse up.
//!
//!   CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!       cargo test -p api --bin api network -- --test-threads=1
use axum::Router;
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use utoipa_axum::router::OpenApiRouter;

use crate::common::ch::test_client_from_env;
use crate::network;
use crate::runtime_enrichment::RuntimeEnrichment;
use crate::runtime_enrichment::sep1::Sep1Fetcher;
use crate::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
use crate::state::AppState;

fn app(ch: clickhouse::Client) -> Router {
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: StellarArchiveFetcher::new(
            crate::runtime_enrichment::stellar_archive::test_client(),
        ),
        sep1: Sep1Fetcher::new().expect("build sep1 fetcher"),
        nft_token_uri: crate::runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
            .expect("build nft_token_uri fetcher"),
        wasm_code: crate::runtime_enrichment::wasm_code::WasmCodeFetcher::new()
            .expect("build wasm_code fetcher"),
        account_sponsors: crate::runtime_enrichment::account_sponsors::AccountSponsorsFetcher::new(
        )
        .expect("account sponsors fetcher"),
    };
    let state = AppState::for_tests(ch, runtime_enrichment);

    let (router, _spec) = OpenApiRouter::new()
        .nest("/v1", network::router())
        .with_state(state)
        .split_for_parts();
    router
}

/// Each test owns its own `AppState` (and therefore its own moka
/// cache instance), so global serialisation is no longer required —
/// parallel tests cannot trample each other's cache state.
#[tokio::test]
async fn stats_endpoint_returns_documented_shape_against_real_db() {
    let Some(ch) = test_client_from_env() else {
        eprintln!("CH_URL unset — skipping network stats integration test");
        return;
    };

    let resp = app(ch)
        .oneshot(
            Request::builder()
                .uri("/v1/network/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let cc = resp
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");
    assert_eq!(
        cc.as_deref(),
        Some("public, max-age=0, must-revalidate"),
        "Cache-Control header missing or wrong: {cc:?}"
    );

    // Shape asserted regardless of row counts — empty DB is fine.
    for key in [
        "tps_60s",
        "total_accounts",
        "total_contracts",
        "latest_ledger_sequence",
        "generated_at",
    ] {
        assert!(json.get(key).is_some(), "envelope missing `{key}`: {json}");
    }
    assert!(json["tps_60s"].is_number(), "tps_60s not number: {json}");
    assert!(
        json["total_accounts"].is_number(),
        "total_accounts not number: {json}"
    );
    assert!(
        json["total_contracts"].is_number(),
        "total_contracts not number: {json}"
    );
    assert!(
        json["latest_ledger_sequence"].is_number(),
        "latest_ledger_sequence not number: {json}"
    );
    // `latest_ledger_closed_at` may be `null` (empty DB) or an
    // ISO-8601 timestamp string serialised by chrono.
    if let Some(v) = json.get("latest_ledger_closed_at") {
        assert!(
            v.is_null() || v.is_string(),
            "latest_ledger_closed_at bad type: {json}"
        );
    }
    // `generated_at` is always present (DB `NOW()` on populated
    // cluster, `Utc::now()` fallback on empty cluster).
    assert!(
        json["generated_at"].is_string(),
        "generated_at not string: {json}"
    );
}
