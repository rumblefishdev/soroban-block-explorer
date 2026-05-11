//! Sink abstraction over the two write-path targets.
//!
//! Task 0205 / ADR 0044: the same backfill orchestrator now feeds both
//! Postgres (production) and ClickHouse (pilot). The narrow PG surface
//! (preflight + load_completed + persist) collapses into three enum-
//! dispatched methods here; the rest of the runner sees `&Sink` and is
//! target-agnostic.
//!
//! The ClickHouse `persist_ledger` arm is **stubbed** — it parses the
//! ledger and calls `db_clickhouse::persist::persist_ledger_clickhouse`,
//! which logs and returns `Ok` without issuing any INSERT. Real CH
//! writes land in a follow-up task.

use std::collections::HashSet;

use clickhouse::Client as ClickhouseClient;
use indexer::handler::persist::ClassificationCache;
use serde::Deserialize;
use sqlx::PgPool;
use stellar_xdr::curr::LedgerCloseMeta;
use tracing::{info, warn};

use crate::error::BackfillError;
use crate::resume;

/// Write-path target wired up at startup. Variants own the connection
/// handle for the chosen store. `PgPool` and `clickhouse::Client` are
/// both `Arc`-backed, but the runner only holds one and passes `&Sink`
/// down — no clones needed.
///
/// `large_enum_variant`: `ClickhouseClient` is ~300 bytes, the `PgPool`
/// handle is 8 bytes. Boxing the larger variant would add a heap
/// indirection on every `match` for no gain — exactly one `Sink` exists
/// per process and it's never stored in a collection.
#[allow(clippy::large_enum_variant)]
pub enum Sink {
    Postgres(PgPool),
    Clickhouse(ClickhouseClient),
}

/// Row shape for the resume / status query against ClickHouse. Private
/// to this module — callers see `HashSet<u32>`.
#[derive(Debug, Deserialize, clickhouse::Row)]
struct LedgerSeqRow {
    sequence: i64,
}

impl Sink {
    /// Confirm the store is reachable. Both arms run `SELECT 1`.
    ///
    /// Same posture as the previous PG-only `preflight_db`: a failure
    /// here is a config / environment error, so the caller panics on
    /// it. We still return `Result` so the panic site stays in
    /// `run.rs` and not in this module.
    pub async fn preflight(&self) -> Result<(), BackfillError> {
        match self {
            Sink::Postgres(pool) => {
                sqlx::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(pool)
                    .await?;
            }
            Sink::Clickhouse(client) => {
                let _ = client.query("SELECT 1").fetch_one::<u8>().await?;
            }
        }
        Ok(())
    }

    /// Load sequences already present in the `ledgers` table within
    /// `[start, end]`. Resume + status both feed off this single batch
    /// query.
    pub async fn load_completed(
        &self,
        start: u32,
        end: u32,
    ) -> Result<HashSet<u32>, BackfillError> {
        let set = match self {
            // PG path stays in `resume::load_completed` — keeps the
            // existing tests load-bearing and concentrates PG SQL in
            // one module.
            Sink::Postgres(pool) => resume::load_completed(pool, start, end).await?,
            Sink::Clickhouse(client) => {
                let rows: Vec<i64> = client
                    .query("SELECT sequence FROM ledgers WHERE sequence BETWEEN ? AND ?")
                    .bind(i64::from(start))
                    .bind(i64::from(end))
                    .fetch_all::<LedgerSeqRow>()
                    .await?
                    .into_iter()
                    .map(|r| r.sequence)
                    .collect();
                // `sequence` is i64 in the CH schema (matches PG bigint) but
                // ledger sequences are u32-bounded by Stellar protocol. The
                // SQL `BETWEEN start AND end` already constrains the range,
                // but defend against bogus / manually-inserted rows by
                // using `try_from` and warning on anything that doesn't
                // fit. A silent `as u32` would wrap negatives / overflows.
                let set: HashSet<u32> = rows
                    .into_iter()
                    .filter_map(|s| match u32::try_from(s) {
                        Ok(v) => Some(v),
                        Err(_) => {
                            warn!(
                                value = s,
                                "skipping out-of-range sequence from clickhouse ledgers"
                            );
                            None
                        }
                    })
                    .collect();
                info!(
                    start,
                    end,
                    completed = set.len(),
                    total = u64::from(end) - u64::from(start) + 1,
                    target = "clickhouse",
                    "resume state loaded"
                );
                set
            }
        };
        Ok(set)
    }

