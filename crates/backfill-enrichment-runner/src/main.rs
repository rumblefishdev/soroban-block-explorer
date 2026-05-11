//! `enrich` — local CLI that drains pre-existing un-enriched DB rows by
//! calling the same `enrichment_shared::enrich_and_persist::*` functions
//! the live SQS-driven `enrichment-worker` Lambda invokes per message.
//! No SQS in the loop — a 50K-row publish would (a) hit SQS rate limits
//! and (b) waste per-message visibility-timeout / delete-after-ack
//! overhead when we already hold a DB connection.
//!
//! Subcommands mirror enrichment kinds; one binary covers all of them
//! so the `BackfillReport` + chunk-stream + `Semaphore`-bounded fan-out
//! live exactly once.
//!
//! ## Subcommands
//!
//! - `icon` — drain `assets` rows where `icon_url IS NULL OR
//!   (asset_type IN (1, 2) AND name IS NULL)`. Writes `assets.icon_url`
//!   + `assets.name` via `enrich_asset_from_sep1`.
//! - `nft-metadata` — drain `nfts` rows where all three of
//!   `name / media_url / collection_name` are NULL. Writes
//!   `nfts.{name, media_url, collection_name}` via
//!   `enrich_nft_token_uri`. `NftTokenUriFetcher::resolve` is
//!   `unimplemented!()` until the Soroban-RPC + IPFS fetcher lands;
//!   running the subcommand on real data before then panics loudly
//!   inside the spawned task — caught by the join-error handler and
//!   tallied as `db_failed`, the drain continues but every NFT row
//!   reports a failure.
//! - `status` — print per-column NULL / sentinel counts for both kinds
//!   in one shot. Cheap point-in-time visibility for the runbook.
//!
//! ## Semantics
//!
//! - **Standard filter** = live-worker-aligned producer predicate. The
//!   `''` sentinel written on permanent fail breaks the OR-clause on
//!   the next pass; without sentinels the producer (and this drain)
//!   would infinitely re-publish rows the issuer can't satisfy.
//! - **`--force-retry`** = γ-semantics: drop the filter, walk every
//!   row. `enrich_*` functions are idempotent — real → real, sentinel
//!   → real (upgrade), real → sentinel only on legitimate
//!   re-classification (issuer removed the field). UPDATE uses
//!   `COALESCE(NULLIF($n, ''), col, $n)` so a sentinel never clobbers
//!   a real value.
//! - **`--id N`** = surgical mode for a single row (`assets.id` for
//!   `icon`, `nfts.id` for `nft-metadata`). Bypasses the streaming
//!   cursor. Exit code 0 on success, 1 on transient failure (caller
//!   can chain with `||` in a shell).
//!
//! ## Usage
//!
//! ```bash
//! DATABASE_URL=postgres://... cargo run -p backfill-enrichment-runner -- icon --concurrency 10
//! DATABASE_URL=postgres://... cargo run -p backfill-enrichment-runner -- icon --id 4242
//! DATABASE_URL=postgres://... cargo run -p backfill-enrichment-runner -- nft-metadata --force-retry
//! DATABASE_URL=postgres://... cargo run -p backfill-enrichment-runner -- status
//! ```

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use enrichment_shared::enrich_and_persist::EnrichError;
use enrichment_shared::enrich_and_persist::nft_token_uri::enrich_nft_token_uri;
use enrichment_shared::enrich_and_persist::sep1_assets::enrich_asset_from_sep1;
use enrichment_shared::nft_token_uri::NftTokenUriFetcher;
use enrichment_shared::sep1::Sep1Fetcher;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Semaphore;

#[derive(Parser)]
#[command(name = "enrich", about)]
struct Cli {
    /// PostgreSQL connection string.
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Drain `assets` rows missing `icon_url` or `name` (SEP-1 kind).
    Icon(DrainArgs),
    /// Drain `nfts` rows missing `name` / `media_url` / `collection_name`
    /// (`token_uri()` kind). Panics inside the spawned task until the
    /// Soroban-RPC fetcher lands; failures are caught and tallied.
    NftMetadata(DrainArgs),
    /// Print per-column NULL / sentinel counts for every kind and exit.
    Status,
}

#[derive(Args)]
struct DrainArgs {
    /// Concurrent in-flight fetches.
    #[arg(long, default_value_t = 10)]
    concurrency: usize,

    /// Rows pulled per SELECT chunk.
    #[arg(long, default_value_t = 200)]
    chunk_size: i64,

