//! `enrich` — local CLI that drains un-enriched ClickHouse rows into the
//! enrichment side tables (`asset_enrichment` / `nft_enrichment`, ADR 0048)
//! by calling the same `enrichment_shared::enrich_and_persist::*` functions
//! the live SQS-driven `enrichment-worker` Lambda invokes per message. No
//! SQS in the loop — a bulk publish would hit SQS rate limits and waste
//! per-message overhead when we already hold a ClickHouse client.
//!
//! Each `enrich_*` call owns the full "fetch externally + INSERT the side
//! table" path for one key; this binary only drives iteration (a
//! keyset-paginated candidate stream + a `Semaphore`-bounded fan-out).
//!
//! ## Subcommands
//!
//! - `sep1-assets` — drain classic/SAC `assets` (`asset_type IN (1, 2)`)
//!   that have no `asset_enrichment` row yet. Writes `icon_url` + `name`
//!   from the issuer SEP-1 TOML.
//! - `nft-metadata` — drain `nfts` that have no `nft_enrichment` row yet.
//!   Writes `name` / `media_url` / `collection_name` from `token_uri()`.
//! - `status` — print per-side-table coverage counts and exit.
//!
//! ## Semantics
//!
//! - **Standard filter** = the key has no side-table row yet
//!   (`(key) NOT IN (SELECT key FROM *_enrichment)`). Row existence is the
//!   "tried" marker (ADR 0048): a permanent fail still INSERTs a `''`
//!   sentinel row, so the key is skipped on the next pass.
//! - **`--force-retry`** = drop the filter and walk every key. `enrich_*`
//!   is idempotent — a later run upgrades a sentinel to a real value (or
//!   clears a removed one) via a newer-`version` INSERT.
//!
//! There is no surrogate `id` on ClickHouse (PR #175), so unlike the old
//! Postgres drain there is no `--id` single-row mode; address a single row
//! with `--force-retry --limit 1` against a narrowed deployment if needed.
//!
//! ## Connection
//!
//! Reads `CLICKHOUSE_URL` / `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` /
//! `CLICKHOUSE_DATABASE` (`db_clickhouse::Config::from_env`).
//!
//! ```bash
//! CLICKHOUSE_URL=http://localhost:8123 cargo run -p backfill-enrichment-runner -- sep1-assets --concurrency 10
//! CLICKHOUSE_URL=http://localhost:8123 cargo run -p backfill-enrichment-runner -- nft-metadata --force-retry
//! CLICKHOUSE_URL=http://localhost:8123 cargo run -p backfill-enrichment-runner -- status
//! ```

use std::sync::Arc;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use clickhouse::Client;
use enrichment_shared::enrich_and_persist::nft_token_uri::enrich_nft_token_uri;
use enrichment_shared::enrich_and_persist::sep1_assets::enrich_asset_from_sep1;
use enrichment_shared::enrich_and_persist::{AssetKey, EnrichError, EnrichOutcome, NftKey};
use enrichment_shared::nft_token_uri::NftTokenUriFetcher;
use enrichment_shared::sep1::Sep1Fetcher;
use tokio::sync::Semaphore;

#[derive(Parser)]
#[command(name = "enrich", about)]
struct Cli {
    /// Verbose logging — show per-row `debug` activity. Default: `warn` + error
    /// only (the report / status `println!`s are always shown). Mirrors
    /// `backfill-runner`'s `--verbose`; this crate's per-row detail is at
    /// `debug`, so verbose maps to `debug` (not `info`).
    #[arg(long, short, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Drain classic/SAC `assets` with no `asset_enrichment` row yet.
    /// Writes `icon_url` (`CURRENCIES[].image`) + `name`
    /// (`CURRENCIES[].name`) from the issuer SEP-1 TOML.
    #[command(name = "sep1-assets")]
    Sep1Assets(DrainArgs),
    /// Drain `nfts` with no `nft_enrichment` row yet. Writes
    /// `name` / `media_url` / `collection_name` from `token_uri()`.
    NftMetadata(DrainArgs),
    /// Print per-side-table coverage counts and exit.
    Status,
}

#[derive(Args)]
struct DrainArgs {
    /// Concurrent in-flight fetches. Must be >= 1 (`Semaphore(0)` would
    /// deadlock every spawned task on `acquire_owned()`).
    #[arg(long, default_value_t = 10, value_parser = parse_positive_usize)]
    concurrency: usize,

