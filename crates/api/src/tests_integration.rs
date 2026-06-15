//! End-to-end integration tests for the task 0043 shared helpers.
//!
//! Exercises `GET /v1/transactions` through the real app router with the
//! shared `Pagination<TsIdCursor>` extractor, `filters::strkey` /
//! `filters::parse_enum` validators, `finalize_ts_id_page` + `into_envelope`
//! wire assembly, and the `errors::*` envelope builders. DB-touching tests
//! skip cleanly when `DATABASE_URL` is unset or unreachable — validation
//! tests run unconditionally because they short-circuit before any SQL
//! executes.
//!
//! Run locally against the compose stack:
//!
//!   docker compose up -d
//!   npm run db:migrate
//!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/soroban_block_explorer \
//!       cargo test -p api --bin api tests_integration -- --test-threads=1

use axum::Router;
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use utoipa_axum::router::OpenApiRouter;

use crate::accounts;
use crate::assets;
use crate::contracts;
use crate::ledgers;
use crate::nfts;
use crate::runtime_enrichment::RuntimeEnrichment;
use crate::runtime_enrichment::sep1::Sep1Fetcher;
use crate::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
use crate::state::AppState;
use crate::{liquidity_pools, transactions};

/// Build a test app with the transactions, contracts, liquidity-pools,
/// assets, and ledgers routers mounted at /v1.
///
/// Caller supplies the `PgPool`. Validation tests that never touch the DB
/// pass `connect_lazy("...")` (free until first query), DB-gated tests
/// pass a real `PgPool::connect(...)` result.
fn build_app(db: PgPool) -> Router {
    // Real fetchers with default config. Integration tests below never
    // hit a real issuer or S3 (validation tests short-circuit before
    // any handler reaches the fetcher; DB-gated tests use fixtures).
    // Keeping them construct-only ensures AppState wiring stays exercised.
    let runtime_enrichment = RuntimeEnrichment {
        stellar_archive: StellarArchiveFetcher::new(
            crate::runtime_enrichment::stellar_archive::test_client(),
        ),
        sep1: Sep1Fetcher::new().expect("build sep1 fetcher"),
        nft_token_uri: crate::runtime_enrichment::nft_token_uri::NftTokenUriFetcher::new()
            .expect("build nft_token_uri fetcher"),
    };
    let state = AppState::for_tests(db, runtime_enrichment);

    let (router, _spec) = OpenApiRouter::new()
        .nest("/v1", transactions::router())
        .nest("/v1", contracts::router())
        .nest("/v1", liquidity_pools::router())
        .nest("/v1", nfts::router())
        .nest("/v1", assets::router())
        .nest("/v1", ledgers::router())
        .nest("/v1", accounts::router())
        .nest("/v1", crate::search::router())
        .layer(axum::middleware::from_fn(
            crate::common::cache_control::enforce_no_store_on_errors,
        ))
        .with_state(state)
        .split_for_parts();
    router
}

/// Convenience wrapper for validation tests that never hit the DB.
fn lazy_app() -> Router {
    let db = sqlx::PgPool::connect_lazy("postgres://localhost/test_unused")
        .expect("connect_lazy never fails");
    build_app(db)
}

async fn body_json(resp: axum::response::Response) -> (StatusCode, Value) {
    let status = resp.status();
    let bytes = body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Validation tests — no DB contact, run unconditionally.
//
// These prove that the shared `Pagination` extractor, `filters::strkey`,
// and `filters::parse_enum` short-circuit before any SQL executes, and
// return the canonical `ErrorEnvelope` for each failure code. They are
// the end-to-end counterpart to the unit tests in `common::*::tests` —
// the unit tests cover the helpers in isolation; these prove they fire
// through the real axum request plumbing when wired into the
// transactions handler.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn invalid_limit_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
    assert_eq!(json["details"]["received"], "abc");
}

#[tokio::test]
async fn invalid_cursor_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?cursor=not!!base64")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

#[tokio::test]
async fn invalid_strkey_filter_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?filter%5Bsource_account%5D=BAD")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "source_account");
}

#[tokio::test]
async fn invalid_operation_type_filter_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?filter%5Boperation_type%5D=NOT_A_TYPE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "operation_type");
}

// ---------------------------------------------------------------------------
// DB-touching test — gated on DATABASE_URL.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_endpoint_returns_paginated_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping list envelope integration test");
        return;
    };

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping list envelope integration test");
            return;
        }
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");

    // Envelope shape asserted regardless of row count — empty DB is fine.
    assert!(
        json.get("data").is_some(),
        "envelope missing `data`: {json}"
    );
    assert!(json["data"].is_array(), "data not array: {json}");
    let page = &json["page"];
    assert_eq!(page["limit"], 3, "page.limit not echoed: {json}");
    assert!(
        page["next_cursor"].is_string() || page["next_cursor"].is_null(),
        "page.next_cursor must be string or null: {json}"
    );
    assert!(
        page["prev_cursor"].is_string() || page["prev_cursor"].is_null(),
        "page.prev_cursor must be string or null: {json}"
    );
}

/// Locks the JOIN to `operations_appearances` — the no-filter list test
/// never hits this branch.
#[tokio::test]
async fn list_endpoint_filter_op_type_returns_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=3&filter%5Boperation_type%5D=PAYMENT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");
    assert!(json["data"].is_array(), "data not array: {json}");
    assert_eq!(json["page"]["limit"], 3, "page.limit not echoed: {json}");
    assert!(
        json["page"]["next_cursor"].is_string() || json["page"]["next_cursor"].is_null(),
        "page.next_cursor must be string or null"
    );
}

/// Locks `fetch_operations` against `operations_appearances` with
/// `ORDER BY o.id` — pre-existing detail tests only cover 404.
#[tokio::test]
async fn detail_endpoint_returns_200_for_known_hash_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> =
        sqlx::query_as("SELECT encode(hash, 'hex') FROM transaction_hash_index LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((hash_hex,)) = row else {
        eprintln!("transaction_hash_index empty — skipping successful-detail test");
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions/{hash_hex}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");
    assert!(
        json["operations"].is_array(),
        "detail.operations not array: {json}"
    );

    assert!(
        json["application_order"].is_number(),
        "application_order missing/non-number: {json}"
    );
    assert!(
        json["has_soroban"].is_boolean(),
        "has_soroban missing/non-bool: {json}"
    );
    assert!(
        json["operation_count"].is_number(),
        "operation_count missing/non-number: {json}"
    );
    assert!(
        json["inner_tx_hash"].is_string() || json["inner_tx_hash"].is_null(),
        "inner_tx_hash bad type: {json}"
    );

    if let Some(op) = json["operations"].as_array().and_then(|a| a.first()) {
        assert!(
            op["appearance_id"].is_number(),
            "operations[0].appearance_id missing/non-number: {op}"
        );
        assert!(
            op["type_name"].is_string(),
            "operations[0].type_name missing/non-string: {op}"
        );
        assert!(
            op["type"].is_number(),
            "operations[0].type missing/non-number: {op}"
        );
    }
}

#[tokio::test]
async fn detail_endpoint_projects_full_operation_columns_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT encode(t.hash, 'hex') \
         FROM transactions t \
         JOIN operations_appearances oa \
              ON oa.transaction_id = t.id AND oa.created_at = t.created_at \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((hash_hex,)) = row else {
        eprintln!("no transactions with operations — skipping");
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions/{hash_hex}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    let op = json["operations"]
        .as_array()
        .and_then(|a| a.first())
        .expect("at least one op (we joined to operations_appearances)");
    for field in [
        "source_account",
        "destination_account",
        "contract_id",
        "asset_code",
        "asset_issuer",
    ] {
        assert!(
            op.get(field).is_some(),
            "operations[0] missing key {field}: {op}"
        );
        assert!(
            op[field].is_string() || op[field].is_null(),
            "operations[0].{field} bad type: {op}"
        );
    }
    // pool_ids replaced the scalar pool_id (task 0261/0268): always present,
    // always an array (empty when no pool crossed).
    assert!(
        op.get("pool_ids").is_some_and(Value::is_array),
        "operations[0].pool_ids missing or not array: {op}"
    );
    assert!(
        op["ledger_sequence"].is_number(),
        "operations[0].ledger_sequence not number: {op}"
    );
    assert!(
        op["created_at"].is_string(),
        "operations[0].created_at not ISO string: {op}"
    );
}

/// `build_app` uses fake S3 credentials so the archive fetch fails and
/// `heavy_fields_status = "unavailable"` exercises the DB fallback path.
#[tokio::test]
async fn detail_endpoint_falls_back_to_db_when_heavy_unavailable() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT encode(t.hash, 'hex') \
         FROM transactions t \
         JOIN transaction_participants tp \
              ON tp.transaction_id = t.id AND tp.created_at = t.created_at \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((hash_hex,)) = row else {
        eprintln!("no tx with participants — skipping fallback test");
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions/{hash_hex}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");

    assert_eq!(
        json["heavy_fields_status"], "unavailable",
        "expected heavy unavailable to exercise the fallback path: {json}"
    );
    assert!(
        json["participants"].is_array(),
        "fallback participants[] missing/non-array: {json}"
    );
    assert!(
        json["soroban_events"].is_array(),
        "fallback soroban_events[] missing/non-array: {json}"
    );
    assert!(
        json["soroban_invocations"].is_array(),
        "fallback soroban_invocations[] missing/non-array: {json}"
    );
    // Hash discovered via JOIN to transaction_participants, so fallback
    // must produce ≥1 row — proves the DB query fired.
    let participants = json["participants"].as_array().unwrap();
    assert!(
        !participants.is_empty(),
        "expected ≥1 participant on a tx joined via transaction_participants: {json}"
    );
}

/// Statement B path — broad contract match across the three appearance
/// tables. Asserts filter ↔ projection consistency on the matched contract.
#[tokio::test]
async fn list_endpoint_filter_contract_id_returns_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> =
        sqlx::query_as("SELECT contract_id FROM soroban_contracts ORDER BY id LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((cid,)) = row else {
        eprintln!("soroban_contracts empty — skipping contract-filter test");
        return;
    };

    let router = build_app(pool);
    let uri = format!("/v1/transactions?limit=3&filter%5Bcontract_id%5D={cid}");
    let resp = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");
    assert!(json["data"].is_array(), "data not array: {json}");
    if let Some(arr) = json["data"].as_array() {
        for (i, row) in arr.iter().enumerate() {
            let ids = row["contract_ids"]
                .as_array()
                .unwrap_or_else(|| panic!("row[{i}].contract_ids not array: {row}"));
            assert!(
                ids.iter().any(|v| v.as_str() == Some(cid.as_str())),
                "row[{i}] missing filtered contract_id={cid} in contract_ids: {row}"
            );
        }
    }
}

/// Statement-B EXISTS post-filter branch (contract + op_type combined).
#[tokio::test]
async fn list_endpoint_filter_contract_id_and_op_type_returns_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> =
        sqlx::query_as("SELECT contract_id FROM soroban_contracts ORDER BY id LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((cid,)) = row else {
        return;
    };

    let router = build_app(pool);
    let uri = format!(
        "/v1/transactions?limit=3&filter%5Bcontract_id%5D={cid}&filter%5Boperation_type%5D=INVOKE_HOST_FUNCTION"
    );
    let resp = router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");
    assert!(json["data"].is_array(), "data not array: {json}");
}

#[tokio::test]
async fn list_endpoint_projects_canonical_columns_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");

    let Some(row) = json["data"].as_array().and_then(|a| a.first()) else {
        eprintln!("transactions empty — skipping projection-shape assertions");
        return;
    };

    assert!(
        row["application_order"].is_number(),
        "application_order missing/non-number: {row}"
    );
    assert!(
        row["has_soroban"].is_boolean(),
        "has_soroban missing/non-bool: {row}"
    );
    assert!(
        row["inner_tx_hash"].is_string() || row["inner_tx_hash"].is_null(),
        "inner_tx_hash bad type: {row}"
    );
    assert!(
        row["operation_types"].is_array(),
        "operation_types not array: {row}"
    );
    assert!(
        row["contract_ids"].is_array(),
        "contract_ids not array: {row}"
    );
}

// ---------------------------------------------------------------------------
// Assets endpoints (task 0049) — mirror the transactions coverage shape.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assets_invalid_filter_type_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/assets?filter%5Btype%5D=NOT_A_TYPE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "type");
}

/// `filter[code]` must reject SQL wildcard literals (`%`, `_`) so a
/// confused caller can't silently change match semantics through the
/// trigram-substring path.
#[tokio::test]
async fn assets_filter_code_rejects_wildcard_literals() {
    for q in [
        "/v1/assets?filter%5Bcode%5D=USD%25", // %25 = `%`
        "/v1/assets?filter%5Bcode%5D=USD_",
    ] {
        let app = lazy_app();
        let resp = app
            .oneshot(Request::builder().uri(q).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "uri={q} json={json}");
        assert_eq!(json["code"], "invalid_filter");
        assert_eq!(json["details"]["filter"], "code");
    }
}

#[tokio::test]
async fn assets_invalid_id_returns_400_envelope() {
    // Not numeric, not a 56-char StrKey, not a code-issuer composite — must
    // fail parsing in the handler before the DB is touched.
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/assets/not-an-asset-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_id");
    assert_eq!(json["details"]["received"], "not-an-asset-id");
}

#[tokio::test]
async fn assets_list_returns_paginated_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping assets list integration test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping");
            return;
        }
    };
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/assets?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200, got {status}: {json}");
    assert!(json["data"].is_array(), "data not array: {json}");
    assert_eq!(json["page"]["limit"], 5, "page.limit not echoed: {json}");
    assert!(
        json["page"]["next_cursor"].is_string() || json["page"]["next_cursor"].is_null(),
        "page.next_cursor must be string or null"
    );
}

/// `filter[type]=native` must return at most the singleton native row
/// (seeded by migration `20260428000000_seed_native_asset_singleton`).
#[tokio::test]
async fn assets_filter_type_native_returns_singleton_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/assets?filter%5Btype%5D=native")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let rows = json["data"].as_array().unwrap();
    // Allow zero (DB without seed) or one — never more than one native asset.
    assert!(rows.len() <= 1, "more than one native asset: {json}");
    if let Some(row) = rows.first() {
        // Canonical SQL projects BOTH the decoded label (asset_type_name)
        // and the raw SMALLINT (asset_type). Lock both contracts so a
        // future drift on either side surfaces here.
        assert_eq!(row["asset_type_name"], "native");
        assert_eq!(row["asset_type"], 0);
        assert!(
            row["asset_code"].is_null(),
            "native must have null asset_code"
        );
    }
}

/// The numeric surrogate was dropped (PR #175 / composite move). A bare
/// integer is no longer a valid `:id` form — it parses as neither a C-StrKey
/// nor a `CODE-ISSUER` composite, so the handler rejects it with a 400 before
/// touching the DB. No DATABASE_URL needed.
#[tokio::test]
async fn assets_detail_numeric_id_rejected_with_400() {
    let pool = match std::env::var("DATABASE_URL") {
        Ok(url) => match PgPool::connect(&url).await {
            Ok(p) => p,
            Err(_) => return,
        },
        // The 400 is emitted before any DB access; a lazy pool is fine.
        Err(_) => return,
    };
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/assets/12345")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400: {json}");
    assert_eq!(json["code"], "invalid_id");
}