    /// Parse + persist a single ledger.
    ///
    /// - **Postgres**: delegates to `process_ledger` (unchanged
    ///   behaviour, classification cache threaded through).
    /// - **Clickhouse**: calls `parse_ledger` then the stub
    ///   `persist_ledger_clickhouse`. `classification_cache` is ignored
    ///   on this path (PG-specific NFT filter helper, task 0118 Phase 2).
    ///
    /// Returning `()` (not a timings struct) keeps the PG path byte-for-
    /// byte equivalent: the caller's outer `Instant::now()` measurement
    /// in `ingest.rs` is the same wall-clock value it has always been.
    pub async fn persist_ledger(
        &self,
        meta: &LedgerCloseMeta,
        classification_cache: &ClassificationCache,
    ) -> Result<(), BackfillError> {
        match self {
            Sink::Postgres(pool) => {
                indexer::handler::process::process_ledger(meta, pool, None, classification_cache)
                    .await?;
            }
            Sink::Clickhouse(client) => {
                let parsed = indexer::handler::process::parse_ledger(meta);
                db_clickhouse::persist::persist_ledger_clickhouse(
                    client,
                    &parsed.ledger,
                    &parsed.transactions,
                    &parsed.operations,
                    &parsed.events,
                    &parsed.invocations,
                    &parsed.contract_interfaces,
                    &parsed.contract_deployments,
                    &parsed.account_states,
                    &parsed.liquidity_pools,
                    &parsed.pool_snapshots,
                    &parsed.assets,
                    &parsed.nfts,
                    &parsed.nft_events,
                    &parsed.lp_positions,
                    &parsed.contract_name_writes,
                )
                .await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! ClickHouse-flavored Sink tests. Gated on `CLICKHOUSE_URL` so
    //! `cargo test -p backfill-runner` stays green in CI without a
    //! ClickHouse instance — mirrors the gating posture used by
    //! `db-clickhouse/tests/smoke.rs` and the PG tests in `resume.rs`.
    //!
    //! Run locally (against the `docker-compose up clickhouse` instance):
    //!
    //! ```bash
    //! CLICKHOUSE_URL=http://localhost:8123 \
    //!     cargo test -p backfill-runner --lib sink -- --test-threads=1
    //! ```
    use super::*;
    use db_clickhouse::{Config, apply_init_sql};

    /// Far above any realistic Soroban sequence — fits in u32 and i64,
    /// keeps fixtures out of the way of any real data on a shared CH.
    const TEST_BASE: u32 = 4_000_000_000;

    async fn build_sink() -> Option<Sink> {
        let url = std::env::var("CLICKHOUSE_URL").ok()?;
        let cfg = Config {
            url,
            ..Config::from_env()
        };
        let client = db_clickhouse::client(&cfg);
        // Make sure the schema is in place before the test runs the queries.
        // `apply_init_sql` is idempotent so calling it on every test run is
        // cheap and survives a fresh container.
        if let Err(err) = apply_init_sql(&client).await {
            eprintln!("CLICKHOUSE_URL set but apply_init_sql failed ({err}) — skipping");
            return None;
        }
        Some(Sink::Clickhouse(client))
    }

    async fn ch_cleanup(client: &ClickhouseClient, start: u32, end: u32) {
        let _ = client
            .query("ALTER TABLE ledgers DELETE WHERE sequence BETWEEN ? AND ?")
            .bind(i64::from(start))
            .bind(i64::from(end))
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }

    async fn ch_insert_ledger(client: &ClickhouseClient, seq: u32) {
        client
            .query(
                "INSERT INTO ledgers (sequence, hash, closed_at, protocol_version, transaction_count, base_fee) \
                 VALUES (?, unhex(?), now64(3), 22, 0, 100)",
            )
            .bind(i64::from(seq))
            // 32-byte hash unique per seq.
            .bind(format!("{:062x}{:02x}", 0u32, (seq & 0xff) as u8))
            .execute()
            .await
            .expect("insert fixture ledger");
    }

    #[tokio::test]
    async fn clickhouse_preflight_succeeds() {
        let Some(sink) = build_sink().await else {
            eprintln!("CLICKHOUSE_URL not set — skipping");
            return;
        };
        sink.preflight()
            .await
            .expect("CH preflight (SELECT 1) must succeed");
    }

    #[tokio::test]
    async fn clickhouse_load_completed_returns_only_in_range() {
        let Some(sink) = build_sink().await else {
            eprintln!("CLICKHOUSE_URL not set — skipping");
            return;
        };
        let Sink::Clickhouse(ref client) = sink else {
            unreachable!()
        };

        let below = TEST_BASE + 100;
        let a = TEST_BASE + 110;
        let b = TEST_BASE + 111;
        let c = TEST_BASE + 115;
        let above = TEST_BASE + 120;

        ch_cleanup(client, below, above).await;
        for seq in [below, a, b, c, above] {
            ch_insert_ledger(client, seq).await;
        }

        let got = sink.load_completed(a, c).await.expect("load_completed");
        let expected: HashSet<u32> = [a, b, c].into_iter().collect();
        assert_eq!(got, expected);

        ch_cleanup(client, below, above).await;
    }
}