    /// Keys pulled per candidate chunk. Must be >= 1.
    #[arg(long, default_value_t = 200, value_parser = parse_positive_i64)]
    chunk_size: i64,

    /// Stop after processing at most N keys. Default: drain everything.
    #[arg(long)]
    limit: Option<usize>,

    /// Drop the "no side-table row yet" filter and walk every key.
    /// Idempotent re-enrich — refreshes stale sentinels / values.
    #[arg(long)]
    force_retry: bool,
}

fn parse_positive_usize(s: &str) -> Result<usize, String> {
    let v: usize = s
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    if v < 1 {
        return Err("must be >= 1".to_owned());
    }
    Ok(v)
}

fn parse_positive_i64(s: &str) -> Result<i64, String> {
    let v: i64 = s
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    if v < 1 {
        return Err("must be >= 1".to_owned());
    }
    Ok(v)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    // `--verbose` → `debug` (per-row activity); default → `warn` + error only.
    // Matches `backfill-runner`'s flag-driven filter (RUST_LOG is not consulted).
    let filter = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());

    let report = match cli.command {
        Command::Sep1Assets(args) => {
            let fetcher = Arc::new(Sep1Fetcher::new()?);
            run_sep1_assets(&client, fetcher, args).await?
        }
        Command::NftMetadata(args) => {
            let fetcher = Arc::new(NftTokenUriFetcher::new()?);
            run_nft_metadata(&client, fetcher, args).await?
        }
        Command::Status => {
            print_status(&client).await?;
            return Ok(());
        }
    };

    print_report(&report);

    // Exit non-zero ONLY on a real problem (DB failure) so a
    // `enrich sep1-assets && enrich nft-metadata` chain short-circuits.
    // Transient unreachability is the EXPECTED terminal state of a healthy run
    // against a flaky upstream — those keys just retry on the next run — so it
    // must NOT break the chain (exit 0). Sentinel writes count as `succeeded`.
    if report.db_failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `sep1-assets` — SEP-1 assets drain (icon_url + name from issuer TOML)
// ---------------------------------------------------------------------------

async fn run_sep1_assets(
    client: &Client,
    fetcher: Arc<Sep1Fetcher>,
    args: DrainArgs,
) -> Result<BackfillReport, clickhouse::error::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new("sep1-assets");
    let sem = Arc::new(Semaphore::new(args.concurrency));
    let mut cursor: Option<AssetKey> = None;

    loop {
        if limit_reached(args.limit, report.processed) {
            break;
        }
        let chunk = effective_chunk(args.chunk_size, args.limit, report.processed);
        let keys = select_sep1_chunk(client, cursor.as_ref(), chunk, args.force_retry).await?;
        if keys.is_empty() {
            break;
        }
        cursor = keys.last().cloned();

        let mut handles = Vec::with_capacity(keys.len());
        for key in keys {
            let sem = sem.clone();
            let fetcher = fetcher.clone();
            let client = client.clone();
            let label = key.to_string();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore never closed");
                let res = enrich_asset_from_sep1(&client, key, &fetcher).await;
                (label, res)
            }));
        }
        for h in handles {
            collect_join(&mut report, h.await);
        }
    }

    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

async fn select_sep1_chunk(
    client: &Client,
    cursor: Option<&AssetKey>,
    chunk: i64,
    force_retry: bool,
) -> Result<Vec<AssetKey>, clickhouse::error::Error> {
    // Candidate = a classic/SAC asset with no `asset_enrichment` row yet.
    let not_enriched = if force_retry {
        ""
    } else {
        " AND (asset_type, asset_code, issuer_id, contract_id) NOT IN \
          (SELECT asset_type, asset_code, issuer_id, contract_id FROM asset_enrichment)"
    };
    // Keyset pagination over the `assets` ORDER BY 4-tuple. Omitted on
    // page 1 so no value is bound (clickhouse 0.15 None-in-tuple defect).
    let cursor_clause = if cursor.is_some() {
        " AND (asset_type, asset_code, issuer_id, contract_id) > (?, ?, ?, ?)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT asset_type, asset_code, issuer_id, contract_id FROM assets FINAL \
         WHERE asset_type IN (1, 2){not_enriched}{cursor_clause} \
         ORDER BY asset_type, asset_code, issuer_id, contract_id LIMIT ?"
    );
    let mut q = client.query(&sql);
    if let Some(k) = cursor {
        q = q
            .bind(k.asset_type)
            .bind(&k.asset_code)
            .bind(k.issuer_id)
            .bind(k.contract_id);
    }
    q.bind(chunk).fetch_all::<AssetKey>().await
}