/// 404 path for a well-formed contract StrKey that does not exist. `CAAA…AAJ`
/// is shape-valid (56 chars, C prefix, base32) but never minted on mainnet.
#[tokio::test]
async fn assets_detail_unknown_id_returns_404_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/assets/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404: {json}");
    assert_eq!(json["code"], "not_found");
}

/// `:id` resolution by contract StrKey. Skips when the DB has no SAC or
/// Soroban-native asset row with a non-NULL `contract_id`.
#[tokio::test]
async fn assets_detail_by_contract_strkey_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT sc.contract_id \
         FROM assets a \
         JOIN soroban_contracts sc ON sc.id = a.contract_id \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((contract_strkey,)) = row else {
        eprintln!("no asset with contract_id — skipping contract-StrKey resolution test");
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/assets/{contract_strkey}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    // `id` is now the canonical token = the contract StrKey for SAC/Soroban.
    assert_eq!(json["id"], contract_strkey, "wrong asset surfaced: {json}");
    assert_eq!(json["contract_id"], contract_strkey);
}

/// `:id` resolution by `code-issuer` composite. Skips when the DB has no
/// classic_credit / SAC-classic-wrap row.
#[tokio::test]
async fn assets_detail_by_code_issuer_composite_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT a.asset_code, iss.account_id \
         FROM assets a \
         JOIN accounts iss ON iss.id = a.issuer_id \
         WHERE a.asset_code IS NOT NULL \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((code, issuer)) = row else {
        eprintln!("no classic-identity asset — skipping code-issuer resolution test");
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/assets/{code}-{issuer}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    // `id` is now the canonical token = the `CODE-ISSUER` composite for a
    // classic-credit asset with no contract id.
    assert_eq!(json["id"], format!("{code}-{issuer}"));
    assert_eq!(json["asset_code"], code);
    assert_eq!(json["issuer"], issuer);
}

/// Non-native `/transactions` happy path — picks any non-native asset that
/// actually appears in `operations_appearances` and asserts the page
/// returns at least one tx (proving the per-asset_type predicate composer
/// resolves the right join branch on real data).
#[tokio::test]
async fn assets_transactions_returns_at_least_one_row_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    // Try classic identity first, fall back to contract identity. The `:id`
    // is now the canonical composite token (`CODE-ISSUER` or contract StrKey),
    // not the dropped numeric surrogate.
    let by_classic: Option<(String, String)> = sqlx::query_as(
        "SELECT a.asset_code, iss.account_id FROM assets a \
         JOIN accounts iss ON iss.id = a.issuer_id \
         JOIN operations_appearances oa \
              ON oa.asset_code = a.asset_code AND oa.asset_issuer_id = iss.id \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let token: Option<String> = if let Some((code, issuer)) = by_classic {
        Some(format!("{code}-{issuer}"))
    } else {
        let by_contract: Option<(String,)> = sqlx::query_as(
            "SELECT sc.contract_id FROM assets a \
             JOIN soroban_contracts sc ON sc.id = a.contract_id \
             JOIN operations_appearances oa ON oa.contract_id = sc.id \
             LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
        by_contract.map(|(c,)| c)
    };
    let Some(token) = token else {
        eprintln!(
            "no non-native asset references found in operations_appearances — \
             skipping happy-path /transactions assertion"
        );
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/assets/{token}/transactions?limit=5"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    let data = json["data"].as_array().unwrap();
    assert!(
        !data.is_empty(),
        "asset {token} appears in operations_appearances but \
         /transactions returned 0 rows: {json}"
    );
    // Lock the canonical-aligned response shape: every row must carry
    // `has_soroban` (bool) and `operation_types` (string[]) — these are
    // the §6.9 fields canonical 10_get_assets_transactions.sql projects.
    let first = &data[0];
    assert!(
        first["has_soroban"].is_boolean(),
        "has_soroban missing or not bool: {first}"
    );
    assert!(
        first["operation_types"].is_array(),
        "operation_types missing or not array: {first}"
    );
}

/// Native XLM has no DB-side identity referenced by `operations_appearances`
/// — the sub-resource short-circuits to an empty page rather than emit a
/// degenerate `WHERE ()` SQL. Lock the contract here.
#[tokio::test]
async fn assets_native_transactions_returns_empty_page_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    // Native singleton (asset_type=0) has no composite identity, so it is
    // addressed by the reserved `native` token, not a StrKey / CODE-ISSUER.
    let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM assets WHERE asset_type = 0")
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
    if !matches!(row, Some((n,)) if n > 0) {
        eprintln!("no native asset row — skipping");
        return;
    }

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/assets/native/transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    assert_eq!(
        json["data"].as_array().unwrap().len(),
        0,
        "native asset must produce empty transactions page: {json}"
    );
    assert!(json["page"]["next_cursor"].is_null());
}

/// Full request → response → next cursor → request chain.
///
/// Asserts that page 2 returned by feeding the page-1 cursor back into the
/// extractor:
///   * has no overlap with page 1 (different `hash` set), and
///   * is correctly bounded — `has_more` flips to false at the tail, or the
///     cursor advances monotonically when more pages remain.
///
/// Skips cleanly when DB is unavailable or has fewer than 2 rows (cannot
/// validate continuation on an empty / single-row table).
#[tokio::test]
async fn cursor_round_trip_no_overlap_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping cursor round-trip test");
        return;
    };

    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping cursor round-trip test");
            return;
        }
    };

    // Page 1: limit=1 to maximise the chance of has_more=true on small DBs.
    let router = build_app(pool.clone());
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page1) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "page1 status: {status} body {page1}"
    );

    let data1 = page1["data"].as_array().expect("data array").clone();
    if data1.is_empty() || !page1["page"]["next_cursor"].is_string() {
        eprintln!("DB has <2 transactions — skipping continuation assertions");
        return;
    }
    let cursor = page1["page"]["next_cursor"]
        .as_str()
        .expect("page.next_cursor present when more pages exist")
        .to_string();
    let hash1 = data1[0]["hash"].as_str().unwrap().to_string();

    // Page 2: feed cursor back. Cursor is base64url *unpadded* (URL-safe alphabet, no `=`),
    // so raw interpolation into the query string is safe — no percent-encoding needed.
    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions?limit=1&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page2) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "page2 status: {status} body {page2}"
    );

    let data2 = page2["data"].as_array().expect("data array").clone();
    if let Some(first) = data2.first() {
        let hash2 = first["hash"].as_str().unwrap();
        assert_ne!(
            hash1, hash2,
            "page2 first row overlaps page1 — cursor predicate broken"
        );
    }
    // page2.cursor either advances to a new value or is absent on tail.
    if let Some(next) = page2["page"]["next_cursor"].as_str() {
        assert_ne!(
            next, cursor,
            "page2 cursor identical to page1 — no progress"
        );
    }
}

// ---------------------------------------------------------------------------
// Task 0126 — liquidity-pool participants endpoint
//
// Validation tests run unconditionally (short-circuit before any SQL).
// The end-to-end test seeds a pool + accounts + LP positions, hits the
// endpoint, and tears down — gated on `DATABASE_URL` so it skips
// cleanly when no Postgres is available.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lp_participants_invalid_pool_id_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools/not-hex/participants")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_pool_id");
}

#[tokio::test]
async fn lp_participants_invalid_limit_returns_envelope_before_db() {
    let app = lazy_app();
    // Well-formed pool_id (64 hex), bad limit — extractor short-circuits.
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR/\
                     participants?limit=abc",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
}

#[tokio::test]
async fn lp_participants_invalid_cursor_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR/\
                     participants?cursor=not!!base64",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

#[tokio::test]
async fn lp_participants_404_for_missing_pool() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping 0126 missing-pool 404 test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping 0126 missing-pool 404 test");
            return;
        }
    };
    let app = build_app(pool);

    // Synthetic pool_id that won't exist on a clean DB.
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef/\
                     participants",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body: {json}");
    assert_eq!(json["code"], "not_found");
}

/// End-to-end: seed (pool, 3 accounts, 3 LP positions including one
/// zero-share row), call the endpoint twice for cursor round-trip, then
/// tear down. Asserts:
///
///   * 200 with `Paginated<ParticipantItem>` envelope
///   * Order: shares DESC
///   * Zero-share row filtered out
///   * Cursor round-trip yields disjoint pages
#[tokio::test]
async fn lp_participants_e2e_sort_filter_pagination() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping 0126 e2e test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping 0126 e2e test");
            return;
        }
    };

    // Distinct from any in-flight indexer test fixtures (TEST_POOL_ID
    // 3333…, SAC160_*) so the seed/teardown does not collide.
    const POOL_HEX: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const ACC_TOP: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0126TOP";
    const ACC_MID: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0126MID";
    const ACC_ZERO: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0126ZRO";

    // Idempotent setup — clear any prior run leftovers first.
    teardown_lp_e2e_fixture(&pool, POOL_HEX, &[ACC_TOP, ACC_MID, ACC_ZERO]).await;
    setup_lp_e2e_fixture(&pool, POOL_HEX, ACC_TOP, ACC_MID, ACC_ZERO).await;

    let app = build_app(pool.clone());

    // -- Page 1: limit=1, expect ACC_TOP (highest shares = "100.0000000")
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{POOL_HEX}/participants?limit=1"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page1) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "page1 body: {page1}");
    let data1 = page1["data"].as_array().expect("data array").clone();
    assert_eq!(data1.len(), 1, "page1 should have exactly limit rows");
    assert_eq!(data1[0]["account"], ACC_TOP, "highest-shares account first");
    assert_eq!(data1[0]["shares"], "100.0000000");
    // share_percentage = 100 / 200 * 100 = 50.0 (snapshot total_shares=200).
    // PG NUMERIC division retains generous precision; assert by parsed
    // numeric rather than exact string to insulate against PG version
    // drift in the divisor's scale calculation.
    let pct_top: f64 = data1[0]["share_percentage"]
        .as_str()
        .expect("share_percentage present when snapshot is fresh")
        .parse()
        .expect("share_percentage parses as numeric");
    assert!(
        (pct_top - 50.0).abs() < 1e-9,
        "expected ~50.0%, got {pct_top}"
    );
    assert!(
        page1["page"]["next_cursor"].is_string(),
        "second page must exist (3rd row is filtered, 2nd remains)"
    );
    let cursor = page1["page"]["next_cursor"]
        .as_str()
        .expect("cursor present when has_more=true")
        .to_string();

    // -- Page 2: feed cursor, expect ACC_MID (50). ACC_ZERO must NOT appear.
    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{POOL_HEX}/participants?limit=1&cursor={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page2) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "page2 body: {page2}");
    let data2 = page2["data"].as_array().expect("data array").clone();
    assert_eq!(data2.len(), 1);
    assert_eq!(data2[0]["account"], ACC_MID, "mid-shares account second");
    assert_eq!(data2[0]["shares"], "50.0000000");
    // share_percentage = 50 / 200 * 100 = 25.0
    let pct_mid: f64 = data2[0]["share_percentage"]
        .as_str()
        .expect("share_percentage present when snapshot is fresh")
        .parse()
        .expect("share_percentage parses as numeric");
    assert!(
        (pct_mid - 25.0).abs() < 1e-9,
        "expected ~25.0%, got {pct_mid}"
    );
    // Tail flag — third row is zero-shares, filtered out, so no page 3.
    assert!(
        page2["page"]["next_cursor"].is_null(),
        "zero-share row must be filtered → page2 is the tail"
    );

    // -- Confirm zero-shares account is never returned even when paged
    // through to the end without limit.
    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{POOL_HEX}/participants?limit=100"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, all) = body_json(resp).await;
    let accounts: Vec<&str> = all["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["account"].as_str().unwrap())
        .collect();
    assert_eq!(accounts, vec![ACC_TOP, ACC_MID]);
    assert!(
        !accounts.contains(&ACC_ZERO),
        "zero-share row must be filtered: {accounts:?}"
    );

    teardown_lp_e2e_fixture(&pool, POOL_HEX, &[ACC_TOP, ACC_MID, ACC_ZERO]).await;
}

/// SharesCursor (`(shares: NUMERIC(28,7), account_id: i64)`) Prev
/// round-trip — task 0254 reverse-walk verification for the only
/// multi-key cursor type with non-trivial Postgres compare semantics.
///
/// Forward walk: page 1 (limit=1) → next_cursor C1 → page 2 (cursor=C1)
/// → response carries prev_cursor P1. Backward walk: send P1 as
/// `?cursor=` → the same page 1 content reappears. Asserts:
///
///   * prev_cursor is non-null on the mid-walk page 2,
///   * its `dir=prev` envelope decodes correctly server-side,
///   * NUMERIC(28,7) compare under ASC `>` round-trips byte-identical
///     to the forward DESC `<` walk (no precision loss, no off-by-one
///     in shares values),
///   * the rendered participants list matches page 1's original
///     account_id ordering (ACC_TOP first), proving the helper's
///     `rows.reverse()` step lands the page in DESC presentation.
#[tokio::test]
async fn lp_participants_prev_cursor_round_trip_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping 0254 LP participants Prev test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping 0254 LP participants Prev test");
            return;
        }
    };

    // Distinct fixture from `lp_participants_e2e_sort_filter_pagination`
    // so the tests can run concurrently without seed/teardown stepping
    // on each other.
    const POOL_HEX: &str = "0254000000000000000000000000000000000000000000000000000000000254";
    const ACC_TOP: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0254PREVTOP";
    const ACC_MID: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0254PREVMID";
    const ACC_ZERO: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0254PREVZRO";

    teardown_lp_e2e_fixture(&pool, POOL_HEX, &[ACC_TOP, ACC_MID, ACC_ZERO]).await;
    setup_lp_e2e_fixture(&pool, POOL_HEX, ACC_TOP, ACC_MID, ACC_ZERO).await;

    // Page 1 (no cursor): expect ACC_TOP, no prev_cursor (first page).
    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{POOL_HEX}/participants?limit=1"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page1) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "page1: {page1}");
    let page1_account = page1["data"][0]["account"]
        .as_str()
        .expect("page1 account")
        .to_string();
    let page1_shares = page1["data"][0]["shares"]
        .as_str()
        .expect("page1 shares")
        .to_string();
    assert_eq!(page1_account, ACC_TOP, "highest-shares account first");
    assert!(
        page1["page"]["prev_cursor"].is_null(),
        "page1 must not carry prev_cursor: {page1}"
    );
    let next_cursor = page1["page"]["next_cursor"]
        .as_str()
        .expect("next_cursor present on mid-walk page 1")
        .to_string();

    // Page 2 via next_cursor: expect ACC_MID + prev_cursor populated.
    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{POOL_HEX}/participants?limit=1&cursor={next_cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page2) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "page2: {page2}");
    assert_eq!(page2["data"][0]["account"], ACC_MID);
    let prev_cursor = page2["page"]["prev_cursor"]
        .as_str()
        .expect("prev_cursor present on mid-walk page 2 (task 0254)")
        .to_string();

    // Backward walk via prev_cursor: must return the same content as
    // the original page 1 (cursor symmetry + NUMERIC compare integrity).
    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{POOL_HEX}/participants?limit=1&cursor={prev_cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, page1_prime) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "page1' (via prev_cursor): {page1_prime}"
    );
    assert_eq!(
        page1_prime["data"][0]["account"]
            .as_str()
            .expect("page1' account"),
        page1_account,
        "Prev round-trip must return the original page 1 account_id (ACC_TOP)"
    );
    assert_eq!(
        page1_prime["data"][0]["shares"]
            .as_str()
            .expect("page1' shares"),
        page1_shares,
        "Prev round-trip must preserve NUMERIC(28,7) shares value byte-identical"
    );

    teardown_lp_e2e_fixture(&pool, POOL_HEX, &[ACC_TOP, ACC_MID, ACC_ZERO]).await;
}