    /// Stop after processing at most N rows. Default: drain everything.
    #[arg(long)]
    limit: Option<usize>,

    /// Surgical mode — enrich a single row by id then exit.
    /// `assets.id` for `icon`, `nfts.id` for `nft-metadata`.
    #[arg(long)]
    id: Option<i32>,

    /// γ-semantics — drop the standard NULL filter and walk every row.
    /// Idempotent overwrite. Used after upstream fixes (issuer
    /// republishes TOML, RPC outage recovers) to refresh stale
    /// sentinels.
    #[arg(long)]
    force_retry: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Each spawned drain task holds a pool connection across SELECT
    // seed-row + HTTP fetch + UPDATE inside the enrich function.
    // Undersizing the pool would throttle effective concurrency below
    // the requested fan-out.
    let pool_size: u32 = match &cli.command {
        Command::Icon(a) | Command::NftMetadata(a) => a.concurrency as u32 + 2,
        Command::Status => 2,
    };
    let db = PgPoolOptions::new()
        .max_connections(pool_size)
        .connect(&cli.database_url)
        .await?;

    let report = match cli.command {
        Command::Icon(args) => {
            let fetcher = Arc::new(Sep1Fetcher::new()?);
            run_icon(&db, fetcher, args).await?
        }
        Command::NftMetadata(args) => {
            let fetcher = Arc::new(NftTokenUriFetcher::new()?);
            run_nft_metadata(&db, fetcher, args).await?
        }
        Command::Status => {
            print_status(&db).await?;
            db.close().await;
            return Ok(());
        }
    };

    print_report(&report);
    db.close().await;

    // Operator-chainable exit: non-zero on transient unreachability so
    // `enrich icon && enrich nft-metadata` short-circuits cleanly on
    // the first failing kind. Sentinel writes are part of `succeeded`
    // — they are the intended outcome of a permanent-fail row.
    if report.unreachable > 0 || report.db_failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `icon` — SEP-1 assets drain
// ---------------------------------------------------------------------------

async fn run_icon(
    db: &sqlx::PgPool,
    fetcher: Arc<Sep1Fetcher>,
    args: DrainArgs,
) -> Result<BackfillReport, sqlx::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new(Kind::Icon);

    if let Some(id) = args.id {
        let res = enrich_asset_from_sep1(db, id, &fetcher).await;
        tally(&mut report, id, res);
        report.duration_ms = started.elapsed().as_millis();
        return Ok(report);
    }

    let sem = Arc::new(Semaphore::new(args.concurrency));
    let mut last_id: i32 = 0;
    loop {
        if limit_reached(args.limit, report.processed) {
            break;
        }
        let chunk = effective_chunk(args.chunk_size, args.limit, report.processed);
        let ids = select_icon_chunk(db, last_id, chunk, args.force_retry).await?;
        if ids.is_empty() {
            break;
        }
        last_id = *ids.last().expect("chunk non-empty");

        let mut handles = Vec::with_capacity(ids.len());
        for asset_id in ids {
            let sem = sem.clone();
            let fetcher = fetcher.clone();
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore never closed");
                let res = enrich_asset_from_sep1(&db, asset_id, &fetcher).await;
                (asset_id, res)
            }));
        }
        for h in handles {
            collect_join(&mut report, h.await);
            if limit_reached(args.limit, report.processed) {
                break;
            }
        }
    }

    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

async fn select_icon_chunk(
    db: &sqlx::PgPool,
    last_id: i32,
    chunk_size: i64,
    force_retry: bool,
) -> Result<Vec<i32>, sqlx::Error> {
    // Cursor pagination on `assets.id` — stable forward walk that
    // tolerates concurrent INSERTs from the live indexer. `id > $1
    // ORDER BY id` is O(seek) on the PK index.
    let sql = if force_retry {
        "SELECT id FROM assets WHERE id > $1 ORDER BY id LIMIT $2"
    } else {
        "SELECT id FROM assets
         WHERE (icon_url IS NULL OR (asset_type IN (1, 2) AND name IS NULL))
           AND id > $1
         ORDER BY id LIMIT $2"
    };
    sqlx::query_scalar(sql)
        .bind(last_id)
        .bind(chunk_size)
        .fetch_all(db)
        .await
}

// ---------------------------------------------------------------------------
// `nft-metadata` — token_uri kind drain
// ---------------------------------------------------------------------------