// ---------------------------------------------------------------------------
// `nft-metadata` — token_uri kind drain
// ---------------------------------------------------------------------------

async fn run_nft_metadata(
    client: &Client,
    fetcher: Arc<NftTokenUriFetcher>,
    args: DrainArgs,
) -> Result<BackfillReport, clickhouse::error::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new("nft-metadata");
    let sem = Arc::new(Semaphore::new(args.concurrency));
    let mut cursor: Option<NftKey> = None;

    loop {
        if limit_reached(args.limit, report.processed) {
            break;
        }
        let chunk = effective_chunk(args.chunk_size, args.limit, report.processed);
        let keys = select_nft_chunk(client, cursor.as_ref(), chunk, args.force_retry).await?;
        if keys.is_empty() {
            break;
        }
        cursor = keys.last().cloned();

        let mut handles = Vec::with_capacity(keys.len());
        for key in keys {
            let sem = sem.clone();
            let fetcher = fetcher.clone();
            let client = client.clone();
            let label = key.to_string();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire_owned().await.expect("semaphore never closed");
                let res = enrich_nft_token_uri(&client, key, &fetcher).await;
                (label, res)
            }));
        }
        for h in handles {
            collect_join(&mut report, h.await);
        }
    }

    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

async fn select_nft_chunk(
    client: &Client,
    cursor: Option<&NftKey>,
    chunk: i64,
    force_retry: bool,
) -> Result<Vec<NftKey>, clickhouse::error::Error> {
    let not_enriched = if force_retry {
        ""
    } else {
        " AND (contract_id, token_id) NOT IN \
          (SELECT contract_id, token_id FROM nft_enrichment)"
    };
    let cursor_clause = if cursor.is_some() {
        " AND (contract_id, token_id) > (?, ?)"
    } else {
        ""
    };
    // `WHERE 1{not_enriched}` keeps the clause-concatenation uniform with
    // the keyset clause when both are present.
    let sql = format!(
        "SELECT contract_id, token_id FROM nfts FINAL \
         WHERE 1 = 1{not_enriched}{cursor_clause} \
         ORDER BY contract_id, token_id LIMIT ?"
    );
    let mut q = client.query(&sql);
    if let Some(k) = cursor {
        q = q.bind(k.contract_id).bind(&k.token_id);
    }
    q.bind(chunk).fetch_all::<NftKey>().await
}

// ---------------------------------------------------------------------------
// `status` — per-side-table coverage counts
// ---------------------------------------------------------------------------

/// Per-side-table coverage. "untried" = candidate keys with no enrichment row
/// yet (existence = "tried", ADR 0048). Per column: "real" = non-empty
/// non-NULL value, "sentinel" = the `''` permanent-fail marker.
async fn print_status(client: &Client) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    async fn cnt(c: &Client, sql: &str) -> Result<u64, clickhouse::error::Error> {
        c.query(sql).fetch_one::<u64>().await
    }
    /// `(real, sentinel)` split for one `Nullable(String)` side-table column.
    async fn col(
        c: &Client,
        table: &str,
        column: &str,
    ) -> Result<(u64, u64), clickhouse::error::Error> {
        let real = cnt(
            c,
            &format!("SELECT countIf({column} IS NOT NULL AND {column} != '') FROM {table} FINAL"),
        )
        .await?;
        let sentinel = cnt(
            c,
            &format!("SELECT countIf({column} = '') FROM {table} FINAL"),
        )
        .await?;
        Ok((real, sentinel))
    }

    let a_cand = cnt(
        client,
        "SELECT count() FROM assets FINAL WHERE asset_type IN (1, 2)",
    )
    .await?;
    let a_rows = cnt(client, "SELECT count() FROM asset_enrichment FINAL").await?;
    let (a_icon_r, a_icon_s) = col(client, "asset_enrichment", "icon_url").await?;
    let (a_name_r, a_name_s) = col(client, "asset_enrichment", "name").await?;

    let n_cand = cnt(client, "SELECT count() FROM nfts FINAL").await?;
    let n_rows = cnt(client, "SELECT count() FROM nft_enrichment FINAL").await?;
    let (n_name_r, n_name_s) = col(client, "nft_enrichment", "name").await?;
    let (n_media_r, n_media_s) = col(client, "nft_enrichment", "media_url").await?;
    let (n_coll_r, n_coll_s) = col(client, "nft_enrichment", "collection_name").await?;

    println!("# enrich — status\n");

    println!("## `asset_enrichment` (classic/SAC assets)\n");
    println!(
        "candidates: {a_cand} | rows (tried): {a_rows} | untried: {}\n",
        a_cand.saturating_sub(a_rows)
    );
    println!("| column     | real | `''` sentinel |");
    println!("| ---------- | ---: | ------------: |");
    println!("| `icon_url` | {a_icon_r:>4} | {a_icon_s:>13} |");
    println!("| `name`     | {a_name_r:>4} | {a_name_s:>13} |");

    println!("\n## `nft_enrichment`\n");
    println!(
        "candidates: {n_cand} | rows (tried): {n_rows} | untried: {}\n",
        n_cand.saturating_sub(n_rows)
    );
    println!("| column            | real | `''` sentinel |");
    println!("| ----------------- | ---: | ------------: |");
    println!("| `name`            | {n_name_r:>4} | {n_name_s:>13} |");
    println!("| `media_url`       | {n_media_r:>4} | {n_media_s:>13} |");
    println!("| `collection_name` | {n_coll_r:>4} | {n_coll_s:>13} |");

    Ok(())
}