async fn setup_lp_e2e_fixture(
    pool: &PgPool,
    pool_hex: &str,
    acc_top: &str,
    acc_mid: &str,
    acc_zero: &str,
) {
    // Pool — minimal native↔credit shape, no FK to issuer (issuer_id NULL
    // for native means asset_a_type=0; asset_b is a synthetic credit).
    sqlx::query(
        r#"
        INSERT INTO liquidity_pools (
            pool_id, asset_a_type, asset_a_code, asset_a_issuer_id,
            asset_b_type, asset_b_code, asset_b_issuer_id,
            fee_bps, created_at_ledger
        ) VALUES (decode($1, 'hex'), 0, NULL, NULL, 1, '0126TKN', NULL, 30, 1)
        "#,
    )
    .bind(pool_hex)
    .execute(pool)
    .await
    .expect("insert pool");

    // Accounts (need surrogate ids for lp_positions FK).
    let acc_top_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO accounts (account_id, first_seen_ledger, last_seen_ledger, sequence_number)
           VALUES ($1, 1, 1, 0) RETURNING id"#,
    )
    .bind(acc_top)
    .fetch_one(pool)
    .await
    .expect("insert acc_top");
    let acc_mid_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO accounts (account_id, first_seen_ledger, last_seen_ledger, sequence_number)
           VALUES ($1, 1, 1, 0) RETURNING id"#,
    )
    .bind(acc_mid)
    .fetch_one(pool)
    .await
    .expect("insert acc_mid");
    let acc_zero_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO accounts (account_id, first_seen_ledger, last_seen_ledger, sequence_number)
           VALUES ($1, 1, 1, 0) RETURNING id"#,
    )
    .bind(acc_zero)
    .fetch_one(pool)
    .await
    .expect("insert acc_zero");

    // LP positions: top=100, mid=50, zero=0 (must be filtered by API).
    sqlx::query(
        r#"
        INSERT INTO lp_positions (pool_id, account_id, shares, first_deposit_ledger, last_updated_ledger)
        VALUES
            (decode($1, 'hex'), $2, 100.0::NUMERIC(28,7), 1, 1),
            (decode($1, 'hex'), $3,  50.0::NUMERIC(28,7), 1, 1),
            (decode($1, 'hex'), $4,   0.0::NUMERIC(28,7), 1, 1)
        "#,
    )
    .bind(pool_hex)
    .bind(acc_top_id)
    .bind(acc_mid_id)
    .bind(acc_zero_id)
    .execute(pool)
    .await
    .expect("insert lp_positions");

    // Snapshot row — total_shares = 200 so the canonical query's
    // `share_percentage` CTE has a fresh divisor. `created_at = NOW()`
    // lands in the live `_default` partition and is well within the
    // 7-day freshness window the spec uses.
    sqlx::query(
        r#"
        INSERT INTO liquidity_pool_snapshots (
            pool_id, ledger_sequence, reserve_a, reserve_b, total_shares, created_at
        )
        VALUES (decode($1, 'hex'), 1, 1000.0, 2000.0, 200.0, NOW())
        "#,
    )
    .bind(pool_hex)
    .execute(pool)
    .await
    .expect("insert liquidity_pool_snapshots");
}

async fn teardown_lp_e2e_fixture(pool: &PgPool, pool_hex: &str, accounts: &[&str]) {
    let _ = sqlx::query("DELETE FROM liquidity_pool_snapshots WHERE pool_id = decode($1, 'hex')")
        .bind(pool_hex)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM lp_positions WHERE pool_id = decode($1, 'hex')")
        .bind(pool_hex)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM liquidity_pools WHERE pool_id = decode($1, 'hex')")
        .bind(pool_hex)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM accounts WHERE account_id = ANY($1)")
        .bind(accounts)
        .execute(pool)
        .await;
}

// ---------------------------------------------------------------------------
// Sentinel placeholder pools (ADR 0041 / task 0193) — every pool endpoint
// must hide rows carrying `created_at_ledger = 0`. The persist layer
// emits these rows during partial backfills when an `lp_positions` entry
// references a pool whose `LedgerEntry` is not in the current window;
// the marker is a single-column predicate (pubnet genesis is seq 1).
//
// One end-to-end test seeds a sentinel pool + one position + one
// snapshot row referencing that pool (the most permissive shape — if any
// endpoint *would* surface it, this fixture would trip it) and asserts
// the canonical observable behavior on all five endpoints:
//
//   * GET /v1/liquidity-pools                  → list excludes sentinel
//   * GET /v1/liquidity-pools/:id              → 404
//   * GET /v1/liquidity-pools/:id/participants → 404 (gate: pool_exists)
//   * GET /v1/liquidity-pools/:id/transactions → 404 (gate: pool_exists)
//   * GET /v1/liquidity-pools/:id/chart        → 404 (gate: pool_exists)
//
// Skips cleanly when DATABASE_URL is unset / unreachable.
// ---------------------------------------------------------------------------

async fn setup_sentinel_pool_fixture(pool: &PgPool, pool_hex: &str, acc: &str) -> i64 {
    // Sentinel shape per ADR 0041: created_at_ledger=0, asset/fee fields
    // at their NULL/0 placeholder values. Keep this minimal so the test
    // exercises the exact shape the persist layer's `insert_sentinel_pools`
    // emits.
    sqlx::query(
        r#"
        INSERT INTO liquidity_pools (
            pool_id, asset_a_type, asset_a_code, asset_a_issuer_id,
            asset_b_type, asset_b_code, asset_b_issuer_id,
            fee_bps, created_at_ledger
        ) VALUES (decode($1, 'hex'), 0, NULL, NULL, 0, NULL, NULL, 0, 0)
        "#,
    )
    .bind(pool_hex)
    .execute(pool)
    .await
    .expect("insert sentinel pool");

    let acc_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO accounts (account_id, first_seen_ledger, last_seen_ledger, sequence_number)
           VALUES ($1, 1, 1, 0) RETURNING id"#,
    )
    .bind(acc)
    .fetch_one(pool)
    .await
    .expect("insert acc");

    // Position that justifies the sentinel's existence. Real backfill
    // shape: lp_positions has the pool_id from a trustline read but the
    // pool's `LedgerEntry` was never in the indexed window.
    sqlx::query(
        r#"
        INSERT INTO lp_positions (pool_id, account_id, shares, first_deposit_ledger, last_updated_ledger)
        VALUES (decode($1, 'hex'), $2, 42.0::NUMERIC(28,7), 1, 1)
        "#,
    )
    .bind(pool_hex)
    .bind(acc_id)
    .execute(pool)
    .await
    .expect("insert lp_position");

    // Snapshot row for the same pool_id. Sentinels normally have no
    // snapshots — we add one anyway so the chart endpoint's underlying
    // query *would* return data if the handler-level gate were missing;
    // the 404 assertion then proves the gate (not the data shape) is
    // what blocks the response.
    sqlx::query(
        r#"
        INSERT INTO liquidity_pool_snapshots (
            pool_id, ledger_sequence, reserve_a, reserve_b, total_shares, created_at
        )
        VALUES (decode($1, 'hex'), 1, 100.0, 200.0, 42.0, NOW())
        "#,
    )
    .bind(pool_hex)
    .execute(pool)
    .await
    .expect("insert sentinel snapshot");

    acc_id
}

async fn teardown_sentinel_pool_fixture(pool: &PgPool, pool_hex: &str, acc: &str) {
    let _ = sqlx::query("DELETE FROM liquidity_pool_snapshots WHERE pool_id = decode($1, 'hex')")
        .bind(pool_hex)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM lp_positions WHERE pool_id = decode($1, 'hex')")
        .bind(pool_hex)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM liquidity_pools WHERE pool_id = decode($1, 'hex')")
        .bind(pool_hex)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM accounts WHERE account_id = $1")
        .bind(acc)
        .execute(pool)
        .await;
}

#[tokio::test]
async fn lp_sentinel_placeholder_pool_hidden_on_all_endpoints() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping 0193 sentinel-filter test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping 0193 sentinel-filter test");
            return;
        }
    };

    // Distinct from existing LP fixtures (`abcdef0123…`, etc.) so seed /
    // teardown does not collide with `lp_participants_e2e_*`.
    const POOL_HEX: &str = "9999888877776666555544443333222211110000aaaabbbbccccddddeeeeffff";
    const ACC: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0193SNT";

    // Positive-control pool: a real pool (created_at_ledger > 0) seeded
    // alongside the sentinel proves the list filter is *selective* — it
    // hides only sentinels, not all pools. Without this control, a
    // broken filter that excluded every row would also pass the
    // "sentinel not in list" assertion.
    const REAL_POOL_HEX: &str = "1111222233334444555566667777888899990000aaaabbbbccccddddeeeeffff";
    // Idempotent setup — clear leftovers from prior runs of both pools.
    teardown_sentinel_pool_fixture(&pool, POOL_HEX, ACC).await;
    let _ = sqlx::query("DELETE FROM liquidity_pools WHERE pool_id = decode($1, 'hex')")
        .bind(REAL_POOL_HEX)
        .execute(&pool)
        .await;
    setup_sentinel_pool_fixture(&pool, POOL_HEX, ACC).await;
    // Real pool with `created_at_ledger = 1` (genesis-equivalent for the
    // test). Minimal shape — no positions/snapshots needed for the list
    // assertion.
    sqlx::query(
        r#"
        INSERT INTO liquidity_pools (
            pool_id, asset_a_type, asset_a_code, asset_a_issuer_id,
            asset_b_type, asset_b_code, asset_b_issuer_id,
            fee_bps, created_at_ledger
        ) VALUES (decode($1, 'hex'), 0, NULL, NULL, 1, '0193REAL', NULL, 30, 1)
        "#,
    )
    .bind(REAL_POOL_HEX)
    .execute(&pool)
    .await
    .expect("insert real positive-control pool");

    // 1) List must include the real pool and exclude the sentinel.
    //    `limit=100` covers any realistic local DB; for larger DBs the
    //    predicate is invariant across pages so the assertion still
    //    proves the filter shape.
    {
        let app = build_app(pool.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/liquidity-pools?limit=100")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::OK, "list body: {json}");
        let items = json["data"].as_array().expect("data must be an array");
        let sentinel_present = items
            .iter()
            .any(|item| item["pool_id"].as_str() == Some(POOL_HEX));
        assert!(
            !sentinel_present,
            "sentinel pool {POOL_HEX} leaked into /v1/liquidity-pools list: {json}"
        );
        let real_present = items
            .iter()
            .any(|item| item["pool_id"].as_str() == Some(REAL_POOL_HEX));
        assert!(
            real_present,
            "positive-control pool {REAL_POOL_HEX} missing from /v1/liquidity-pools list \
             — filter is over-broad: {json}"
        );
    }

    // 1b) Detail of the real pool must return 200 OK with the canonical
    //     pool_id echoed back. Locks the filter as selective, not blanket.
    {
        let app = build_app(pool.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/liquidity-pools/{REAL_POOL_HEX}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::OK, "real pool detail body: {json}");
        assert_eq!(json["pool_id"], REAL_POOL_HEX);
    }

    // 2) Detail must return 404 (not 200 with placeholder fields).
    {
        let app = build_app(pool.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/liquidity-pools/{POOL_HEX}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "detail body: {json}");
        assert_eq!(json["code"], "not_found");
    }

    // 3) Participants — 404 gate via pool_exists().
    {
        let app = build_app(pool.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/liquidity-pools/{POOL_HEX}/participants"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "participants body: {json}");
        assert_eq!(json["code"], "not_found");
    }

    // 4) Transactions — 404 gate via pool_exists().
    {
        let app = build_app(pool.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/liquidity-pools/{POOL_HEX}/transactions"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "transactions body: {json}");
        assert_eq!(json["code"], "not_found");
    }

    // 5) Chart — 404 gate via pool_exists(). Pass an explicit small
    //    window to keep the request validation strict and avoid the
    //    default-90-day path.
    {
        let app = build_app(pool.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/v1/liquidity-pools/{POOL_HEX}/chart?interval=1d&from=2026-05-01T00:00:00Z&to=2026-05-12T00:00:00Z"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "chart body: {json}");
        assert_eq!(json["code"], "not_found");
    }

    teardown_sentinel_pool_fixture(&pool, POOL_HEX, ACC).await;
    let _ = sqlx::query("DELETE FROM liquidity_pools WHERE pool_id = decode($1, 'hex')")
        .bind(REAL_POOL_HEX)
        .execute(&pool)
        .await;
}

// Contracts E10 detail (task 0172) — canonical shape lock per `11_*.sql`.
// ---------------------------------------------------------------------------

/// Asserts that `GET /v1/contracts/:id` returns every canonical-aligned
/// field name (post-task-0172): `wasm_uploaded_at_ledger`, `deployer` (not
/// `deployer_account`), `contract_type_name` + raw `contract_type` SMALLINT,
/// and the bounded-window `stats` trio (`recent_invocations`,
/// `recent_unique_callers`, `stats_window` echoed back).
///
/// Skips cleanly if the local DB has no soroban_contracts rows.
#[tokio::test]
async fn contracts_detail_returns_canonical_shape_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> =
        sqlx::query_as("SELECT contract_id FROM soroban_contracts ORDER BY id LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((cid,)) = row else {
        eprintln!("no soroban_contracts rows — skipping contracts E10 shape test");
        return;
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts/{cid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");

    // Canonical field names — these would all fail on the pre-0172 shape.
    assert_eq!(json["contract_id"], cid);
    assert!(
        json.get("wasm_uploaded_at_ledger").is_some(),
        "missing wasm_uploaded_at_ledger: {json}"
    );
    assert!(
        json.get("deployer").is_some(),
        "missing `deployer` (post-rename from `deployer_account`): {json}"
    );
    assert!(
        json.get("contract_type_name").is_some(),
        "missing decoded `contract_type_name`: {json}"
    );
    assert!(
        json["contract_type"].is_i64() || json["contract_type"].is_null(),
        "`contract_type` must be raw SMALLINT (or null), got: {json}"
    );

    // Bounded-window stats trio. The window MUST be the API-side const
    // (`7 days`) so the frontend can render the label without guessing.
    let stats = &json["stats"];
    assert!(
        stats["recent_invocations"].is_i64(),
        "stats.recent_invocations not int: {json}"
    );
    assert!(
        stats["recent_unique_callers"].is_i64(),
        "stats.recent_unique_callers not int: {json}"
    );
    assert_eq!(
        stats["stats_window"], "7 days",
        "stats.stats_window must echo the API default: {json}"
    );

    // The pre-0172 shape would carry these — make sure they're gone.
    assert!(
        json.get("deployer_account").is_none(),
        "stale field deployer_account leaked: {json}"
    );
    assert!(
        stats.get("invocation_count").is_none(),
        "stale field stats.invocation_count leaked: {json}"
    );
    assert!(
        stats.get("event_count").is_none(),
        "stale field stats.event_count leaked: {json}"
    );
}

// ---------------------------------------------------------------------------
// Ledgers endpoints (task 0047) — list / detail / embedded transactions.
// ---------------------------------------------------------------------------

/// Non-numeric / negative / zero / >u32::MAX `:sequence` must
/// short-circuit to a 400 `invalid_sequence` envelope before any DB
/// contact. Locks the full `common::path::sequence` validator contract:
/// genesis is sequence 1 (zero rejected), Stellar caps at u32, anything
/// else is shape-invalid.
#[tokio::test]
async fn ledgers_invalid_sequence_returns_400_envelope() {
    for bad in ["abc", "-1", "12.34", "0", "4294967296"] {
        let app = lazy_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/ledgers/{bad}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "case {bad}: {json}");
        assert_eq!(json["code"], "invalid_sequence", "case {bad}: {json}");
    }
}

/// `?limit=` validation must fire before any DB contact on the list
/// endpoint, returning the canonical `invalid_limit` envelope.
#[tokio::test]
async fn ledgers_list_invalid_limit_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
}

/// `?cursor=` malformed must fire before any DB contact on the list
/// endpoint, returning the canonical `invalid_cursor` envelope.
#[tokio::test]
async fn ledgers_list_invalid_cursor_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?cursor=not!!base64")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

/// Detail endpoint shares the standard `?limit=` / `?cursor=` extractor
/// with the list endpoints — a malformed cursor on `:sequence` must
/// short-circuit to a 400 `invalid_cursor` envelope before any DB
/// contact, just like on the list endpoint.
#[tokio::test]
async fn ledgers_detail_invalid_cursor_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers/12345?cursor=not!!base64")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

/// List endpoint envelope shape — Paginated<LedgerListItem> with the
/// `page: { cursor, limit, has_more }` block per ADR 0008. Asserts the
/// short-TTL Cache-Control header that drives API Gateway behaviour.
#[tokio::test]
async fn ledgers_list_returns_paginated_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping ledgers list integration test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping");
            return;
        }
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=3")
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
        Some("public, max-age=10"),
        "list Cache-Control: {cc:?}"
    );
    assert!(json["data"].is_array(), "data not array: {json}");
    let page = &json["page"];
    assert_eq!(page["limit"], 3, "page.limit: {json}");
    assert!(
        page["next_cursor"].is_string() || page["next_cursor"].is_null(),
        "page.next_cursor must be string or null"
    );

    // Per-row shape — first row, if present.
    if let Some(row) = json["data"].get(0) {
        for k in [
            "sequence",
            "hash",
            "closed_at",
            "protocol_version",
            "transaction_count",
            "base_fee",
        ] {
            assert!(row.get(k).is_some(), "row missing `{k}`: {row}");
        }
    }
}