async fn run_nft_metadata(
    db: &sqlx::PgPool,
    fetcher: Arc<NftTokenUriFetcher>,
    args: DrainArgs,
) -> Result<BackfillReport, sqlx::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new(Kind::NftTokenUri);

    if let Some(id) = args.id {
        let res = enrich_nft_token_uri(db, id, &fetcher).await;
        tally(&mut report, id, res);
        report.duration_ms = started.elapsed().as_millis();
        return Ok(report);
    }

    let sem = Arc::new(Semaphore::new(args.concurrency));
    let mut last_id: i32 = 0;
    loop {
        if limit_reached(args.limit, report.processed) {
            break;
        }
        let chunk = effective_chunk(args.chunk_size, args.limit, report.processed);
        let ids = select_nft_chunk(db, last_id, chunk, args.force_retry).await?;
        if ids.is_empty() {
            break;
        }
        last_id = *ids.last().expect("chunk non-empty");

        let mut handles = Vec::with_capacity(ids.len());
        for nft_id in ids {
            let sem = sem.clone();
            let fetcher = fetcher.clone();
            let db = db.clone();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore never closed");
                let res = enrich_nft_token_uri(&db, nft_id, &fetcher).await;
                (nft_id, res)
            }));
        }
        for h in handles {
            collect_join(&mut report, h.await);
            if limit_reached(args.limit, report.processed) {
                break;
            }
        }
    }

    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

async fn select_nft_chunk(
    db: &sqlx::PgPool,
    last_id: i32,
    chunk_size: i64,
    force_retry: bool,
) -> Result<Vec<i32>, sqlx::Error> {
    let sql = if force_retry {
        "SELECT id FROM nfts WHERE id > $1 ORDER BY id LIMIT $2"
    } else {
        // All three columns NULL = "live worker (or this bin) never
        // touched the row". A successful pass always writes all three
        // (real values or empty sentinels), so any populated column
        // means a terminal decision has been recorded.
        "SELECT id FROM nfts
         WHERE name IS NULL
           AND media_url IS NULL
           AND collection_name IS NULL
           AND id > $1
         ORDER BY id LIMIT $2"
    };
    sqlx::query_scalar(sql)
        .bind(last_id)
        .bind(chunk_size)
        .fetch_all(db)
        .await
}

// ---------------------------------------------------------------------------
// `status` — combined report across kinds
// ---------------------------------------------------------------------------

async fn print_status(db: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let assets_q = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE icon_url IS NULL)                                AS icon_null,
            COUNT(*) FILTER (WHERE icon_url = '')                                   AS icon_sentinel,
            COUNT(*) FILTER (WHERE asset_type IN (1, 2) AND name IS NULL)           AS name_null,
            COUNT(*) FILTER (WHERE asset_type IN (1, 2) AND name = '')              AS name_sentinel,
            COUNT(*)                                                                AS total
        FROM assets
        "#,
    )
    .fetch_one(db);

    let nfts_q = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE name IS NULL)                                AS name_null,
            COUNT(*) FILTER (WHERE name = '')                                   AS name_sentinel,
            COUNT(*) FILTER (WHERE media_url IS NULL)                           AS media_null,
            COUNT(*) FILTER (WHERE media_url = '')                              AS media_sentinel,
            COUNT(*) FILTER (WHERE collection_name IS NULL)                     AS collection_null,
            COUNT(*) FILTER (WHERE collection_name = '')                        AS collection_sentinel,
            COUNT(*)                                                            AS total
        FROM nfts
        "#,
    )
    .fetch_one(db);

    let (assets, nfts) = tokio::try_join!(assets_q, nfts_q)?;

    let now = Utc::now().to_rfc3339();
    println!("# backfill-enrichment-runner — status\n");
    println!("**Timestamp:** {now}\n");

    println!("## `icon` kind (assets table)\n");
    print_status_header();
    print_status_row(
        "`assets.icon_url`           ",
        assets.try_get("icon_null")?,
        assets.try_get("icon_sentinel")?,
    );
    print_status_row(
        "`assets.name` (type IN 1, 2)",
        assets.try_get("name_null")?,
        assets.try_get("name_sentinel")?,
    );
    println!(
        "\n**Total assets rows:** {}\n",
        assets.try_get::<i64, _>("total")?
    );

    println!("## `nft-metadata` kind (nfts table)\n");
    print_status_header();
    print_status_row(
        "`nfts.name`                 ",
        nfts.try_get("name_null")?,
        nfts.try_get("name_sentinel")?,
    );
    print_status_row(
        "`nfts.media_url`            ",
        nfts.try_get("media_null")?,
        nfts.try_get("media_sentinel")?,
    );
    print_status_row(
        "`nfts.collection_name`      ",
        nfts.try_get("collection_null")?,
        nfts.try_get("collection_sentinel")?,
    );
    println!(
        "\n**Total nfts rows:** {}",
        nfts.try_get::<i64, _>("total")?
    );

    Ok(())
}