/// Cap this chunk so the total never overshoots `--limit`.
fn effective_chunk(chunk_size: i64, limit: Option<usize>, processed: usize) -> i64 {
    match limit {
        Some(cap) => {
            let remaining = cap.saturating_sub(processed) as i64;
            chunk_size.min(remaining.max(0))
        }
        None => chunk_size,
    }
}

fn limit_reached(limit: Option<usize>, processed: usize) -> bool {
    limit.is_some_and(|cap| processed >= cap)
}

/// Fold a joined task. A `JoinError` (panic) is tallied as `db_failed` so a
/// single bad key cannot tear the drain down.
fn collect_join(
    report: &mut BackfillReport,
    joined: Result<(String, Result<EnrichOutcome, EnrichError>), tokio::task::JoinError>,
) {
    match joined {
        Ok((key, res)) => tally(report, &key, res),
        Err(e) => {
            report.processed += 1;
            report.db_failed += 1;
            tracing::error!(error = %e, "enrich task panicked (join error); counted as db_failed");
        }
    }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct BackfillReport {
    kind: &'static str,
    processed: usize,
    /// A real value landed (`EnrichOutcome::Real`). Split from sentinels so the
    /// report's vocabulary matches the `status` real/sentinel breakdown.
    enriched: usize,
    /// Only a `''` "tried, nothing" sentinel was written (permanent fail / no
    /// match). Still a healthy terminal outcome — just no usable value.
    sentinel: usize,
    unreachable: usize,
    db_failed: usize,
    transient_samples: Vec<TransientSample>,
    duration_ms: u128,
}

/// A bounded sample of transient (retryable) failures — `key` is the composite
/// key label so an operator sees the exact rows to re-run / investigate.
#[derive(Debug)]
struct TransientSample {
    key: String,
    message: String,
}

/// Cap the sample list so a million-key drain against a broken upstream cannot
/// OOM on diagnostics.
const TRANSIENT_SAMPLE_CAP: usize = 100;

impl BackfillReport {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            processed: 0,
            enriched: 0,
            sentinel: 0,
            unreachable: 0,
            db_failed: 0,
            transient_samples: Vec::new(),
            duration_ms: 0,
        }
    }
}

/// Fold one `enrich_*` outcome into the report. `Real`/`Sentinel` are the two
/// successful terminal writes (counted separately); `Transient` = unreachable
/// upstream (retry later, sampled); `Database` = ClickHouse error (logged).
fn tally(report: &mut BackfillReport, key: &str, res: Result<EnrichOutcome, EnrichError>) {
    report.processed += 1;
    match res {
        Ok(EnrichOutcome::Real) => report.enriched += 1,
        Ok(EnrichOutcome::Sentinel) => report.sentinel += 1,
        Err(EnrichError::Transient(msg)) => {
            report.unreachable += 1;
            if report.transient_samples.len() < TRANSIENT_SAMPLE_CAP {
                report.transient_samples.push(TransientSample {
                    key: key.to_owned(),
                    message: msg,
                });
            }
        }
        Err(EnrichError::Database(e)) => {
            report.db_failed += 1;
            tracing::error!(key, error = %e, "database error during enrichment");
        }
    }
}