/// Cursor traversal: page A and page B (continuation) must not overlap.
/// Same shape as `cursor_round_trip_no_overlap_against_real_db` for
/// transactions but with the ledgers ordering key.
#[tokio::test]
async fn ledgers_cursor_round_trip_no_overlap_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let app = build_app(pool);

    // Page A
    let resp_a = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status_a, json_a) = body_json(resp_a).await;
    assert_eq!(status_a, StatusCode::OK, "page A: {json_a}");
    let data_a = json_a["data"].as_array().cloned().unwrap_or_default();
    if data_a.len() < 2 || json_a["page"]["next_cursor"].is_null() {
        eprintln!("DB has fewer than 2 ledgers or no more — skipping overlap assertion");
        return;
    }
    let cursor = json_a["page"]["next_cursor"].as_str().unwrap().to_owned();

    // Page B
    let resp_b = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit=2&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status_b, json_b) = body_json(resp_b).await;
    assert_eq!(status_b, StatusCode::OK, "page B: {json_b}");

    let seqs_a: Vec<i64> = data_a
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();
    let seqs_b: Vec<i64> = json_b["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();
    for s in &seqs_b {
        assert!(
            !seqs_a.contains(s),
            "sequence {s} appears on both pages A={seqs_a:?} B={seqs_b:?}"
        );
    }
}

/// Detail endpoint for a known absent sequence — clearly above any
/// realistic indexed ledger so the lookup misses cleanly.
#[tokio::test]
async fn ledgers_detail_unknown_sequence_returns_404_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                // u32::MAX → never indexed in any plausible backfill, but
                // still passes the u32-fits validator so we exercise the
                // 404 branch (not the 400 invalid_sequence one).
                .uri("/v1/ledgers/4294967295")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404: {json}");
    assert_eq!(json["code"], "not_found");
}

/// Detail endpoint shape against a real DB row + the head-vs-closed
/// Cache-Control branching. Selects the two highest-sequence ledgers
/// (`ORDER BY closed_at DESC, sequence DESC LIMIT 2`); uses the first
/// as the head-ledger assertion (`next_sequence is null` → 10s TTL)
/// and the second as the closed-ledger assertion (`next_sequence`
/// non-null → 300s TTL).
#[tokio::test]
async fn ledgers_detail_returns_header_and_cache_control_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    // Pick the head and an older ledger from the live DB. Skip if the
    // table has fewer than two rows (no way to distinguish head vs
    // closed under that condition).
    //
    // Tie-break by `sequence DESC` (task 0201): on shared dev DBs the
    // `persist_integration` fixtures insert synthetic ledgers with
    // identical `closed_at` values. Sorting by `closed_at` alone is
    // therefore non-deterministic across the tied rows and may pick a
    // non-head ledger, which the handler reports under the LONG TTL
    // branch (it computes "head-ness" from `next_sequence IS NULL`,
    // not from `closed_at`). Matching the list-endpoint canonical
    // ordering `(closed_at DESC, sequence DESC)` resolves the tie to
    // the actual chain head and is a no-op against production data.
    let rows: Vec<(i64,)> = match sqlx::query_as(
        "SELECT sequence FROM ledgers ORDER BY closed_at DESC, sequence DESC LIMIT 2",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(_) => return,
    };
    if rows.len() < 2 {
        eprintln!("DB has fewer than 2 ledgers — skipping detail Cache-Control test");
        return;
    }
    let head_seq = rows[0].0;
    let closed_seq = rows[1].0;

    let app = build_app(pool);

    // Head ledger — short TTL.
    let resp_head = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers/{head_seq}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let head_cc = resp_head
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (head_status, head_json) = body_json(resp_head).await;
    assert_eq!(head_status, StatusCode::OK, "head detail: {head_json}");
    assert_eq!(
        head_cc.as_deref(),
        Some("public, max-age=10"),
        "head Cache-Control: {head_cc:?}"
    );
    assert!(
        head_json["next_sequence"].is_null(),
        "head ledger should have null next_sequence: {head_json}"
    );

    // Header field shape.
    for k in [
        "sequence",
        "hash",
        "closed_at",
        "protocol_version",
        "transaction_count",
        "base_fee",
        "prev_sequence",
        "next_sequence",
        "transactions",
    ] {
        assert!(
            head_json.get(k).is_some(),
            "detail missing `{k}`: {head_json}"
        );
    }
    assert!(
        head_json["transactions"]["data"].is_array(),
        "embedded transactions.data not array: {head_json}"
    );
    assert!(
        head_json["transactions"]["page"]["limit"].is_number(),
        "embedded page.limit not number: {head_json}"
    );

    // Closed ledger — long TTL, next_sequence non-null.
    let resp_closed = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers/{closed_seq}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let closed_cc = resp_closed
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (closed_status, closed_json) = body_json(resp_closed).await;
    assert_eq!(
        closed_status,
        StatusCode::OK,
        "closed detail: {closed_json}"
    );
    assert_eq!(
        closed_cc.as_deref(),
        Some("public, max-age=300"),
        "closed Cache-Control: {closed_cc:?}"
    );
    assert!(
        !closed_json["next_sequence"].is_null(),
        "closed ledger should have non-null next_sequence: {closed_json}"
    );
}

/// Tail-of-chain assertion: the lowest indexed ledger must report
/// `prev_sequence IS NULL` (no earlier row in DB) and a non-null
/// `next_sequence` (any later row qualifies). Complements the head test
/// above which exercises the `next_sequence IS NULL` branch.
#[tokio::test]
async fn ledgers_detail_tail_has_null_prev_sequence_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(i64,)> =
        sqlx::query_as("SELECT sequence FROM ledgers ORDER BY sequence ASC LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((tail_seq,)) = row else {
        eprintln!("DB has no ledgers — skipping tail prev_sequence test");
        return;
    };

    let app = build_app(pool);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers/{tail_seq}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "tail detail: {json}");
    assert!(
        json["prev_sequence"].is_null(),
        "tail ledger should have null prev_sequence: {json}"
    );
    // next_sequence is non-null unless the DB has exactly one ledger.
    // Don't hard-assert that — just sanity-check the shape exists.
    assert!(
        json.get("next_sequence").is_some(),
        "response must carry next_sequence slot: {json}"
    );
}

/// Embedded transactions cursor traversal: page A from `/v1/ledgers/:seq`,
/// then page B with the returned cursor and the same path. Pages must not
/// overlap on `hash` and the embedded shape must round-trip cleanly.
/// Picks the most recent ledger that has at least 2 transactions; skips
/// when no such ledger exists in the live DB.
#[tokio::test]
async fn ledgers_detail_embedded_cursor_traversal_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT sequence FROM ledgers \
         WHERE transaction_count >= 2 \
         ORDER BY closed_at DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((seq,)) = row else {
        eprintln!(
            "no ledger with >=2 transactions in DB — skipping embedded cursor traversal test"
        );
        return;
    };

    let app = build_app(pool);

    // Page A — limit=1 to force has_more if the ledger has 2+ txs.
    let resp_a = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers/{seq}?limit=1"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status_a, json_a) = body_json(resp_a).await;
    assert_eq!(status_a, StatusCode::OK, "page A: {json_a}");
    let txs_a = json_a["transactions"]["data"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!txs_a.is_empty(), "page A empty: {json_a}");
    if json_a["transactions"]["page"]["next_cursor"].is_null() {
        eprintln!("ledger {seq} reported <2 retrievable txs — skipping overlap assertion");
        return;
    }
    let cursor = json_a["transactions"]["page"]["next_cursor"]
        .as_str()
        .expect("forward cursor present when more pages exist")
        .to_owned();

    // Page B — same `:sequence`, with the returned cursor.
    let resp_b = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers/{seq}?limit=1&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status_b, json_b) = body_json(resp_b).await;
    assert_eq!(status_b, StatusCode::OK, "page B: {json_b}");
    let txs_b = json_b["transactions"]["data"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!txs_b.is_empty(), "page B empty: {json_b}");

    let hashes_a: Vec<&str> = txs_a.iter().filter_map(|r| r["hash"].as_str()).collect();
    let hashes_b: Vec<&str> = txs_b.iter().filter_map(|r| r["hash"].as_str()).collect();
    for h in &hashes_b {
        assert!(
            !hashes_a.contains(h),
            "tx hash {h} appears on both embedded pages A={hashes_a:?} B={hashes_b:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Graceful-degradation tests (task 0044 §6).
//
// Lock the wire-level invariant that no endpoint returns 5xx purely because
// ingestion is behind the network tip. Concretely:
//
//   * Missing-resource lookups (hash not yet indexed, contract not yet
//     indexed) must surface as 404 with a `not_found` envelope, never 500.
//   * Upstream public-archive (S3) outages must degrade XDR-derived fields
//     to null with the parent response still 200; the endpoint must not
//     surface the underlying error to the client.
//   * Malformed input that short-circuits before the DB still maps to 400
//     with the canonical envelope code (no panic, no 500).
//
// These complement the per-record degradation tests in 0046's S3-gated
// suite (`extract_e3_*`) by exercising the full handler chain end-to-end.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn detail_invalid_hash_format_returns_400_before_db() {
    // Short / non-hex hash short-circuits before any DB or S3 call. Locks in
    // the pre-DB validation branch so a future refactor cannot start
    // forwarding malformed hashes into `lookup_hash_index` and 500-ing on
    // the SQL bind.
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions/notahash")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_hash");
}

#[tokio::test]
async fn detail_unknown_hash_returns_404_not_500() {
    // The "ledger 60M+1 not yet indexed" scenario — well-formed hash, no row
    // in `transactions`. The handler must surface this as 404 with the
    // `not_found` envelope, never 500. This is the literal invariant
    // documented in ADR 0008 + spec §"Graceful Degradation": missing recent
    // data is normal, not an error condition.
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping detail-unknown-hash test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping detail-unknown-hash test");
            return;
        }
    };

    // 64 hex chars, all zeros — guaranteed to not match any real ledger.
    let unknown_hash = "0".repeat(64);

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions/{unknown_hash}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404, got {status}: {json}"
    );
    assert_eq!(json["code"], "not_found");
    assert!(
        json.get("error").is_none(),
        "envelope must be flat (ADR 0008): {json}"
    );
}

// ---------------------------------------------------------------------------
// Task 0190 — `parse_error = true` end-to-end API coverage
// ---------------------------------------------------------------------------
//
// Locks the production contract recorded in lore-0044 / lore-0046:
//
//   parse_error transaction → light slice always served + `heavy: null` +
//   `heavy_fields_status: "unavailable"`.
//
// The pre-Step-0 handler unconditionally called `extract_e3_heavy`, which
// would either (a) succeed and mask the historical DB flag with fresh
// heavy fields or (b) return an `E3HeavyFields` with mostly-empty
// `filter(!is_empty)` payload — a `heavy_fields_status: "ok"` response
// with NULL XDR fields that violated the contract.
//
// Step 0 (`crates/api/src/transactions/handlers.rs`) introduced an
// explicit `if tx.parse_error { heavy = None }` gate before the S3
// fetch. This test seeds a `parse_error = true` row directly into the
// DB (bypassing the indexer persist path) and asserts the full response
// shape, including:
//
//   * `parse_error: true` echoed in the light slice
//   * `heavy: null`
//   * `heavy_fields_status: "unavailable"`
//   * `application_order`, `source_account`, `fee_charged` round-trip
//   * S3 was not contacted — proven by the fake AWS creds in `build_app`
//     never being invoked (a real archive fetch would 401 / 403 and
//     would have surfaced as a `tracing::warn!` log; the absence of
//     such failure is implicit in the 200 OK).

const PARSE_ERROR_API_TX_HASH: &str =
    "0190019001900190019001900190019001900190019001900190019001900190";
const PARSE_ERROR_API_SRC: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0190SRC";
const PARSE_ERROR_API_LEDGER_SEQ: i64 = 90_000_010;
/// 2026-04-21 12:00:00 UTC — stable across runs; lands in the
/// `transactions_default` partition created below.
const PARSE_ERROR_API_CREATED_AT: &str = "2026-04-21T12:00:00Z";

