//! `CH_URL`-gated conditional-GET tests for `GET /v1/transactions`.
//! Skips cleanly when the env var is unset/unreachable. Runs against a real
//! ClickHouse — a migrated (possibly empty) `transactions` table is enough.
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
        nft_token_uri: crate::runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
            .expect("build nft_token_uri fetcher"),
        wasm_code: crate::runtime_enrichment::wasm_code::WasmCodeFetcher::new()
            .expect("build wasm_code fetcher"),
    };
    AppState::for_tests(ch, runtime_enrichment)
}

fn app(state: AppState) -> axum::Router {
    let (router, _spec) = OpenApiRouter::new()
        .nest("/v1", crate::transactions::router())
        .with_state(state)
        .split_for_parts();
    router
}

/// The load-bearing acceptance criterion (task 0292): a matching
/// `If-None-Match` on the live first page returns `304` with an empty body
/// **without** running the heavy list query — asserted via the shared
/// `list_query_count` audit counter, which only the heavy path increments.
#[tokio::test]
async fn live_list_304_short_circuits_before_heavy_query() {
    let Some(ch) = test_client_from_env() else {
        eprintln!("CH_URL unset — skipping tx conditional-GET test");
        return;
    };
    let state = test_state(ch);

    // 1) Live first page → 200 + ETag; the heavy query runs exactly once.
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=5")
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
    assert_eq!(
        state.list_query_count.load(Ordering::Relaxed),
        1,
        "first live request must run the heavy query"
    );

    // 2) Same head via If-None-Match → 304, empty body, heavy query NOT run.
    let resp = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=5")
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