fn print_status_header() {
    println!("| column                       | NULL (pending) | `''` (sentinel) |");
    println!("| ---------------------------- | -------------: | --------------: |");
}

fn print_status_row(label: &str, null: i64, sentinel: i64) {
    println!("| {label} | {null:>14} | {sentinel:>15} |");
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn effective_chunk(chunk_size: i64, limit: Option<usize>, processed: usize) -> i64 {
    match limit {
        Some(cap) => (cap - processed).min(chunk_size as usize) as i64,
        None => chunk_size,
    }
}

fn limit_reached(limit: Option<usize>, processed: usize) -> bool {
    limit.is_some_and(|cap| processed >= cap)
}

/// Map a `JoinHandle::await` result into the report. A `JoinError`
/// here means the spawned task panicked (e.g. an `unimplemented!()`
/// fetcher path) — counting it as `db_failed` keeps the drain alive
/// instead of tearing down on one bad row.
fn collect_join(
    report: &mut BackfillReport,
    join_result: Result<(i32, Result<(), EnrichError>), tokio::task::JoinError>,
) {
    match join_result {
        Ok((id, res)) => tally(report, id, res),
        Err(e) => {
            tracing::error!(error = %e, "spawned drain task panicked");
            report.processed += 1;
            report.db_failed += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Kind {
    Icon,
    NftTokenUri,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Icon => "icon",
            Kind::NftTokenUri => "nft_token_uri",
        }
    }
}

#[derive(Debug)]
struct BackfillReport {
    kind: Kind,
    processed: usize,
    /// Either a real value landed in the row OR a sentinel was written
    /// (permanent-fail classification). Both are healthy terminal
    /// outcomes; distinguishing post-hoc means the `status` subcommand.
    succeeded: usize,
    unreachable: usize,
    db_failed: usize,
    /// Bounded so a million-row drain against a broken upstream doesn't OOM.
    transient_samples: Vec<TransientSample>,
    duration_ms: u128,
}

#[derive(Debug)]
struct TransientSample {
    id: i32,
    message: String,
}

const TRANSIENT_SAMPLE_CAP: usize = 10;

impl BackfillReport {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            processed: 0,
            succeeded: 0,
            unreachable: 0,
            db_failed: 0,
            transient_samples: Vec::new(),
            duration_ms: 0,
        }
    }
}

fn tally(report: &mut BackfillReport, id: i32, res: Result<(), EnrichError>) {
    report.processed += 1;
    match res {
        Ok(()) => report.succeeded += 1,
        Err(EnrichError::Transient(msg)) => {
            report.unreachable += 1;
            if report.transient_samples.len() < TRANSIENT_SAMPLE_CAP {
                report
                    .transient_samples
                    .push(TransientSample { id, message: msg });
            }
        }
        Err(EnrichError::Database(e)) => {
            report.db_failed += 1;
            tracing::error!(id, kind = report.kind.label(), error = %e, "database error during enrichment");
        }
    }
}

fn print_report(r: &BackfillReport) {
    let now = Utc::now().to_rfc3339();
    println!(
        "# backfill-enrichment-runner — {} drain report\n",
        r.kind.label()
    );
    println!("**Timestamp:** {now}");
    println!("**Processed:** {}", r.processed);
    println!("**Succeeded (incl. sentinel writes):** {}", r.succeeded);
    println!(
        "**Unreachable (transient, retry candidate):** {}",
        r.unreachable
    );
    println!("**DB failures:** {}", r.db_failed);
    println!("**Duration:** {} ms\n", r.duration_ms);

    if r.unreachable == 0 && r.db_failed == 0 {
        println!("✓ All processed rows reached a terminal outcome.");
        return;
    }
    if !r.transient_samples.is_empty() {
        println!(
            "## Sample transient errors (first {})\n",
            r.transient_samples.len()
        );
        println!("| id | error |");
        println!("| --- | --- |");
        for s in &r.transient_samples {
            println!("| {} | {} |", s.id, s.message);
        }
    }
}