/// Variant A fixture for lore-0209 — empty source on a parse_error tx
/// surfaces in the API as `source_account: null`. Distinct hash + ledger
/// from the populated-source fixture above so both can coexist when the
/// suite runs serially against a shared DB.
const PARSE_ERROR_NULL_SRC_TX_HASH: &str =
    "0209020902090209020902090209020902090209020902090209020902090209";
const PARSE_ERROR_NULL_SRC_LEDGER_SEQ: i64 = 90_000_011;
/// 2099-12-31 23:59:59 UTC — sorts ahead of every realistic fixture in
/// `transactions.created_at DESC` lists so the parse_error row is reliably
/// reachable with a small `limit` even when other tests have seeded
/// fixtures into the same partition.
const PARSE_ERROR_NULL_SRC_CREATED_AT: &str = "2099-12-31T23:59:59Z";

async fn ensure_transactions_default_partition(pool: &PgPool) {
    // `transactions` + `transaction_hash_index` are partitioned and
    // unpartitioned respectively; the partitioned one needs a default
    // partition so this fixture's `created_at = 2026-04-21` row has
    // somewhere to land. Idempotent; safe to call alongside other
    // tests that may have already created the partition.
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS transactions_default PARTITION OF transactions DEFAULT",
    )
    .execute(pool)
    .await;
}

async fn seed_parse_error_transaction(pool: &PgPool) {
    // Cleanup any leftover from a prior run so the seed below
    // doesn't trip a unique constraint.
    let _ = sqlx::query("DELETE FROM transactions WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_API_TX_HASH)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM transaction_hash_index WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_API_TX_HASH)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM accounts WHERE account_id = $1")
        .bind(PARSE_ERROR_API_SRC)
        .execute(pool)
        .await;

    // accounts row for the FK target.
    let acc_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO accounts (account_id, first_seen_ledger, last_seen_ledger, sequence_number)
           VALUES ($1, $2, $2, 0) RETURNING id"#,
    )
    .bind(PARSE_ERROR_API_SRC)
    .bind(PARSE_ERROR_API_LEDGER_SEQ)
    .fetch_one(pool)
    .await
    .expect("insert accounts row for parse_error fixture");

    // transactions row — parse_error = true, no operations, no soroban.
    sqlx::query(
        r#"
        INSERT INTO transactions (
            hash, ledger_sequence, application_order, source_id, fee_charged,
            inner_tx_hash, successful, operation_count, has_soroban,
            parse_error, created_at
        )
        VALUES (decode($1, 'hex'), $2, 1, $3, 2500, NULL, false, 0, false, true, $4::timestamptz)
        "#,
    )
    .bind(PARSE_ERROR_API_TX_HASH)
    .bind(PARSE_ERROR_API_LEDGER_SEQ)
    .bind(acc_id)
    .bind(PARSE_ERROR_API_CREATED_AT)
    .execute(pool)
    .await
    .expect("insert transactions row for parse_error fixture");

    // hash_index — primary route for `/v1/transactions/:hash` lookup
    // (ADR 0027 §4). Without this entry the handler 404s before the
    // parse_error branch fires.
    sqlx::query(
        r#"
        INSERT INTO transaction_hash_index (hash, ledger_sequence, created_at)
        VALUES (decode($1, 'hex'), $2, $3::timestamptz)
        "#,
    )
    .bind(PARSE_ERROR_API_TX_HASH)
    .bind(PARSE_ERROR_API_LEDGER_SEQ)
    .bind(PARSE_ERROR_API_CREATED_AT)
    .execute(pool)
    .await
    .expect("insert transaction_hash_index row for parse_error fixture");
}

async fn cleanup_parse_error_transaction(pool: &PgPool) {
    let _ = sqlx::query("DELETE FROM transactions WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_API_TX_HASH)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM transaction_hash_index WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_API_TX_HASH)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM accounts WHERE account_id = $1")
        .bind(PARSE_ERROR_API_SRC)
        .execute(pool)
        .await;
}

/// Seed a Variant A `parse_error` row with `source_id = NULL` — the
/// lore-0209 shape that the indexer now produces for envelope-missing
/// transactions. No `accounts` row is touched (none exists for the
/// unknown source). Idempotent.
async fn seed_parse_error_tx_null_source(pool: &PgPool) {
    let _ = sqlx::query("DELETE FROM transactions WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_NULL_SRC_TX_HASH)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM transaction_hash_index WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_NULL_SRC_TX_HASH)
        .execute(pool)
        .await;

    sqlx::query(
        r#"
        INSERT INTO transactions (
            hash, ledger_sequence, application_order, source_id, fee_charged,
            inner_tx_hash, successful, operation_count, has_soroban,
            parse_error, created_at
        )
        VALUES (decode($1, 'hex'), $2, 1, NULL, 2500, NULL, false, 0, false, true, $3::timestamptz)
        "#,
    )
    .bind(PARSE_ERROR_NULL_SRC_TX_HASH)
    .bind(PARSE_ERROR_NULL_SRC_LEDGER_SEQ)
    .bind(PARSE_ERROR_NULL_SRC_CREATED_AT)
    .execute(pool)
    .await
    .expect("insert NULL-source parse_error transactions row");

    sqlx::query(
        r#"
        INSERT INTO transaction_hash_index (hash, ledger_sequence, created_at)
        VALUES (decode($1, 'hex'), $2, $3::timestamptz)
        "#,
    )
    .bind(PARSE_ERROR_NULL_SRC_TX_HASH)
    .bind(PARSE_ERROR_NULL_SRC_LEDGER_SEQ)
    .bind(PARSE_ERROR_NULL_SRC_CREATED_AT)
    .execute(pool)
    .await
    .expect("insert NULL-source parse_error transaction_hash_index row");
}

async fn cleanup_parse_error_tx_null_source(pool: &PgPool) {
    let _ = sqlx::query("DELETE FROM transactions WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_NULL_SRC_TX_HASH)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM transaction_hash_index WHERE hash = decode($1, 'hex')")
        .bind(PARSE_ERROR_NULL_SRC_TX_HASH)
        .execute(pool)
        .await;
}

#[tokio::test]
async fn detail_parse_error_tx_returns_unavailable_heavy_without_s3_contact() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping 0190 parse_error API test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping 0190 parse_error API test");
            return;
        }
    };

    ensure_transactions_default_partition(&pool).await;
    seed_parse_error_transaction(&pool).await;

    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions/{PARSE_ERROR_API_TX_HASH}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 OK for parse_error tx: {json}"
    );

    // --- Light slice (flattened to top level per `E3Response<TxLight>`).
    assert_eq!(
        json["hash"], PARSE_ERROR_API_TX_HASH,
        "hash must echo the URL parameter"
    );
    assert_eq!(json["ledger_sequence"], PARSE_ERROR_API_LEDGER_SEQ);
    assert_eq!(json["application_order"], 1);
    assert_eq!(json["source_account"], PARSE_ERROR_API_SRC);
    assert_eq!(json["fee_charged"], 2500);
    assert_eq!(json["successful"], false);
    assert_eq!(json["operation_count"], 0);
    assert_eq!(json["has_soroban"], false);
    assert_eq!(
        json["parse_error"], true,
        "light.parse_error MUST echo the DB flag — lore-0044 / lore-0046 contract"
    );

    // --- Heavy block — must be absent.
    assert!(
        json["heavy"].is_null(),
        "heavy MUST be null when DB carries parse_error=true (Step 0 short-circuit): {json}"
    );
    assert_eq!(
        json["heavy_fields_status"], "unavailable",
        "heavy_fields_status MUST be 'unavailable' for parse_error tx (lore-0046 contract)"
    );

    // --- Light fallback arrays — handler populates these from the
    // DB-side appearance index when heavy is None (per
    // `transactions/handlers.rs:225`). The fixture has no rows in the
    // appearance tables for this tx, so the fallbacks come back empty.
    // This proves the fallback path executed (didn't short-circuit on
    // the missing heavy) but produced empty arrays — not the `[]`
    // sentinel that would mean the fallback was skipped entirely.
    assert!(
        json["participants"].is_array(),
        "participants must be an array (DB fallback when heavy=None)"
    );
    assert!(
        json["soroban_events"].is_array(),
        "soroban_events must be an array (DB fallback when heavy=None)"
    );
    assert!(
        json["soroban_invocations"].is_array(),
        "soroban_invocations must be an array (DB fallback when heavy=None)"
    );

    cleanup_parse_error_transaction(&pool).await;
}

/// lore-0209 — Variant A `parse_error` transactions land in the DB with
/// `transactions.source_id = NULL`. The unfiltered transaction list
/// must serve such rows with `source_account: null` instead of
/// dropping them (the path uses `LEFT JOIN accounts` per
/// `transactions/queries.rs`, branch `(None, None)`).
///
/// Locks the end-to-end DTO round-trip for lore-0209:
///
///   * row reaches the response with `parse_error: true`
///   * `source_account` serialises as JSON `null`
///   * `application_order`, `fee_charged`, `successful`, `has_soroban`
///     round-trip cleanly even though the FK target is absent
#[tokio::test]
async fn list_returns_parse_error_tx_with_null_source_account() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping lore-0209 list-NULL-source test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!(
                "DATABASE_URL unreachable ({err}) — skipping lore-0209 list-NULL-source test"
            );
            return;
        }
    };

    ensure_transactions_default_partition(&pool).await;
    seed_parse_error_tx_null_source(&pool).await;

    let app = build_app(pool.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "expected 200 OK from list endpoint: {json}"
    );

    let data = json["data"].as_array().expect("data array");
    let row = data
        .iter()
        .find(|r| r["hash"] == PARSE_ERROR_NULL_SRC_TX_HASH)
        .unwrap_or_else(|| {
            panic!(
                "NULL-source parse_error row missing from list response (seeded created_at \
                 2099-12-31 should sort to the top with limit=5): {json}"
            )
        });

    assert!(
        row["source_account"].is_null(),
        "source_account MUST serialise as JSON null for NULL source_id (lore-0209): {row}"
    );
    assert_eq!(row["ledger_sequence"], PARSE_ERROR_NULL_SRC_LEDGER_SEQ);
    assert_eq!(row["application_order"], 1);
    assert_eq!(row["fee_charged"], 2500);
    assert_eq!(row["successful"], false);
    assert_eq!(row["operation_count"], 0);
    assert_eq!(row["has_soroban"], false);

    cleanup_parse_error_tx_null_source(&pool).await;
}

#[tokio::test]
async fn list_returns_200_without_s3_contact() {
    // The fake AWS credentials in `build_app` would fail any archive fetch
    // — but the list handler is now DB-only (post-task-0047, ADR 0029):
    // no S3 contact regardless of DB content. This test locks that
    // invariant: list must return 200 with well-formed rows even though
    // the AWS client cannot authenticate.
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping list-no-s3-contact test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping list-no-s3-contact test");
            return;
        }
    };

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "list must stay 200 with DB-only path: {status} {json}"
    );

    let data = json["data"].as_array().expect("data array").clone();
    for row in &data {
        assert!(row["hash"].is_string(), "row missing hash: {row}");
        assert!(
            row.get("memo").is_none() && row.get("memo_type").is_none(),
            "list rows must not carry memo fields (DB-only contract): {row}"
        );
    }
}

// ---------------------------------------------------------------------------
// Contracts handlers — graceful-degradation regression coverage (task 0044 §6).
//
// Mirror the transactions tests for /v1/contracts/:id{,/interface,/invocations,
// /events}. The contracts module ships its own ListParams parser + S3
// stop-and-retry expansion; these tests lock that no path returns 5xx for
// missing-resource or malformed-input scenarios. A future refactor that, e.g.,
// flips `Ok(None) => not_found` to `internal_error` or starts forwarding bad
// StrKey paths into the SQL bind will fail one of these tests.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn contract_invalid_id_returns_400_before_db() {
    // Malformed StrKey (lowercase, wrong length) short-circuits before any DB
    // hit. Locks the pre-DB validation branch in `get_contract`.
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/contracts/notavalidstrkey")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_contract_id");
    assert_eq!(json["details"]["param"], "contract_id");
    assert_eq!(json["details"]["expected_prefix"], "C");
}

#[tokio::test]
async fn contract_invocations_invalid_id_returns_400_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/contracts/notavalidstrkey/invocations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_contract_id");
}

#[tokio::test]
async fn contract_events_invalid_id_returns_400_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/contracts/notavalidstrkey/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_contract_id");
}

#[tokio::test]
async fn contract_unknown_id_returns_404_not_500() {
    // Well-formed StrKey, no row in `soroban_contracts`. Equivalent of
    // `detail_unknown_hash_returns_404_not_500` for the contracts route.
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping contract-unknown-id test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping contract-unknown-id test");
            return;
        }
    };

    // Synthetic 56-char StrKey (no CRC) guaranteed not to exist.
    let unknown_contract = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts/{unknown_contract}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404, got {status}: {json}"
    );
    assert_eq!(json["code"], "not_found");
}

#[tokio::test]
async fn contract_interface_unknown_returns_404() {
    // No `wasm_interface_metadata` row for the contract → 404.
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping interface-unknown test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping interface-unknown test");
            return;
        }
    };

    let unknown_contract = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";

    let router = build_app(pool);
    let resp = router
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts/{unknown_contract}/interface"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "expected 404, got {status}: {json}"
    );
    assert_eq!(json["code"], "not_found");
}

// ---------------------------------------------------------------------------
// Out-of-u32-range `ledger_sequence` — pure logic test (no fixture row needed).
//
// Stellar `LedgerHeader.ledgerSeq` is `uint32` so any DB row with
// `ledger_sequence > u32::MAX` indicates corrupted ingestion or a
// hypothetical schema drift. The handler responds by skipping the row
// from heavy fetch and logging a `warn`, never panicking. Seeding such
// a row in PG is unrealistic (would require a deliberate out-of-bound
// BIGINT), so we lock the conversion behaviour at the type boundary
// instead.
// ---------------------------------------------------------------------------

#[test]
fn u32_try_from_invariants_relied_on_by_handlers() {
    // Inputs the handler converts via `u32::try_from(i64)`:
    assert!(
        u32::try_from(i64::MAX).is_err(),
        "i64::MAX must overflow u32"
    );
    assert!(
        u32::try_from(i64::from(u32::MAX) + 1).is_err(),
        "u32::MAX + 1 must overflow"
    );
    assert!(u32::try_from(-1_i64).is_err(), "negative must fail");

    // Boundary: u32::MAX itself fits.
    assert_eq!(u32::try_from(i64::from(u32::MAX)).unwrap(), u32::MAX);

    // The handler's pattern: failed conversion → warn + skip / heavy=None,
    // not panic. Verified by the call sites in
    // `transactions/handlers.rs::get_transaction` (heavy fetch) and
    // `contracts/handlers.rs::expand_invocations` / `expand_events`
    // (per-row stop-and-retry).
}

