//! Axum handler for `GET /v1/network/stats`.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse, Response};

use crate::common::cache_control;
use crate::common::datasource::{DataSource, Module};
use crate::common::errors;
use crate::openapi::schemas::ErrorEnvelope;
use crate::state::AppState;

use super::dto::NetworkStats;
use super::{queries, queries_ch};

/// Unified per-call fetch error so the moka cache initializer can dispatch
/// between the PG and CH backends without leaking driver types up the
/// call stack. Only the `Display` impl is observed (forwarded to the
/// canonical `db_error` envelope + tracing); the variant is for
/// diagnostics on the log side.
#[derive(Debug, thiserror::Error)]
enum FetchStatsError {
    #[error("pg: {0}")]
    Pg(#[from] sqlx::Error),
    #[error("ch: {0}")]
    Ch(#[from] clickhouse::error::Error),
}

/// Get top-level chain overview stats.
///
/// Reads the canonical single-statement network-stats query (latest
/// ledger row + `ledgers` 60s aggregate for TPS + `pg_class.reltuples`
/// estimates for accounts / contracts) and caches the assembled
/// response for 30s in process memory. See the task 0045 spec and
/// `docs/architecture/database-schema/endpoint-queries/01_get_network_stats.sql`
/// for the full data-source mapping.
///
/// Concurrent cold-cache requests deduplicate via
/// `moka::future::Cache::try_get_with` — the first task runs the
/// async DB query and the rest wait on its result instead of fanning
/// out N Postgres round-trips.
#[utoipa::path(
    get,
    path = "/network/stats",
    tag = "network",
    responses(
        (status = 200, description = "Chain overview stats", body = NetworkStats),
        (status = 500, description = "Database error",       body = ErrorEnvelope),
    ),
)]
pub async fn get_network_stats(State(state): State<AppState>) -> Response {
    let source = DataSource::for_module(Module::Network);
    // `try_get_with` deduplicates concurrent cold-cache requests: only
    // the first task runs the DB query, every other concurrent task on
    // the same key waits for that task's result. Errors are propagated
    // as `Arc<FetchStatsError>` so a single failed fetch is not cached
    // and the next request retries cleanly.
    let result: Result<Arc<NetworkStats>, Arc<FetchStatsError>> = state
        .network_cache
        .try_get_with((), async {
            match source {
                DataSource::Pg => queries::fetch_stats(&state.db)
                    .await
                    .map(Arc::new)
                    .map_err(FetchStatsError::from),
                DataSource::Ch => queries_ch::fetch_stats(state.ch())
                    .await
                    .map(Arc::new)
                    .map_err(FetchStatsError::from),
            }
        })
        .await;

    match result {
        Ok(stats) => ok_response(stats),
        Err(e) => {
            tracing::error!(source = ?source, "DB error in get_network_stats: {e}");
            errors::internal_error(errors::DB_ERROR, "Unable to retrieve network statistics.")
        }
    }
}

fn ok_response(stats: Arc<NetworkStats>) -> Response {
    let mut resp = Json(stats).into_response();
    cache_control::attach(&mut resp, cache_control::SHORT);
    resp
}

#[cfg(test)]
mod tests {
    //! End-to-end shape check for `/v1/network/stats`.
    //!
    //! Mirrors the `DATABASE_URL`-gated pattern used by
    //! `crates/indexer/tests/persist_integration.rs` — runs only when
    //! the env var is set and reachable, skips cleanly otherwise so
    //! `cargo test` is green on a workstation without the compose
    //! stack up.
    //!
    //!   docker compose up -d
    //!   npm run db:migrate
    //!   DATABASE_URL=postgres://postgres:postgres@localhost:5432/soroban_block_explorer \
    //!       cargo test -p api --bin api network -- --test-threads=1
    use axum::Router;
    use axum::body::{self, Body};
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use sqlx::PgPool;
    use tower::ServiceExt;
    use utoipa_axum::router::OpenApiRouter;

    use crate::network;
    use crate::runtime_enrichment::RuntimeEnrichment;
    use crate::runtime_enrichment::sep1::Sep1Fetcher;
    use crate::runtime_enrichment::stellar_archive::StellarArchiveFetcher;
    use crate::state::AppState;

    fn app(db: PgPool) -> Router {
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
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL unset — skipping network stats integration test");
            return;
        };
        let pool = match PgPool::connect(&database_url).await {
            Ok(p) => p,
            Err(err) => {
                eprintln!("DATABASE_URL unreachable ({err}) — skipping network stats test");
                return;
            }
        };

        let resp = app(pool)
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
            Some("public, max-age=10"),
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
}
