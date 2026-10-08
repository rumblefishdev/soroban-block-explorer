//! `CH_URL`-gated conditional-GET tests for `GET /v1/ledgers`.
//! Skips cleanly when the env var is unset/unreachable. Runs against a real
//! ClickHouse — a migrated (possibly empty) `ledgers` table is enough.
use std::sync::atomic::Ordering;

use axum::body::{self, Body};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use utoipa_axum::router::OpenApiRouter;

use crate::common::ch::test_client_from_env;
use crate::runtime_enrichment::RuntimeEnrichment;
use crate::runtime_enrichment::sep1::Sep1Fetcher;
use crate::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
use crate::state::AppState;

fn test_state(ch: clickhouse::Client) -> AppState {
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: StellarArchiveFetcher::new(
            crate::runtime_enrichment::stellar_archive::test_client(),
        ),
        sep1: Sep1Fetcher::new().expect("build sep1 fetcher"),
        nft_token_uri: crate::runtime_enrichment::nft_token_uri::NftTokenUriFetcher::with_rpc_url(
            "http://unused".to_owned(),
        )
        .expect("build nft_token_uri fetcher"),
        wasm_code: crate::runtime_enrichment::wasm_code::WasmCodeFetcher::with_rpc_urls(vec![
            "http://unused".to_owned(),
        ])
        .expect("build wasm_code fetcher"),
    };
    AppState::for_tests(ch, runtime_enrichment)
}

fn app(state: AppState) -> axum::Router {
    let (router, _spec) = OpenApiRouter::new()
        .nest("/v1", crate::ledgers::router())
        .with_state(state)
        .split_for_parts();
    router
}

/// Live first page → `200` + ETag, then a matching `If-None-Match` → `304`
/// empty body with the heavy query NOT re-run (task 0292), asserted via the
/// shared `list_query_count` audit counter.
#[tokio::test]
async fn live_list_304_short_circuits_before_heavy_query() {
    let Some(ch) = test_client_from_env() else {
        eprintln!("CH_URL unset — skipping ledgers conditional-GET test");
        return;
    };
    let state = test_state(ch);

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let etag = resp
        .headers()
        .get(header::ETAG)
        .expect("ETag on live 200")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(state.list_query_count.load(Ordering::Relaxed), 1);

    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=5")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert!(bytes.is_empty(), "304 body must be empty");
    assert_eq!(
        state.list_query_count.load(Ordering::Relaxed),
        1,
        "304 short-circuit must NOT run the heavy query"
    );
}

/// `?order=asc` (oldest, immutable page) is excluded from the conditional
/// layer: no ETag emitted, so it never short-circuits.
#[tokio::test]
async fn asc_oldest_page_emits_no_etag() {
    let Some(ch) = test_client_from_env() else {
        eprintln!("CH_URL unset — skipping ledgers asc test");
        return;
    };
    let state = test_state(ch);

    let resp = app(state)
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=5&order=asc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get(header::ETAG).is_none(),
        "order=asc (immutable oldest page) must not carry a head ETag"
    );
}