fn print_report(r: &BackfillReport) {
    println!("# enrich {} — drain report\n", r.kind);
    println!("**Processed:** {}", r.processed);
    println!("**Real values:** {}", r.enriched);
    println!("**Sentinels (`''` tried-nothing):** {}", r.sentinel);
    println!(
        "**Unreachable (transient, retry candidate):** {}",
        r.unreachable
    );
    println!("**DB failures:** {}", r.db_failed);
    println!("**Duration:** {} ms\n", r.duration_ms);

    if r.unreachable == 0 && r.db_failed == 0 {
        println!("✓ All processed keys reached a terminal outcome.");
        return;
    }
    if !r.transient_samples.is_empty() {
        println!(
            "## Sample transient errors (first {})\n",
            r.transient_samples.len()
        );
        println!("| key | error |");
        println!("| --- | --- |");
        for s in &r.transient_samples {
            println!("| {} | {} |", s.key, s.message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_err() -> EnrichError {
        EnrichError::Database(clickhouse::error::Error::Custom("boom".to_owned()))
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
        let r = BackfillReport::new("sep1-assets");
        assert_eq!(r.kind, "sep1-assets");
        assert_eq!(r.processed, 0);
        assert_eq!(r.enriched, 0);
        assert_eq!(r.sentinel, 0);
        assert_eq!(r.unreachable, 0);
        assert_eq!(r.db_failed, 0);
        assert!(r.transient_samples.is_empty());
    }

    #[test]
    fn tally_real_and_sentinel_increment_separately() {
        let mut r = BackfillReport::new("sep1-assets");
        tally(&mut r, "k", Ok(EnrichOutcome::Real));
        tally(&mut r, "k2", Ok(EnrichOutcome::Sentinel));
        assert_eq!(r.processed, 2);
        assert_eq!(r.enriched, 1);
        assert_eq!(r.sentinel, 1);
        assert_eq!(r.unreachable, 0);
        assert_eq!(r.db_failed, 0);
    }

    #[test]
    fn tally_transient_increments_unreachable_and_captures_sample() {
        let mut r = BackfillReport::new("sep1-assets");
        tally(
            &mut r,
            "1-USDC-42-7",
            Err(EnrichError::Transient("dns fail".into())),
        );
        assert_eq!(r.processed, 1);
        assert_eq!(r.unreachable, 1);
        assert_eq!(r.transient_samples.len(), 1);
        assert_eq!(r.transient_samples[0].key, "1-USDC-42-7");
        assert_eq!(r.transient_samples[0].message, "dns fail");
    }

    #[test]
    fn tally_database_error_increments_db_failed_without_sample() {
        let mut r = BackfillReport::new("sep1-assets");
        tally(&mut r, "k", Err(db_err()));
        assert_eq!(r.processed, 1);
        assert_eq!(r.db_failed, 1);
        assert!(r.transient_samples.is_empty());
    }

    #[test]
    fn transient_samples_cap_at_constant() {
        let mut r = BackfillReport::new("sep1-assets");
        // Overshoot the cap by a margin so the assertion holds for any CAP value.
        let n = TRANSIENT_SAMPLE_CAP + 15;
        for i in 0..n {
            tally(
                &mut r,
                &format!("k{i}"),
                Err(EnrichError::Transient(format!("err{i}"))),
            );
        }
        assert_eq!(r.processed, n);
        assert_eq!(r.unreachable, n);
        assert_eq!(r.transient_samples.len(), TRANSIENT_SAMPLE_CAP);
        for (i, s) in r.transient_samples.iter().enumerate() {
            assert_eq!(s.key, format!("k{i}"));
            assert_eq!(s.message, format!("err{i}"));
        }
    }

    #[test]
    fn tally_mixed_outcomes_accumulate_correctly() {
        let mut r = BackfillReport::new("nft-metadata");
        tally(&mut r, "a", Ok(EnrichOutcome::Real));
        tally(&mut r, "b", Ok(EnrichOutcome::Sentinel));
        tally(
            &mut r,
            "c",
            Err(EnrichError::Transient("upstream 502".into())),
        );
        tally(&mut r, "d", Err(db_err()));
        tally(&mut r, "e", Ok(EnrichOutcome::Real));
        assert_eq!(r.processed, 5);
        assert_eq!(r.enriched, 2);
        assert_eq!(r.sentinel, 1);
        assert_eq!(r.unreachable, 1);
        assert_eq!(r.db_failed, 1);
        assert_eq!(r.transient_samples.len(), 1);
    }

    /// CH-backed candidate-query smoke (task 0231 step 5): `select_sep1_chunk`
    /// returns only classic/SAC assets with no `asset_enrichment` row, skips
    /// the enriched one, excludes native; `--force-retry` drops the NOT-IN.
    /// `#[ignore]` (needs live local CH).
    #[tokio::test]
    #[ignore = "needs live local ClickHouse"]
    async fn select_sep1_chunk_skips_enriched_and_native() {
        let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
        // 1 classic (un-enriched), 1 SAC (enriched), 1 native (type-excluded).
        client
            .query(
                "INSERT INTO assets \
                 (asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url) \
                 VALUES (1,'AAA',7001,0,NULL,NULL,NULL,NULL), \
                        (2,'BBB',7002,7003,NULL,NULL,NULL,NULL), \
                        (0,'',0,0,NULL,NULL,NULL,NULL)",
            )
            .execute()
            .await
            .expect("seed assets");
        client
            .query(
                "INSERT INTO asset_enrichment \
                 (asset_type, asset_code, issuer_id, contract_id, icon_url, name, version) \
                 VALUES (2,'BBB',7002,7003,'i','n',now64(3))",
            )
            .execute()
            .await
            .expect("seed enrichment");

        // force_retry=false → only AAA (BBB enriched-skip, native type-skip).
        let got = select_sep1_chunk(&client, None, 100, false)
            .await
            .expect("candidate query");
        assert_eq!(
            got,
            vec![AssetKey {
                asset_type: 1,
                asset_code: "AAA".into(),
                issuer_id: 7001,
                contract_id: 0,
            }],
        );

        // force_retry=true → AAA + BBB (NOT-IN dropped), still no native.
        let forced = select_sep1_chunk(&client, None, 100, true)
            .await
            .expect("force-retry query");
        assert_eq!(forced.len(), 2);
        assert!(
            forced
                .iter()
                .all(|k| k.asset_type == 1 || k.asset_type == 2)
        );

        client
            .query(
                "ALTER TABLE assets DELETE WHERE asset_code IN ('AAA','BBB') \
                 OR (asset_type = 0 AND asset_code = '')",
            )
            .execute()
            .await
            .expect("cleanup assets");
        client
            .query("ALTER TABLE asset_enrichment DELETE WHERE asset_code = 'BBB'")
            .execute()
            .await
            .expect("cleanup enrichment");
    }

    /// NFT counterpart — `select_nft_chunk` skips the enriched key, returns the
    /// rest; `--force-retry` returns all. `#[ignore]`.
    #[tokio::test]
    #[ignore = "needs live local ClickHouse"]
    async fn select_nft_chunk_skips_enriched() {
        let client = db_clickhouse::client(&db_clickhouse::Config::from_env());
        client
            .query("INSERT INTO nfts (contract_id, token_id) VALUES (6001,'1'),(6002,'2')")
            .execute()
            .await
            .expect("seed nfts");
        client
            .query(
                "INSERT INTO nft_enrichment \
                 (contract_id, token_id, name, media_url, collection_name, version) \
                 VALUES (6001,'1','n','m','c',now64(3))",
            )
            .execute()
            .await
            .expect("seed nft enrichment");

        let got = select_nft_chunk(&client, None, 100, false)
            .await
            .expect("candidate query");
        assert_eq!(
            got,
            vec![NftKey {
                contract_id: 6002,
                token_id: "2".into(),
            }],
        );

        let forced = select_nft_chunk(&client, None, 100, true)
            .await
            .expect("force-retry");
        assert_eq!(forced.len(), 2);

        client
            .query("ALTER TABLE nfts DELETE WHERE contract_id IN (6001,6002)")
            .execute()
            .await
            .expect("cleanup nfts");
        client
            .query("ALTER TABLE nft_enrichment DELETE WHERE contract_id = 6001")
            .execute()
            .await
            .expect("cleanup enrichment");
    }
}