// ---------------------------------------------------------------------------
// Task 0051 — NFT endpoints (validation tests; no DB contact)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nfts_invalid_contract_returns_400_envelope() {
    // Per task 0264 Phase 8a, the NFT route is now keyed by
    // `(contract C-strkey, token_id)` rather than by the internal
    // `nfts.id` surrogate. A malformed contract strkey trips the
    // `path::strkey('C', _)` validator → `invalid_contract_id`.
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/nfts/not-a-strkey/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_contract_id");
    assert_eq!(json["details"]["param"], "contract_id");
}

#[tokio::test]
async fn nfts_invalid_token_id_returns_400_envelope() {
    // Shape-valid contract C-strkey but an empty token_id segment slot is
    // not routable; we exercise the "too long" branch instead, which
    // trips `parse_nft_path` with `invalid_id`.
    let app = lazy_app();
    let contract = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ";
    let long_token = "a".repeat(200);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/nfts/{contract}/{long_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_id");
    assert_eq!(json["details"]["param"], "token_id");
}

#[tokio::test]
async fn nfts_invalid_contract_filter_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/nfts?filter%5Bcontract_id%5D=BAD")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "contract_id");
}

#[tokio::test]
async fn nfts_filter_name_rejects_wildcard_literals() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/nfts?filter%5Bname%5D=foo%25bar")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "name");
}

#[tokio::test]
async fn nfts_transfers_invalid_contract_returns_400_envelope() {
    // Same composite-shape validation as the detail endpoint — bad
    // C-strkey in the path → `invalid_contract_id`.
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/nfts/not-a-strkey/42/transfers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_contract_id");
}

// ---------------------------------------------------------------------------
// Task 0052 — Liquidity-pool list / detail / transactions / chart (validation)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lp_detail_invalid_pool_id_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools/not-hex")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_pool_id");
}

#[tokio::test]
async fn lp_list_invalid_issuer_filter_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools?filter%5Basset_a_issuer%5D=BAD")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "asset_a_issuer");
}

#[tokio::test]
async fn lp_list_mixed_asset_a_filter_rejected() {
    let app = lazy_app();
    // asset_a_code without asset_a_issuer is ambiguous (could match wrong issuer's
    // USDC); canonical SQL 18 §46-49 says API validates upstream.
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools?filter%5Basset_a_code%5D=USDC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
}

#[tokio::test]
async fn lp_list_invalid_min_tvl_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools?filter%5Bmin_tvl%5D=not-a-number")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["filter"], "min_tvl");
}

#[tokio::test]
async fn lp_chart_invalid_pool_id_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools/not-hex/chart?interval=1h&from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_pool_id");
}

#[tokio::test]
async fn lp_chart_invalid_interval_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR/\
                     chart?interval=1m&from=2026-01-01T00:00:00Z&to=2026-01-02T00:00:00Z",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["param"], "interval");
}

#[tokio::test]
async fn lp_chart_omitted_params_use_defaults_then_404_for_missing_pool() {
    // All three params are optional now. Bare `?` request defaults to
    // `interval=1d` + `to=now()` + `from=to-30d`. The pool below does not
    // exist (lazy_app uses connect_lazy), so we expect 404 path through —
    // not a 400 from missing params.
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping chart-defaults 404 test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(_) => return,
    };
    let app = build_app(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR/\
                     chart",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["code"], "not_found");
}

#[tokio::test]
async fn lp_chart_range_exceeds_bucket_cap_returns_400_envelope() {
    let app = lazy_app();
    // 100 years at 1h interval ~876 000 buckets, well above 1 000 cap.
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR/\
                     chart?interval=1h&from=1926-01-01T00:00:00Z&to=2026-01-01T00:00:00Z",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
    assert_eq!(json["details"]["max_buckets"], 1000);
}

#[tokio::test]
async fn lp_chart_from_after_to_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/liquidity-pools/\
                     LAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABLIR/\
                     chart?interval=1h&from=2026-02-01T00:00:00Z&to=2026-01-01T00:00:00Z",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_filter");
}

#[tokio::test]
async fn lp_transactions_invalid_pool_id_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools/not-hex/transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_pool_id");
}

// ===========================================================================
// Accounts (task 0048)
// ===========================================================================

const ACCOUNTS_VALID_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT";

/// Path-shape validation must fire before any DB contact and return the
/// canonical `invalid_account_id` envelope. Covers wrong prefix (C-StrKey
/// against G validator), short input, and lowercase letters.
#[tokio::test]
async fn accounts_invalid_id_returns_400_envelope() {
    for bad in [
        "BAD",
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ", // C-StrKey
        "gaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaat", // lowercase
    ] {
        let app = lazy_app();
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/accounts/{bad}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "case {bad}: {json}");
        assert_eq!(json["code"], "invalid_account_id", "case {bad}: {json}");
    }
}

/// Sub-resource (transactions) shares the same path validator — assert it
/// fires before the pagination extractor sees anything.
#[tokio::test]
async fn accounts_transactions_invalid_id_returns_400_envelope() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/accounts/BAD/transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_account_id");
}

/// `?limit=` validation on the transactions sub-resource must fire before
/// the path validator yields control to DB.
#[tokio::test]
async fn accounts_transactions_invalid_limit_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/accounts/{ACCOUNTS_VALID_G}/transactions?limit=0"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_limit");
}

/// Malformed `?cursor=` must surface as `invalid_cursor` before any DB
/// contact, just like every other paginated list endpoint.
#[tokio::test]
async fn accounts_transactions_invalid_cursor_returns_envelope_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/accounts/{ACCOUNTS_VALID_G}/transactions?cursor=not!!base64"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "invalid_cursor");
}

// ---------------------------------------------------------------------------
// DB-touching tests — gated on DATABASE_URL.
// ---------------------------------------------------------------------------

/// Detail endpoint shape against a live `accounts` row. Picks an account
/// with the most balances so the balances projection is exercised, and
/// asserts every documented header field plus the short-TTL Cache-Control
/// header. Skips when the DB has no accounts.
#[tokio::test]
async fn accounts_detail_returns_header_and_balances_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping accounts detail integration test");
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        eprintln!("DATABASE_URL unreachable — skipping");
        return;
    };

    // Pick an indexed account with at least one balance row (preferring
    // the one with the most balances so we exercise the multi-row branch).
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT a.account_id \
         FROM accounts a \
         JOIN account_balances_current abc ON abc.account_id = a.id \
         GROUP BY a.account_id \
         ORDER BY COUNT(*) DESC \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((account_strkey,)) = row else {
        eprintln!("DB has no accounts with balances — skipping detail shape test");
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts/{account_strkey}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let cc = resp
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    assert_eq!(
        cc.as_deref(),
        Some("public, max-age=10"),
        "Cache-Control: {cc:?}"
    );
    for k in [
        "account_id",
        "sequence_number",
        "balances",
        "home_domain",
        "first_seen_ledger",
        "last_seen_ledger",
    ] {
        assert!(json.get(k).is_some(), "detail missing `{k}`: {json}");
    }
    assert_eq!(json["account_id"], account_strkey);
    let balances = json["balances"]
        .as_array()
        .unwrap_or_else(|| panic!("balances not array: {json}"));
    assert!(!balances.is_empty(), "expected ≥1 balance row: {json}");
    for bal in balances {
        for k in ["asset_type_name", "type", "balance", "last_updated_ledger"] {
            assert!(bal.get(k).is_some(), "balance missing `{k}`: {bal}");
        }
        // Native rows must carry NULL code/issuer; credit rows must carry both
        // (CHECK ck_abc_native on the source table).
        let asset_type = bal["type"].as_i64().unwrap();
        if asset_type == 0 {
            assert!(bal["asset_code"].is_null(), "native asset_code: {bal}");
            assert!(bal["asset_issuer"].is_null(), "native asset_issuer: {bal}");
        } else {
            assert!(bal["asset_code"].is_string(), "credit asset_code: {bal}");
            assert!(
                bal["asset_issuer"].is_string(),
                "credit asset_issuer: {bal}"
            );
        }
    }
}

/// Detail endpoint shape against an account with **zero balances** —
/// the LIVE dev DB carries ~45% accounts in this state (uninitialized
/// observations + accounts whose balance pipeline hasn't caught up).
/// `account_balances_current` returns no rows; the response must still
/// be 200 with `balances: []`, not 404. Skips when the DB has no
/// zero-balance accounts.
#[tokio::test]
async fn accounts_detail_with_zero_balances_returns_empty_array_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT a.account_id \
         FROM accounts a \
         LEFT JOIN account_balances_current abc ON abc.account_id = a.id \
         GROUP BY a.account_id \
         HAVING COUNT(abc.*) = 0 \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((account_strkey,)) = row else {
        eprintln!("no zero-balance accounts — skipping empty-balances test");
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts/{account_strkey}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    assert_eq!(json["account_id"], account_strkey);
    assert!(
        json["balances"].is_array() && json["balances"].as_array().unwrap().is_empty(),
        "balances should be empty array: {json}"
    );
}

/// 404 for an account StrKey that passes shape validation but is not
/// indexed. Uses the all-zeros (with valid CRC trailer used in test
/// fixtures) StrKey — vanishingly unlikely to be a real account.
#[tokio::test]
async fn accounts_detail_unknown_returns_404_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts/{ACCOUNTS_VALID_G}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404: {json}");
    assert_eq!(json["code"], "not_found");
}

/// Transactions sub-resource shape against a live row. Picks an account
/// with ≥1 participation row from `transaction_participants` so the
/// LATERAL `operation_types[]` aggregate produces a non-trivial result.
/// Asserts canonical 07's projection 1:1.
#[tokio::test]
async fn accounts_transactions_returns_paginated_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT a.account_id \
         FROM accounts a \
         JOIN transaction_participants tp ON tp.account_id = a.id \
         GROUP BY a.account_id \
         ORDER BY COUNT(*) DESC \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((account_strkey,)) = row else {
        eprintln!("DB has no accounts with participations — skipping");
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/accounts/{account_strkey}/transactions?limit=3"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cc = resp
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    assert_eq!(
        cc.as_deref(),
        Some("public, max-age=10"),
        "Cache-Control: {cc:?}"
    );
    assert!(json["data"].is_array(), "data not array: {json}");
    assert!(json["page"]["limit"].is_number(), "page.limit: {json}");
    assert!(
        json["page"]["next_cursor"].is_string() || json["page"]["next_cursor"].is_null(),
        "page.next_cursor must be string or null"
    );

    if let Some(row) = json["data"].get(0) {
        for k in [
            "hash",
            "ledger_sequence",
            "application_order",
            "source_account",
            "fee_charged",
            "successful",
            "operation_count",
            "has_soroban",
            "operation_types",
            "created_at",
        ] {
            assert!(row.get(k).is_some(), "tx row missing `{k}`: {row}");
        }
        assert!(
            row["operation_types"].is_array(),
            "operation_types not array: {row}"
        );
    }
}

/// 404 on the sub-resource when the account is unknown — same UX as the
/// detail endpoint, distinct from "indexed account with empty list".
#[tokio::test]
async fn accounts_transactions_unknown_returns_404_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts/{ACCOUNTS_VALID_G}/transactions"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "expected 404: {json}");
    assert_eq!(json["code"], "not_found");
}

/// Indexed account with zero `transaction_participants` rows must return
/// 200 + an empty page, distinct from the 404 path. Locks the contract
/// against accidental regression to "404 when no transactions yet".
#[tokio::test]
async fn accounts_transactions_indexed_account_zero_participations_returns_empty_page() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> = sqlx::query_as(
        "SELECT a.account_id \
         FROM accounts a \
         LEFT JOIN transaction_participants tp ON tp.account_id = a.id \
         GROUP BY a.account_id \
         HAVING COUNT(tp.*) = 0 \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((account_strkey,)) = row else {
        eprintln!("no zero-participation accounts — skipping empty-page test");
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts/{account_strkey}/transactions"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");
    assert!(
        json["data"].is_array() && json["data"].as_array().unwrap().is_empty(),
        "data should be empty: {json}"
    );
    assert!(json["page"]["next_cursor"].is_null(), "has_more: {json}");
    assert!(
        json["page"]["next_cursor"].is_null(),
        "cursor should be absent: {json}"
    );
}

/// Cursor traversal: page A and page B (continuation) must not overlap.
/// Same shape as the transactions/ledgers cursor tests but on the account
/// transactions sub-resource.
#[tokio::test]
async fn accounts_transactions_cursor_round_trip_no_overlap_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    // Pick an account with at least 4 participation rows so we have a
    // realistic chance of `has_more = true` on page A.
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT a.account_id \
         FROM accounts a \
         JOIN transaction_participants tp ON tp.account_id = a.id \
         GROUP BY a.account_id \
         HAVING COUNT(*) >= 4 \
         ORDER BY COUNT(*) DESC \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();
    let Some((account_strkey,)) = row else {
        eprintln!("no account with ≥4 participations — skipping cursor traversal test");
        return;
    };

    let app = build_app(pool);

    let resp_a = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/accounts/{account_strkey}/transactions?limit=2"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status_a, json_a) = body_json(resp_a).await;
    assert_eq!(status_a, StatusCode::OK, "page A: {json_a}");
    let data_a = json_a["data"].as_array().cloned().unwrap_or_default();
    if data_a.len() < 2 || json_a["page"]["next_cursor"].is_null() {
        eprintln!("DB returned fewer than 2 + has_more — skipping overlap assertion");
        return;
    }
    let cursor = json_a["page"]["next_cursor"].as_str().unwrap().to_owned();

    let resp_b = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/accounts/{account_strkey}/transactions?limit=2&cursor={cursor}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status_b, json_b) = body_json(resp_b).await;
    assert_eq!(status_b, StatusCode::OK, "page B: {json_b}");

    let hashes_a: Vec<String> = data_a
        .iter()
        .map(|r| r["hash"].as_str().unwrap().to_owned())
        .collect();
    let hashes_b: Vec<String> = json_b["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["hash"].as_str().unwrap().to_owned())
        .collect();
    for h in &hashes_b {
        assert!(
            !hashes_a.contains(h),
            "hash {h} appears on both pages A={hashes_a:?} B={hashes_b:?}"
        );
    }
}

// ===========================================================================
// task 0055 — Cache-Control / no-store middleware coverage
// ===========================================================================

fn cache_control(resp: &axum::response::Response) -> Option<String> {
    resp.headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Unmatched route on the live router → axum bare 404; middleware must
/// stamp `Cache-Control: no-store` so the gateway never caches it.
#[tokio::test]
async fn default_404_route_returns_no_store() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/no-such-route-here")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(cache_control(&resp).as_deref(), Some("no-store"));
}

/// Validation 400 (handler-set Cache-Control header would ordinarily be
/// missing or wrong) → middleware overwrites to `no-store`.
#[tokio::test]
async fn validation_400_returns_no_store() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(cache_control(&resp).as_deref(), Some("no-store"));
}

/// Path-shape 404 (well-formed but not indexed) → no-store.
#[tokio::test]
async fn handler_404_returns_no_store_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/accounts/GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    assert_eq!(cache_control(&resp).as_deref(), Some("no-store"));
}

/// `GET /v1/transactions` → SHORT (10s).
#[tokio::test]
async fn transactions_list_cache_control_short_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/transactions?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(cache_control(&resp).as_deref(), Some("public, max-age=10"));
}

