//! `CH_URL`-gated check of the participants endpoint's "not indexed" answer.
//!
//! The web app shows "Not indexed yet" only when the 400 body carries
//! `code: "not_indexed"` (`PoolParticipants.tsx`), so that string is a
//! contract between the two sides; this pins the API half. Runs the real
//! `init.sql` in a throwaway database.

use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use utoipa_axum::router::OpenApiRouter;

use crate::common::ch::test_client_from_env;
use crate::runtime_enrichment::RuntimeEnrichment;
use crate::runtime_enrichment::sep1::Sep1Fetcher;
use crate::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
use crate::state::AppState;

const DB: &str = "api_test_0374_participants_not_indexed";
// A concentrated soroban pool: registered, but with no share token.
const POOL_HEX: &str = "e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4";
const POOL: &str = "CDSOJZHE4TSOJZHE4TSOJZHE4TSOJZHE4TSOJZHE4TSOJZHE4TSOJNPB";

fn app(ch: clickhouse::Client) -> axum::Router {
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
        account_sponsors:
            crate::runtime_enrichment::account_sponsors::AccountSponsorsFetcher::with_rpc_urls(
                vec!["http://unused".to_owned()],
            )
            .expect("build account_sponsors fetcher"),
    };
    let (router, _spec) = OpenApiRouter::new()
        .nest("/v1", crate::liquidity_pools::router())
        .with_state(AppState::for_tests(ch, runtime_enrichment))
        .split_for_parts();
    router
}

#[tokio::test]
async fn pool_without_share_token_answers_not_indexed() {
    let Some(base) = test_client_from_env() else {
        eprintln!("CH_URL unset — skipping participants not_indexed check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");
    for sql in [
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind) VALUES \
             (unhex('{POOL_HEX}'), 30, 10, 1)"
        ),
        format!(
            "INSERT INTO pool_instance_state (pool_id, plane_id, share_token_id, total_shares, derived_at_ledger) VALUES \
             (unhex('{POOL_HEX}'), 1, 0, 0, 10)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let resp = app(ch)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/liquidity-pools/{POOL}/participants"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value =
        serde_json::from_slice(&body::to_bytes(resp.into_body(), usize::MAX).await.unwrap())
            .expect("error body is JSON");
    assert_eq!(body["code"], "not_indexed");

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