// ---------------------------------------------------------------------------
// Pure-helper unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn db_err() -> EnrichError {
        EnrichError::Database(sqlx::Error::RowNotFound)
    }

    #[test]
    fn effective_chunk_no_limit_returns_chunk_size() {
        assert_eq!(effective_chunk(200, None, 0), 200);
        assert_eq!(effective_chunk(200, None, 1_000), 200);
    }

    #[test]
    fn effective_chunk_caps_at_remaining_when_below_chunk_size() {
        assert_eq!(effective_chunk(200, Some(50), 0), 50);
        assert_eq!(effective_chunk(200, Some(50), 30), 20);
    }

    #[test]
    fn effective_chunk_full_chunk_when_remaining_exceeds_chunk_size() {
        assert_eq!(effective_chunk(200, Some(1_000), 0), 200);
    }

    #[test]
    fn limit_reached_no_limit_is_false() {
        assert!(!limit_reached(None, 0));
        assert!(!limit_reached(None, 1_000_000));
    }

    #[test]
    fn limit_reached_below_cap_is_false() {
        assert!(!limit_reached(Some(50), 49));
    }

    #[test]
    fn limit_reached_at_or_above_cap_is_true() {
        assert!(limit_reached(Some(50), 50));
        assert!(limit_reached(Some(50), 51));
    }

    #[test]
    fn report_new_initialises_counters_to_zero() {
        let r = BackfillReport::new(Kind::Icon);
        assert_eq!(r.kind.label(), "icon");
        assert_eq!(r.processed, 0);
        assert_eq!(r.succeeded, 0);
        assert_eq!(r.unreachable, 0);
        assert_eq!(r.db_failed, 0);
        assert!(r.transient_samples.is_empty());
    }

    #[test]
    fn tally_ok_increments_succeeded() {
        let mut r = BackfillReport::new(Kind::Icon);
        tally(&mut r, 1, Ok(()));
        assert_eq!(r.processed, 1);
        assert_eq!(r.succeeded, 1);
        assert_eq!(r.unreachable, 0);
        assert_eq!(r.db_failed, 0);
    }

    #[test]
    fn tally_transient_increments_unreachable_and_captures_sample() {
        let mut r = BackfillReport::new(Kind::Icon);
        tally(&mut r, 42, Err(EnrichError::Transient("dns fail".into())));
        assert_eq!(r.processed, 1);
        assert_eq!(r.unreachable, 1);
        assert_eq!(r.transient_samples.len(), 1);
        assert_eq!(r.transient_samples[0].id, 42);
        assert_eq!(r.transient_samples[0].message, "dns fail");
    }

    #[test]
    fn tally_database_error_increments_db_failed_without_sample() {
        let mut r = BackfillReport::new(Kind::Icon);
        tally(&mut r, 7, Err(db_err()));
        assert_eq!(r.processed, 1);
        assert_eq!(r.db_failed, 1);
        assert!(r.transient_samples.is_empty());
    }

    #[test]
    fn transient_samples_cap_at_constant() {
        let mut r = BackfillReport::new(Kind::Icon);
        for i in 0..25 {
            tally(&mut r, i, Err(EnrichError::Transient(format!("err{i}"))));
        }
        assert_eq!(r.processed, 25);
        assert_eq!(r.unreachable, 25);
        assert_eq!(r.transient_samples.len(), TRANSIENT_SAMPLE_CAP);
        for (i, s) in r.transient_samples.iter().enumerate() {
            assert_eq!(s.id, i as i32);
            assert_eq!(s.message, format!("err{i}"));
        }
    }

    #[test]
    fn tally_mixed_outcomes_accumulate_correctly() {
        let mut r = BackfillReport::new(Kind::NftTokenUri);
        tally(&mut r, 1, Ok(()));
        tally(&mut r, 2, Ok(()));
        tally(
            &mut r,
            3,
            Err(EnrichError::Transient("upstream 502".into())),
        );
        tally(&mut r, 4, Err(db_err()));
        tally(&mut r, 5, Ok(()));
        assert_eq!(r.processed, 5);
        assert_eq!(r.succeeded, 3);
        assert_eq!(r.unreachable, 1);
        assert_eq!(r.db_failed, 1);
        assert_eq!(r.transient_samples.len(), 1);
    }
}