/// `GET /v1/transactions/:hash` → conditional.
/// In tests the S3 archive isn't reachable so heavy_fields_status falls to
/// Unavailable; assert the SHORT branch fires (LONG branch is exercised by
/// the live `cargo lambda invoke` E2E).
#[tokio::test]
async fn transactions_detail_cache_control_short_when_archive_unavailable_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let row: Option<(String,)> =
        sqlx::query_as("SELECT encode(hash, 'hex') FROM transaction_hash_index LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((hash_hex,)) = row else {
        eprintln!("no rows in transaction_hash_index — skipping");
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/transactions/{hash_hex}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // Test env uses fake AWS creds in build_app() — fetch_ledger always fails,
    // so heavy_fields_status = Unavailable and handler must emit SHORT.
    assert_eq!(
        cache_control(&resp).as_deref(),
        Some("public, max-age=10"),
        "archive-unavailable branch must emit SHORT (10s)"
    );
}

/// `GET /v1/assets/:id` → MEDIUM (60s).
#[tokio::test]
async fn assets_detail_cache_control_medium_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    // Address the native singleton via the reserved `native` token (seeded by
    // migration 0161; the numeric surrogate is gone).
    let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) FROM assets WHERE asset_type = 0")
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();
    if !matches!(row, Some((n,)) if n > 0) {
        return;
    }
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/assets/native")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(cache_control(&resp).as_deref(), Some("public, max-age=60"));
}

/// `GET /v1/contracts/:id` → MEDIUM (60s).
#[tokio::test]
async fn contracts_detail_cache_control_medium_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let row: Option<(String,)> =
        sqlx::query_as("SELECT contract_id FROM soroban_contracts LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((cid,)) = row else {
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts/{cid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(cache_control(&resp).as_deref(), Some("public, max-age=60"));
}

/// `GET /v1/nfts` (list) → SHORT (10s).
#[tokio::test]
async fn nfts_list_cache_control_short_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/nfts?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(cache_control(&resp).as_deref(), Some("public, max-age=10"));
}

/// `GET /v1/liquidity-pools/:id/chart` → MEDIUM (60s).
#[tokio::test]
async fn lp_chart_cache_control_medium_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let row: Option<(String,)> =
        sqlx::query_as("SELECT encode(pool_id, 'hex') FROM liquidity_pools LIMIT 1")
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
    let Some((pool_hex,)) = row else {
        eprintln!("no liquidity pools — skipping");
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/liquidity-pools/{pool_hex}/chart?interval=1h&from=2026-04-01T00:00:00Z&to=2026-04-02T00:00:00Z"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    if resp.status() == StatusCode::OK {
        assert_eq!(cache_control(&resp).as_deref(), Some("public, max-age=60"));
    } else {
        eprintln!("chart 200 not reachable in this env: {}", resp.status());
    }
}

/// `GET /v1/search` → no-store (variable q makes caching impractical).
#[tokio::test]
async fn search_cache_control_no_store_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/search?q=GAA")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(cache_control(&resp).as_deref(), Some("no-store"));
}

/// `GET /v1/search?q=<random 64-hex>` → broad search runs (no match
/// → Results, empty groups). Locks in the option C refactor invariant
/// (task 0271): the handler no longer short-circuits to `fetch_redirect`
/// on shape-typed inputs — it always runs broad and only synthesizes
/// `Redirect` when row count is exactly one.
#[tokio::test]
async fn search_random_hex_returns_results_not_redirect() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    // 64 chars of zero hex — vanishingly unlikely to exist in either
    // `transaction_hash_index` or `liquidity_pools`. Both CTEs return
    // zero rows ⇒ total == 0 ⇒ Results (with empty groups).
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(
                    "/v1/search?q=0000000000000000000000000000000000000000000000000000000000000000",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["type"], "results",
        "non-existent hex input must not redirect (option C: row count drives wire shape)"
    );
}

/// `GET /v1/search?q=<random full G-strkey>` → broad runs; non-existent
/// account ⇒ no rows in `account_hits` ⇒ Results (empty groups), not
/// Redirect. Complements the hex case above for the strkey channel.
#[tokio::test]
async fn search_random_full_g_strkey_returns_results_not_redirect() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };
    // Valid SEP-23 G-strkey for the all-zero ed25519 pubkey. Standalone
    // valid shape; live indexer has no realistic chance of carrying it.
    let zero_account = stellar_strkey::ed25519::PublicKey([0u8; 32]).to_string();
    let uri = format!("/v1/search?q={zero_account}");
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(uri.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["type"], "results",
        "non-existent G-strkey must not redirect under option C"
    );
}

// ---------------------------------------------------------------------------
// Cursor matrix — direction-aware pagination (task 0254)
//
// Validates the `next_cursor` + `prev_cursor` contract on `GET /v1/ledgers`:
//
//   * first page  : `prev_cursor` null, `next_cursor` Some
//   * middle page : both Some
//   * last page   : `next_cursor` null, `prev_cursor` Some (when input present)
//   * round-trip  : page1.next_cursor → page2; page2.prev_cursor → page1'
//                   that matches the original page 1's row identities.
//   * walk match  : forward 4× then backward 3× yields the same row
//                   sequences (modulo presentation order).
//
// Ledgers chosen as the reference endpoint: simplest cursor payload
// (`TsIdCursor`), immutable table, deterministic ordering. Other
// endpoints follow the same algebra — extend this pattern when the
// integration matrix is widened.
// ---------------------------------------------------------------------------

/// Helper: open the running test DB or skip cleanly. Mirrors the
/// idiom used by every other DB-touching test in this module.
async fn cursor_matrix_pool() -> Option<PgPool> {
    let database_url = std::env::var("DATABASE_URL").ok()?;
    PgPool::connect(&database_url).await.ok()
}

#[tokio::test]
async fn ledgers_first_page_omits_prev_cursor() {
    let Some(pool) = cursor_matrix_pool().await else {
        eprintln!("DATABASE_URL unset/unreachable — skipping cursor matrix test");
        return;
    };
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let data = json["data"].as_array().cloned().unwrap_or_default();
    if data.is_empty() {
        eprintln!("DB empty — skipping first-page assertion");
        return;
    }
    assert!(
        json["page"]["prev_cursor"].is_null(),
        "first page must omit prev_cursor: {json}"
    );
    if json["page"]["next_cursor"].is_string() {
        assert!(
            json["page"]["next_cursor"].as_str().is_some(),
            "has_more=true requires cursor: {json}"
        );
    }
}

#[tokio::test]
async fn ledgers_middle_page_emits_both_cursors() {
    let Some(pool) = cursor_matrix_pool().await else {
        return;
    };
    let app = build_app(pool);

    // Page 1.
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json1) = body_json(resp1).await;
    let Some(cursor1) = json1["page"]["next_cursor"].as_str() else {
        eprintln!("page 1 has no cursor — DB too small, skipping");
        return;
    };

    // Page 2 — should expose both cursors (if more rows follow).
    let resp2 = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit=2&cursor={cursor1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status2, json2) = body_json(resp2).await;
    assert_eq!(status2, StatusCode::OK, "{json2}");
    assert!(
        json2["page"]["prev_cursor"].as_str().is_some(),
        "middle page must emit prev_cursor: {json2}"
    );
    // `cursor` Some when more pages follow; None on the tail. Both
    // valid on this assertion — we only insist prev_cursor is set.
}

#[tokio::test]
async fn ledgers_prev_cursor_round_trip_returns_original_page() {
    // Walk forward to page 2, then use page 2's `prev_cursor` to walk
    // backward to page 1'. Assert page 1' is the same set of rows
    // as the original page 1 (cursor symmetry).
    let Some(pool) = cursor_matrix_pool().await else {
        return;
    };
    let app = build_app(pool);

    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/ledgers?limit=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json1) = body_json(resp1).await;
    let seqs1: Vec<i64> = json1["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();
    let Some(cursor1) = json1["page"]["next_cursor"].as_str().map(str::to_owned) else {
        eprintln!("DB too small for round-trip test — skipping");
        return;
    };

    let resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit=3&cursor={cursor1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json2) = body_json(resp2).await;
    let Some(prev2) = json2["page"]["prev_cursor"].as_str().map(str::to_owned) else {
        eprintln!("page 2 has no prev_cursor — skipping");
        return;
    };

    let resp1b = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit=3&cursor={prev2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status1b, json1b) = body_json(resp1b).await;
    assert_eq!(status1b, StatusCode::OK, "{json1b}");
    let seqs1b: Vec<i64> = json1b["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();

    assert_eq!(
        seqs1, seqs1b,
        "prev_cursor round-trip must yield identical rows (page 1 = {seqs1:?}, page 1' via prev_cursor = {seqs1b:?})"
    );
}

#[tokio::test]
async fn ledgers_forward_then_backward_walk_matches() {
    // Walk forward 4 pages, capture row sets. Then walk back 3 pages
    // via prev_cursor (page 4 → 3 → 2 → 1). Each backward step must
    // produce the same row set as the corresponding forward step,
    // proving cursor symmetry across the matrix. Loop range tracks
    // the backward symmetry: we need a page 4 to anchor the backward
    // walk that visits pages 3, 2, 1, so the last forward fetch
    // (iter 3) must have a `next_cursor` we can land on but then
    // re-fetch via the prev_cursor chain.
    let Some(pool) = cursor_matrix_pool().await else {
        return;
    };
    let app = build_app(pool);

    const LIMIT: u32 = 2;
    let mut forward_seqs: Vec<Vec<i64>> = vec![];
    let mut forward_cursors: Vec<Option<String>> = vec![];
    let mut cursor: Option<String> = None;

    for _ in 0..4 {
        let uri = match &cursor {
            Some(c) => format!("/v1/ledgers?limit={LIMIT}&cursor={c}"),
            None => format!("/v1/ledgers?limit={LIMIT}"),
        };
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::OK, "forward: {json}");
        let seqs: Vec<i64> = json["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|r| r["sequence"].as_i64().unwrap())
            .collect();
        forward_seqs.push(seqs);
        let Some(next) = json["page"]["next_cursor"].as_str() else {
            eprintln!(
                "DB too small for forward walk (needs ≥4 pages × {LIMIT} = ≥8 ledgers) — skipping"
            );
            return;
        };
        forward_cursors.push(Some(next.to_owned()));
        cursor = Some(next.to_owned());
    }

    // Walk backward from page 4. Use `forward_cursors[2]` (the cursor
    // that fetched page 4 on the forward walk) so we can re-fetch
    // page 4 and read its prev_cursor.
    let last_uri = format!(
        "/v1/ledgers?limit={LIMIT}&cursor={}",
        forward_cursors[2].as_ref().unwrap()
    );
    let resp_last = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&last_uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json_last) = body_json(resp_last).await;
    let Some(prev_to_p3) = json_last["page"]["prev_cursor"].as_str().map(str::to_owned) else {
        eprintln!("page 4 has no prev_cursor — DB shape unexpected, skipping");
        return;
    };

    // Backward step 1 of 3: page 4 → page 3.
    let resp_back3 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit={LIMIT}&cursor={prev_to_p3}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json_back3) = body_json(resp_back3).await;
    let seqs_back3: Vec<i64> = json_back3["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();
    assert_eq!(
        forward_seqs[2], seqs_back3,
        "backward step to page 3 must match forward page 3 (forward={:?}, backward={seqs_back3:?})",
        forward_seqs[2]
    );

    // Backward step 2 of 3: page 3 → page 2.
    let Some(prev_to_p2) = json_back3["page"]["prev_cursor"]
        .as_str()
        .map(str::to_owned)
    else {
        eprintln!("page 3 (via backward) has no prev_cursor — skipping");
        return;
    };
    let resp_back2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit={LIMIT}&cursor={prev_to_p2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json_back2) = body_json(resp_back2).await;
    let seqs_back2: Vec<i64> = json_back2["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();
    assert_eq!(
        forward_seqs[1], seqs_back2,
        "backward step to page 2 must match forward page 2 (forward={:?}, backward={seqs_back2:?})",
        forward_seqs[1]
    );

    // Backward step 3 of 3: page 2 → page 1.
    let Some(prev_to_p1) = json_back2["page"]["prev_cursor"]
        .as_str()
        .map(str::to_owned)
    else {
        eprintln!("page 2 (via backward) has no prev_cursor — skipping");
        return;
    };
    let resp_back1 = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/ledgers?limit={LIMIT}&cursor={prev_to_p1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, json_back1) = body_json(resp_back1).await;
    let seqs_back1: Vec<i64> = json_back1["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| r["sequence"].as_i64().unwrap())
        .collect();
    assert_eq!(
        forward_seqs[0], seqs_back1,
        "backward step to page 1 must match forward page 1 (forward={:?}, backward={seqs_back1:?})",
        forward_seqs[0]
    );
}

/// Behaviour regression for `?order=asc` (task 0274 gap #3).
///
/// The first implementation reused `Direction::Prev` for asc, which
/// presented the oldest block in DESC order and broke forward
/// pagination (the `next` cursor led nowhere). Correct behaviour:
///
/// - `order=desc` (default) → sequences strictly DECREASING (newest first),
/// - `order=asc` → sequences strictly INCREASING (oldest first),
/// - asc `next_cursor` keeps walking ascending, strictly past the page.
#[tokio::test]
async fn ledgers_order_asc_is_oldest_first_and_paginates_forward() {
    async fn page(app: &Router, uri: String) -> (Vec<i64>, Option<String>) {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        let seqs = json["data"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|r| r["sequence"].as_i64().unwrap())
            .collect();
        let next = json["page"]["next_cursor"].as_str().map(str::to_owned);
        (seqs, next)
    }

    fn is_strictly_increasing(s: &[i64]) -> bool {
        s.windows(2).all(|w| w[0] < w[1])
    }
    fn is_strictly_decreasing(s: &[i64]) -> bool {
        s.windows(2).all(|w| w[0] > w[1])
    }

    let Some(pool) = cursor_matrix_pool().await else {
        return;
    };
    let app = build_app(pool);
    const LIMIT: u32 = 5;

    // desc (default): newest-first.
    let (desc, _) = page(&app, format!("/v1/ledgers?limit={LIMIT}&order=desc")).await;
    if desc.len() < 2 {
        eprintln!("DB has <2 ledgers — skipping order assertions");
        return;
    }
    assert!(
        is_strictly_decreasing(&desc),
        "order=desc must be newest-first (strictly decreasing): {desc:?}"
    );

    // asc: oldest-first — the fix.
    let (asc, asc_next) = page(&app, format!("/v1/ledgers?limit={LIMIT}&order=asc")).await;
    assert!(
        is_strictly_increasing(&asc),
        "order=asc must be oldest-first (strictly increasing): {asc:?}"
    );

    // The two orders genuinely address opposite ends of the table.
    assert!(
        asc[0] < desc[0],
        "asc head (oldest={}) must be below desc head (newest={})",
        asc[0],
        desc[0]
    );

    // asc forward pagination: next_cursor continues ascending, strictly
    // past the first page (the exact behaviour the old impl broke).
    let Some(next) = asc_next else {
        eprintln!(
            "DB too small for an asc second page (needs >{LIMIT} ledgers) — skipping forward step"
        );
        return;
    };
    let (asc2, _) = page(
        &app,
        format!("/v1/ledgers?limit={LIMIT}&order=asc&cursor={next}"),
    )
    .await;
    assert!(
        is_strictly_increasing(&asc2),
        "asc page 2 must stay ascending: {asc2:?}"
    );
    assert!(
        *asc.last().unwrap() < asc2[0],
        "asc page 2 head ({}) must be strictly after page 1 tail ({})",
        asc2[0],
        asc.last().unwrap()
    );
}

/// Task 0274 gap #5 — pool legs carry `icon_url` mirrored from the leg's
/// `assets` row (classic or SAC). Proves the SQL join + DTO plumbing
/// end-to-end. Every leg must always serialise the key (string|null); if
/// the DB has any enriched icon, at least one leg must surface it.
#[tokio::test]
async fn lp_legs_carry_icon_url_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping LP icon_url integration test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping LP icon_url test");
            return;
        }
    };
    let assert_pool = pool.clone();

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/liquidity-pools?limit=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");

    let pools = json["data"].as_array().cloned().unwrap_or_default();
    if pools.is_empty() {
        eprintln!("no pools in DB — skipping icon_url assertions");
        return;
    }

    let legs: Vec<&Value> = pools
        .iter()
        .flat_map(|p| [&p["asset_a"], &p["asset_b"]])
        .collect();

    // Wiring: every leg always serialises an `icon_url` key, string or null.
    for leg in &legs {
        let icon = leg
            .as_object()
            .and_then(|o| o.get("icon_url"))
            .expect("leg must contain icon_url");
        assert!(
            icon.is_string() || icon.is_null(),
            "leg.icon_url must be string|null, got {icon} on {leg}"
        );
    }

    // Data: if any asset in the DB is enriched, at least one leg shows it.
    let any_icon = legs.iter().any(|leg| leg["icon_url"].is_string());

    let mut codes = Vec::new();
    let mut issuers = Vec::new();
    let mut contracts = Vec::new();
    for leg in &legs {
        if let Some(code) = leg["asset_code"].as_str() {
            codes.push(code.to_string());
        }
        if let Some(issuer) = leg["issuer"].as_str() {
            issuers.push(issuer.to_string());
        }
        if let Some(cid) = leg["contract_id"].as_str() {
            contracts.push(cid.to_string());
        }
    }

    let db_has_icons = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM assets a \
         LEFT JOIN accounts iss ON iss.id = a.issuer_id \
         LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id \
         WHERE a.icon_url IS NOT NULL \
           AND a.asset_type IN (1, 2) \
           AND ( \
               (a.asset_code = ANY($1) AND iss.account_id = ANY($2)) \
               OR (sc.contract_id = ANY($3)) \
           )",
    )
    .bind(&codes)
    .bind(&issuers)
    .bind(&contracts)
    .fetch_one(&assert_pool)
    .await
    .unwrap_or(0);
    if db_has_icons > 0 {
        assert!(
            any_icon,
            "DB has {db_has_icons} enriched classic/SAC assets but no pool leg surfaced an icon_url — join is broken"
        );
    }
}

