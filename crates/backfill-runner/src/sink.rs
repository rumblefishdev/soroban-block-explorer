//! Sink abstraction over the two write-path targets.
//!
//! Task 0205 / ADR 0044 collapsed the PG-only write surface into a
//! target-agnostic `Sink`. Task 0206 (this revision) gains a
//! **partition-writer lifecycle**: `open_partition` → `write_ledger × N`
//! → `commit` / `abort`. The PG variant maps these onto today's
//! per-ledger `process_ledger` (open + commit are no-ops; each
//! `write_ledger` is its own DB transaction, fast commits intact).
//! The CH variant drives a [`db_clickhouse::persist::PartitionWriter`] across
//! the partition so the 14 server-side INSERTs only open once and end
//! once — see that module's docs for why per-ledger inserts are wrong
//! for CH (parts explosion, merger overwhelmed).
//!
//! The legacy [`Sink::persist_ledger`] method is kept as a thin
//! wrapper (`open` → `write` → `commit`) for tests and any caller
//! that wants per-ledger semantics. Production backfill goes through
//! the lifecycle on `ingest.rs::index_partition`.

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

    /// Open a partition writer handle. The CH variant constructs a
    /// long-lived [`db_clickhouse::persist::PartitionWriter`] that holds one
    /// `Insert<RowT>` per table across the partition. The PG variant
    /// is a no-op constructor that just borrows the pool; per-ledger
    /// transaction semantics are intact (no batching across ledgers).
    pub fn open_partition(&self) -> PartitionWriterHandle<'_> {
        match self {
            Sink::Postgres(pool) => PartitionWriterHandle::Postgres { pool },
            Sink::Clickhouse(client) => PartitionWriterHandle::Clickhouse(
                db_clickhouse::persist::PartitionWriter::open(client.clone()),
            ),
        }
    }

    /// Parse + persist a single ledger.
    ///
    /// Kept for legacy / per-ledger callers (tests and any direct
    /// invocation that doesn't want to drive the lifecycle). Wraps
    /// `open_partition` → `write_ledger` → `commit` so the per-ledger
    /// semantics PG had before task 0206 are preserved byte-for-byte.
    ///
    /// Production backfill (`ingest.rs::index_partition`) drives the
    /// lifecycle explicitly across the whole partition — never calls
    /// this method.
    #[allow(dead_code)]
    pub async fn persist_ledger(
        &self,
        meta: &LedgerCloseMeta,
        classification_cache: &ClassificationCache,
    ) -> Result<(), BackfillError> {
        let mut handle = self.open_partition();
        if let Err(err) = handle.write_ledger(meta, classification_cache).await {
            handle.abort().await;
            return Err(err);
        }
        handle.commit().await
    }
}

/// Lifecycle handle for one backfill partition's writes.
///
/// PG: thin borrow of the pool + the per-worker classification cache.
/// Each `write_ledger` is its own atomic transaction.
///
/// Clickhouse: owns a [`db_clickhouse::persist::PartitionWriter`] — the actual
/// 14-handle insert lifecycle described in
/// `db-clickhouse/src/persist/writer.rs`.
#[allow(clippy::large_enum_variant)]
pub enum PartitionWriterHandle<'a> {
    Postgres { pool: &'a PgPool },
    Clickhouse(db_clickhouse::persist::PartitionWriter),
}

impl PartitionWriterHandle<'_> {
    pub async fn write_ledger(
        &mut self,
        meta: &LedgerCloseMeta,
        classification_cache: &ClassificationCache,
    ) -> Result<(), BackfillError> {
        match self {
            Self::Postgres { pool } => {
                indexer::handler::process::process_ledger(meta, pool, None, classification_cache)
                    .await?;
                Ok(())
            }
            Self::Clickhouse(pw) => {
                // Parse on every ledger; the CH path doesn't share the
                // PG-side staging cache. `classification_cache` is
                // ignored: it's a PG-specific NFT filter helper (task
                // 0118 Phase 2) and the CH writer doesn't run the NFT
                // reclassification UPDATE path that needs it.
                let _ = classification_cache;
                let parsed = indexer::handler::process::parse_ledger(meta);
                // Task 0220 — switch to the `_with_sac_overrides` entry
                // point so the CH writer flips `is_sac=true,
                // contract_type=Token` on pre-existing SAC skeleton
                // rows via the forward-derived `ParseOutput.sac_overrides`
                // list. The legacy `stage::prepare` is kept as a
                // backwards-compat shim with empty overrides; this is
                // the production wire-up the PR #186 description called
                // out as a follow-up.
                let staged = db_clickhouse::persist::stage::prepare_with_sac_overrides(
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
                    &parsed.sac_overrides,
                    // Task 0283 live G1/G9 are for the live indexer path only.
                    // Backfill stays as-is (empty maps = pre-0283 behaviour):
                    // historical cross-ledger verdicts are reconstructed by
                    // the batch `ch-maint contract-type-rebuild` + one-shot
                    // `nft-reclassify`, not inline.
                    &std::collections::HashMap::new(),
                    &std::collections::HashMap::new(),
                )?;
                pw.write_ledger(staged).await?;
                Ok(())
            }
        }
    }

    /// End every open insert (CH variant), or a no-op (PG variant).
    /// Mid-partition failure must call [`Self::abort`] instead.
    pub async fn commit(self) -> Result<(), BackfillError> {
        match self {
            Self::Postgres { .. } => Ok(()),
            Self::Clickhouse(pw) => {
                pw.commit().await?;
                Ok(())
            }
        }
    }

    /// Abandon the partition. PG: nothing to roll back (each ledger
    /// committed individually). CH: drops in-flight insert handles
    /// without ending them, ensuring resume finds no `ledgers` rows
    /// for this partition's range and re-does it cleanly.
    pub async fn abort(self) {
        match self {
            Self::Postgres { .. } => {}
            Self::Clickhouse(pw) => pw.abort().await,
        }
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