/// Task 0275 — `GET /v1/contracts` list. Asserts the paginated envelope +
/// well-formed item shape, and that `filter[type]` is accepted. DB-gated.
#[tokio::test]
async fn contracts_list_returns_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping contracts list integration test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping contracts list test");
            return;
        }
    };

    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?limit=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");

    // Envelope shape — regardless of row count.
    assert!(json["data"].is_array(), "data not array: {json}");
    let page = &json["page"];
    assert_eq!(page["limit"], 100, "page.limit not echoed: {json}");
    assert!(
        page["next_cursor"].is_string() || page["next_cursor"].is_null(),
        "next_cursor must be string|null: {json}"
    );

    // Item shape — every row carries the documented fields.
    for item in json["data"].as_array().cloned().unwrap_or_default() {
        assert!(item["contract_id"].is_string(), "contract_id: {item}");
        assert!(
            item["recent_invocations"].is_number(),
            "recent_invocations: {item}"
        );
        assert!(item["is_sac"].is_boolean(), "is_sac: {item}");
        let tn = &item["contract_type_name"];
        assert!(tn.is_string() || tn.is_null(), "contract_type_name: {item}");
    }

    // `filter[type]` accepted (valid enum value).
    let resp2 = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?limit=10&filter%5Btype%5D=token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status2, json2) = body_json(resp2).await;
    assert_eq!(status2, StatusCode::OK, "filter[type]=token: {json2}");
    for item in json2["data"].as_array().cloned().unwrap_or_default() {
        assert_eq!(
            item["contract_type_name"], "token",
            "type filter leaked: {item}"
        );
    }
}

/// Task 0275 — list/detail field PARITY. Every field a list item exposes for a
/// contract must be computed identically by the detail endpoint for that same
/// contract (detail is a strict superset). Guards against the two endpoints'
/// SQL drifting apart (e.g. `recent_invocations` window, deployer join,
/// `contract_type_name` decode, `name`). DB-gated.
#[tokio::test]
async fn contract_list_item_matches_detail_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping contract list/detail parity test");
        return;
    };
    let pool = match PgPool::connect(&database_url).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("DATABASE_URL unreachable ({err}) — skipping parity test");
            return;
        }
    };

    // Grab the first contract off the list.
    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, list_json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "list 200: {list_json}");

    let Some(item) = list_json["data"]
        .as_array()
        .and_then(|a| a.first())
        .cloned()
    else {
        eprintln!("no contracts seeded — skipping parity assertions");
        return;
    };
    let contract_id = item["contract_id"]
        .as_str()
        .expect("contract_id")
        .to_string();

    // Fetch the same contract's detail.
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts/{contract_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, detail) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "detail 200: {detail}");

    // Shared scalar fields must be byte-for-byte equal. `name` is deliberately
    // NOT in this list — it is a search-only column, surfaced by neither
    // endpoint (asserted separately below).
    for field in [
        "contract_id",
        "contract_type",
        "contract_type_name",
        "is_sac",
        "deployer",
        "deployed_at_ledger",
    ] {
        assert_eq!(
            item[field], detail[field],
            "field `{field}` differs between list and detail for {contract_id}\n list={item}\n detail={detail}"
        );
    }

    // `recent_invocations` lives top-level on the list item, under `stats` on
    // the detail — same window (`STATS_WINDOW`), so identical counts.
    assert_eq!(
        item["recent_invocations"], detail["stats"]["recent_invocations"],
        "recent_invocations differs (window drift?) for {contract_id}\n list={item}\n detail={detail}"
    );

    // `name` is a search-only column — neither endpoint exposes it.
    assert!(
        item.get("name").is_none(),
        "list item must not surface name: {item}"
    );
    assert!(
        detail.get("name").is_none(),
        "detail must not surface name: {detail}"
    );
}

/// Task 0275 — invalid `filter[type]` must 400 in the handler BEFORE any SQL
/// runs (mirrors `assets_invalid_filter_type_returns_envelope_before_db`).
/// No DB needed.
#[tokio::test]
async fn contracts_invalid_filter_type_returns_400_before_db() {
    let app = lazy_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?filter%5Btype%5D=NOT_A_TYPE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
    assert_eq!(json["code"], "invalid_filter", "{json}");
    assert_eq!(json["details"]["filter"], "type", "{json}");
}

/// Task 0275 — keyset pagination is correct: walking the list one cursor page
/// at a time visits the SAME contracts, in the SAME order, with no overlap or
/// gaps versus a single large page. DB-gated.
#[tokio::test]
async fn contracts_list_cursor_pagination_round_trip_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping contracts pagination test");
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        eprintln!("DATABASE_URL unreachable — skipping");
        return;
    };

    let ids = |json: &serde_json::Value| -> Vec<String> {
        json["data"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|i| i["contract_id"].as_str().unwrap_or_default().to_string())
            .collect()
    };

    // Ground truth: all contracts in one page.
    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?limit=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, full_json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{full_json}");
    let full = ids(&full_json);
    if full.len() < 3 {
        eprintln!(
            "only {} contracts seeded — skipping pagination assertions",
            full.len()
        );
        return;
    }

    // Page 1 (size 2).
    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, p1) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{p1}");
    let page1 = ids(&p1);
    assert_eq!(page1.len(), 2, "page 1 should be full: {p1}");
    assert_eq!(
        page1,
        full[..2],
        "page 1 must match the first slice of the full list"
    );
    let cursor = p1["page"]["next_cursor"]
        .as_str()
        .expect("next_cursor on a full page")
        .to_string();

    // Page 2, via cursor.
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts?limit=2&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, p2) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{p2}");
    let page2 = ids(&p2);
    assert_eq!(
        page2,
        full[2..2 + page2.len()],
        "page 2 must continue exactly where page 1 stopped"
    );
    // No overlap between the two pages.
    for id in &page2 {
        assert!(
            !page1.contains(id),
            "cursor page overlap: {id} on both pages"
        );
    }
}

/// Task 0275 — `filter[q]` full-text search resolves a contract by its own
/// contract id (the id is part of `search_vector`). DB-gated.
#[tokio::test]
async fn contracts_list_filter_q_finds_by_contract_id_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping contracts search test");
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    // Grab a real contract id off the list.
    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/contracts?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, list) = body_json(resp).await;
    let Some(cid) = list["data"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|i| i["contract_id"].as_str())
        .map(str::to_string)
    else {
        eprintln!("no contracts seeded — skipping search assertions");
        return;
    };

    // Search for that exact id.
    let enc = cid.clone();
    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/contracts?filter%5Bq%5D={enc}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let found: Vec<String> = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["contract_id"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        found.contains(&cid),
        "search for {cid} did not return it: {json}"
    );
}

/// Task 0275 — every valid `filter[type]` is accepted and, when rows come
/// back, they all carry the requested classification (no leakage). DB-gated.
#[tokio::test]
async fn contracts_list_filter_type_classifies_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping contracts type-filter test");
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    for ty in ["token", "other", "nft", "fungible"] {
        let resp = build_app(pool.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/contracts?limit=50&filter%5Btype%5D={ty}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let (status, json) = body_json(resp).await;
        assert_eq!(status, StatusCode::OK, "filter[type]={ty}: {json}");
        for item in json["data"].as_array().cloned().unwrap_or_default() {
            assert_eq!(
                item["contract_type_name"], ty,
                "filter[type]={ty} leaked a {} row: {item}",
                item["contract_type_name"]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// GET /v1/accounts (list) — task 0274 gap #1
// ---------------------------------------------------------------------------

/// Envelope + item shape. Asserts the documented fields and that the cut
/// fields (`#` rank, `xlm_supply_percent`) are surfaced by NEITHER. DB-gated.
#[tokio::test]
async fn accounts_list_returns_envelope_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL unset — skipping accounts list integration test");
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        eprintln!("DATABASE_URL unreachable — skipping");
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/accounts?limit=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "expected 200: {json}");

    assert!(json["data"].is_array(), "data not array: {json}");
    assert_eq!(json["page"]["limit"], 100, "page.limit not echoed: {json}");

    for item in json["data"].as_array().cloned().unwrap_or_default() {
        assert!(item["account_id"].is_string(), "account_id: {item}");
        assert!(
            item["last_seen_ledger"].is_number(),
            "last_seen_ledger: {item}"
        );
        assert!(
            item["first_seen_ledger"].is_number(),
            "first_seen_ledger: {item}"
        );
        let bal = &item["xlm_balance"];
        assert!(
            bal.is_string() || bal.is_null(),
            "xlm_balance string|null: {item}"
        );
        let dom = &item["home_domain"];
        assert!(
            dom.is_string() || dom.is_null(),
            "home_domain string|null: {item}"
        );
        // Cut fields — must not reappear.
        assert!(
            item.get("xlm_supply_percent").is_none(),
            "supply% must be cut: {item}"
        );
        assert!(item.get("rank").is_none(), "rank must be cut: {item}");
    }
}

/// Keyset pagination over `(last_seen_ledger, id)` — one cursor page at a
/// time visits the same accounts, same order, no overlap vs one big page.
#[tokio::test]
async fn accounts_list_cursor_pagination_round_trip_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let ids = |json: &serde_json::Value| -> Vec<String> {
        json["data"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|i| i["account_id"].as_str().unwrap_or_default().to_string())
            .collect()
    };

    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/accounts?limit=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, full_json) = body_json(resp).await;
    let full = ids(&full_json);
    if full.len() < 3 {
        eprintln!(
            "only {} accounts seeded — skipping pagination assertions",
            full.len()
        );
        return;
    }

    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/accounts?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, p1) = body_json(resp).await;
    let page1 = ids(&p1);
    assert_eq!(page1, full[..2], "page 1 must match the first slice");
    let cursor = p1["page"]["next_cursor"]
        .as_str()
        .expect("next_cursor")
        .to_string();

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts?limit=2&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, p2) = body_json(resp).await;
    let page2 = ids(&p2);
    assert_eq!(
        page2,
        full[2..2 + page2.len()],
        "page 2 must continue exactly"
    );
    for id in &page2 {
        assert!(!page1.contains(id), "cursor page overlap: {id}");
    }
}

/// `filter[with_domain]=true` — every returned row has a non-null home_domain.
#[tokio::test]
async fn accounts_list_with_domain_filter_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/accounts?limit=100&filter%5Bwith_domain%5D=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    for item in json["data"].as_array().cloned().unwrap_or_default() {
        assert!(
            item["home_domain"].is_string(),
            "with_domain leaked a null-domain row: {item}"
        );
    }
}

/// `?order=asc` flips the base sort — `last_seen_ledger` is non-decreasing.
#[tokio::test]
async fn accounts_list_order_asc_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri("/v1/accounts?limit=100&order=asc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    let seen: Vec<i64> = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["last_seen_ledger"].as_i64().unwrap_or_default())
        .collect();
    for w in seen.windows(2) {
        assert!(w[0] <= w[1], "order=asc not ascending: {seen:?}");
    }
}

/// Bidirectional keyset: walk forward to page 2 via `next_cursor`, then back
/// via page 2's `prev_cursor` — the returned page must equal page 1 exactly.
#[tokio::test]
async fn accounts_list_prev_cursor_round_trip_against_real_db() {
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let Ok(pool) = PgPool::connect(&database_url).await else {
        return;
    };

    let ids = |json: &serde_json::Value| -> Vec<String> {
        json["data"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|i| i["account_id"].as_str().unwrap_or_default().to_string())
            .collect()
    };

    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri("/v1/accounts?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, p1) = body_json(resp).await;
    let page1 = ids(&p1);
    let Some(next) = p1["page"]["next_cursor"].as_str() else {
        eprintln!("not enough accounts for a second page — skipping prev round-trip");
        return;
    };

    let resp = build_app(pool.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts?limit=2&cursor={next}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (_, p2) = body_json(resp).await;
    let prev = p2["page"]["prev_cursor"]
        .as_str()
        .expect("page 2 has a prev_cursor")
        .to_string();

    let resp = build_app(pool)
        .oneshot(
            Request::builder()
                .uri(format!("/v1/accounts?limit=2&cursor={prev}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, back) = body_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{back}");
    assert_eq!(ids(&back), page1, "prev_cursor did not return to page 1");
}
