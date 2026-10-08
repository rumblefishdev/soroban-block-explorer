//! `enrich` — local CLI that drains un-enriched ClickHouse rows into the
//! enrichment side tables (`asset_enrichment` / `nft_enrichment`, ADR 0050)
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
//! - `sep1-assets` — drain classic `assets` (`asset_type = 1`; a SAC is a
//!   facet of a classic row, not a separate type — ADR 0051) that have no
//!   `asset_enrichment` row yet. Writes `icon_url` + `name` from the issuer
//!   SEP-1 TOML.
//! - `nft-metadata` — drain `nfts` that have no `nft_enrichment` row yet.
//!   Writes `name` / `media_url` / `collection_name` from `token_uri()`.
//! - `nft-collection-name` — per-CONTRACT backfill of
//!   `nft_enrichment.collection_name` from the SEP-50 `name()` RPC simulate
//!   (task 0340). Repairs rows enriched before 0340 (real `name`/`media_url`,
//!   empty `collection_name`) that neither retry mode may touch.
//! - `status` — print per-side-table coverage counts and exit.
//!
//! ## Semantics
//!
//! - **Standard filter** = the key has no side-table row yet
//!   (`(key) NOT IN (SELECT key FROM *_enrichment)`). Row existence is the
//!   "tried" marker (ADR 0050): a permanent fail still INSERTs a `''`
//!   sentinel row, so the key is skipped on the next pass.
//! - **`--force-retry`** = drop the filter and walk every key. `enrich_*`
//!   is idempotent — a later run upgrades a sentinel to a real value (or
//!   clears a removed one) via a newer-`version` INSERT.
//! - **`--retry-sentinels`** = re-attempt ONLY existing all-`''` sentinel rows
//!   (candidate source flips to the side table itself). Real values — incl.
//!   partials (one real column + one `''`) — are left untouched, so a retry
//!   cannot clobber a good value with a regressed source. Mutually exclusive
//!   with `--force-retry`.
//!
//! There is no surrogate `id` on ClickHouse (PR #175), so unlike the old
//! Postgres drain there is no `--id` single-row mode; address a single row
//! with `--force-retry --limit 1` against a narrowed deployment if needed.
//!
//! ## Connection
//!
//! The same flags as `backfill-runner`, built by the same
//! `db_clickhouse::mtls::client_from_cli_flags`:
//!
//! - plain HTTP — `--clickhouse-url` / `CLICKHOUSE_URL` plus
//!   `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` (a local ClickHouse);
//! - mTLS — `--ch-cert` / `--ch-key` / `--ch-ca` (or `CLICKHOUSE_CERT` /
//!   `CLICKHOUSE_KEY` / `CLICKHOUSE_CA`), all three together, with
//!   `--clickhouse-url https://<ch host>`: the production ClickHouse behind
//!   Caddy, which maps the certificate's CN to a ClickHouse user.
//!
//! `CLICKHOUSE_DATABASE` picks the database (`testnet` for the testnet
//! explorer, unset for mainnet's `default`). `nft-metadata` and
//! `nft-collection-name` also need `SOROBAN_RPC_URLS`, the Soroban RPC pool
//! of the same network.
//!
//! ```bash
//! CLICKHOUSE_URL=http://localhost:8123 SOROBAN_RPC_URLS=<rpc urls> cargo run -p backfill-enrichment-runner -- nft-metadata --force-retry
//! CLICKHOUSE_URL=http://localhost:8123 cargo run -p backfill-enrichment-runner -- sep1-assets --concurrency 10
//! CLICKHOUSE_URL=http://localhost:8123 SOROBAN_RPC_URLS=<rpc urls> cargo run -p backfill-enrichment-runner -- nft-collection-name
//! CLICKHOUSE_URL=http://localhost:8123 cargo run -p backfill-enrichment-runner -- status
//!
//! # production ClickHouse over mTLS, testnet database
//! CLICKHOUSE_DATABASE=testnet SOROBAN_RPC_URLS=https://soroban-testnet.stellar.org \
//!   cargo run -p backfill-enrichment-runner -- --clickhouse-url https://<ch host> \
//!   --ch-cert <user>.crt --ch-key <user>.key --ch-ca ca.crt nft-metadata --retry-sentinels
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use clickhouse::Client;
use enrichment_shared::enrich_and_persist::nft_collection_name::backfill_contract_collection_name;
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

    /// ClickHouse HTTP endpoint (e.g. `http://localhost:8123`, or the https
    /// Caddy host with the mTLS flags). Overrides `CLICKHOUSE_URL`. Same flag
    /// as `backfill-runner`.
    #[arg(long, env = "CLICKHOUSE_URL", global = true)]
    clickhouse_url: Option<String>,

    /// Client certificate (PEM) for mTLS to the Caddy-fronted ClickHouse.
    /// `--ch-cert` / `--ch-key` / `--ch-ca` go together; all three absent →
    /// plain client.
    #[arg(long, env = "CLICKHOUSE_CERT", global = true)]
    ch_cert: Option<PathBuf>,

    /// Client private key (PEM) — pairs with `--ch-cert`.
    #[arg(long, env = "CLICKHOUSE_KEY", global = true)]
    ch_key: Option<PathBuf>,

    /// CA cert (PEM) that signed the client cert — pairs with `--ch-cert`.
    #[arg(long, env = "CLICKHOUSE_CA", global = true)]
    ch_ca: Option<PathBuf>,

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
    /// Backfill `nft_enrichment.collection_name` from the contract-level
    /// SEP-50 `name()` (task 0340). Walks DISTINCT contracts whose rows still
    /// lack a collection name — one RPC per contract, then one INSERT-SELECT
    /// stamping the name onto its rows (`name` / `media_url` preserved).
    NftCollectionName(CollectionNameArgs),
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

    /// Drop the "no side-table row yet" filter and walk every key
    /// (re-enriches real AND sentinel rows alike). Idempotent re-enrich.
    #[arg(long)]
    force_retry: bool,

    /// Re-enrich ONLY existing all-`''` sentinel rows (permanent-fail
    /// "tried-nothing"); real values are left untouched. Walks the side table,
    /// not the base table. Mutually exclusive with `--force-retry`.
    #[arg(long, conflicts_with = "force_retry")]
    retry_sentinels: bool,
}

#[derive(Args)]
struct CollectionNameArgs {
    /// Concurrent in-flight `name()` fetches. Per-CONTRACT work (~tens of
    /// candidates), so the default is modest.
    #[arg(long, default_value_t = 4, value_parser = parse_positive_usize)]
    concurrency: usize,

    /// Stop after processing at most N contracts. Default: all candidates.
    #[arg(long)]
    limit: Option<usize>,
}

/// Which candidate set a drain walks (resolved from the flags).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DrainMode {
    /// Only keys with no side-table row yet (`(key) NOT IN *_enrichment`).
    /// The default (no flag).
    Untried,
    /// Only existing all-`''` sentinel rows — re-attempt permanent fails
    /// without touching real values (no clobber). `--retry-sentinels`.
    Sentinels,
    /// Every key, real + sentinel alike. `--force-retry`.
    All,
}

impl DrainArgs {
    fn mode(&self) -> DrainMode {
        if self.force_retry {
            DrainMode::All
        } else if self.retry_sentinels {
            DrainMode::Sentinels
        } else {
            DrainMode::Untried
        }
    }
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
    // Shared `MultiProgress` so the tracing writer and the drain's progress bar
    // share one stderr surface: logs render ABOVE the sticky bar instead of
    // shredding it. The type annotation is load-bearing (mirrors backfill-runner)
    // — `IndicatifWriter::new` is generic over `W` and `init()` fails E0283
    // without it. Bar auto-disables when stderr is not a TTY (piped / CI).
    let mp = indicatif::MultiProgress::new();
    let writer: tracing_indicatif::writer::IndicatifWriter<tracing_indicatif::writer::Stderr> =
        tracing_indicatif::writer::IndicatifWriter::new(mp.clone());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .init();
    let client = db_clickhouse::mtls::client_from_cli_flags(
        cli.clickhouse_url.as_deref(),
        cli.ch_cert.as_deref(),
        cli.ch_key.as_deref(),
        cli.ch_ca.as_deref(),
    );

    let report = match cli.command {
        Command::Sep1Assets(args) => {
            let fetcher = Arc::new(Sep1Fetcher::new()?);
            run_sep1_assets(&client, fetcher, args, &mp).await?
        }
        Command::NftMetadata(args) => {
            let fetcher = Arc::new(NftTokenUriFetcher::new()?);
            run_nft_metadata(&client, fetcher, args, &mp).await?
        }
        Command::NftCollectionName(args) => {
            let fetcher = Arc::new(NftTokenUriFetcher::new()?);
            run_nft_collection_name(&client, fetcher, args, &mp).await?
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
    mp: &indicatif::MultiProgress,
) -> Result<BackfillReport, clickhouse::error::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new("sep1-assets");
    let mode = args.mode();
    let bar = make_bar(
        mp,
        candidate_total(client, sep1_base(mode), args.limit).await?,
    );
    let sem = Arc::new(Semaphore::new(args.concurrency));
    let mut cursor: Option<AssetKey> = None;

    loop {
        if limit_reached(args.limit, report.processed) {
            break;
        }
        let chunk = effective_chunk(args.chunk_size, args.limit, report.processed);
        let keys = select_sep1_chunk(client, cursor.as_ref(), chunk, mode).await?;
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
            collect_join(&mut report, &bar, h.await);
        }
    }

    bar.finish();
    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

/// Candidate `FROM … WHERE …` (no SELECT clause) per drain mode. `Untried` /
/// `All` scan `assets`; `Sentinels` scans the side table's all-`''` rows so real
/// values are never re-fetched/clobbered. `FINAL` → latest-wins (no stale
/// pre-merge duplicate). Shared by `select_sep1_chunk` and the bar total count.
fn sep1_base(mode: DrainMode) -> &'static str {
    match mode {
        DrainMode::Untried => {
            "FROM assets FINAL WHERE asset_type = 1 \
             AND (asset_type, asset_code, issuer_id, contract_id) NOT IN \
             (SELECT asset_type, asset_code, issuer_id, contract_id FROM asset_enrichment)"
        }
        DrainMode::All => "FROM assets FINAL WHERE asset_type = 1",
        DrainMode::Sentinels => "FROM asset_enrichment FINAL WHERE icon_url = '' AND name = ''",
    }
}

/// Determinate-bar total = candidate count, clamped to `--limit` (so the bar
/// reaches 100%). One `count()` per drain over the same predicate the loop walks.
async fn candidate_total(
    client: &Client,
    base: &str,
    limit: Option<usize>,
) -> Result<u64, clickhouse::error::Error> {
    let n = client
        .query(&format!("SELECT count() {base}"))
        .fetch_one::<u64>()
        .await?;
    Ok(match limit {
        Some(l) => n.min(l as u64),
        None => n,
    })
}

/// Sticky determinate progress bar on the shared `MultiProgress`. Template
/// matches `backfill-runner`'s dashboard bar verbatim (bar / pos / len /
/// percent / elapsed / ETA) for cross-tool consistency, with a trailing `{msg}`
/// tail carrying the enrichment outcome tally (real · sentinel · transient ·
/// db) — set per row in `collect_join`. (backfill-runner surfaces its stats via
/// separate text-line spinners; a single drain warrants one bar.) `steady_tick`
/// refreshes elapsed/ETA between rows. Auto-hidden off-TTY.
fn make_bar(mp: &indicatif::MultiProgress, total: u64) -> indicatif::ProgressBar {
    let bar = mp.add(indicatif::ProgressBar::new(total));
    bar.set_style(
        indicatif::ProgressStyle::with_template(
            "{bar:40} {pos:>6} / {len:>6} ({percent:>3}%) elapsed {elapsed_precise} ETA {eta_precise}  {msg}",
        )
        .expect("static template is valid"),
    );
    bar.enable_steady_tick(std::time::Duration::from_millis(500));
    bar
}

async fn select_sep1_chunk(
    client: &Client,
    cursor: Option<&AssetKey>,
    chunk: i64,
    mode: DrainMode,
) -> Result<Vec<AssetKey>, clickhouse::error::Error> {
    let base = sep1_base(mode);
    // Keyset pagination over the shared ORDER BY 4-tuple (identical for `assets`
    // and `asset_enrichment`). Omitted on page 1 (clickhouse 0.15 None-in-tuple
    // defect).
    let cursor_clause = if cursor.is_some() {
        " AND (asset_type, asset_code, issuer_id, contract_id) > (?, ?, ?, ?)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT asset_type, asset_code, issuer_id, contract_id {base}{cursor_clause} \
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
    mp: &indicatif::MultiProgress,
) -> Result<BackfillReport, clickhouse::error::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new("nft-metadata");
    let mode = args.mode();
    let bar = make_bar(
        mp,
        candidate_total(client, nft_base(mode), args.limit).await?,
    );
    let sem = Arc::new(Semaphore::new(args.concurrency));
    let mut cursor: Option<NftKey> = None;

    loop {
        if limit_reached(args.limit, report.processed) {
            break;
        }
        let chunk = effective_chunk(args.chunk_size, args.limit, report.processed);
        let keys = select_nft_chunk(client, cursor.as_ref(), chunk, mode).await?;
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
            collect_join(&mut report, &bar, h.await);
        }
    }

    bar.finish();
    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

/// NFT candidate `FROM … WHERE …` per drain mode (see `sep1_base`).
fn nft_base(mode: DrainMode) -> &'static str {
    match mode {
        DrainMode::Untried => {
            "FROM nfts FINAL WHERE 1 = 1 \
             AND (contract_id, token_id) NOT IN \
             (SELECT contract_id, token_id FROM nft_enrichment)"
        }
        DrainMode::All => "FROM nfts FINAL WHERE 1 = 1",
        DrainMode::Sentinels => {
            "FROM nft_enrichment FINAL \
             WHERE name = '' AND media_url = '' AND collection_name = ''"
        }
    }
}

async fn select_nft_chunk(
    client: &Client,
    cursor: Option<&NftKey>,
    chunk: i64,
    mode: DrainMode,
) -> Result<Vec<NftKey>, clickhouse::error::Error> {
    let base = nft_base(mode);
    let cursor_clause = if cursor.is_some() {
        " AND (contract_id, token_id) > (?, ?)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT contract_id, token_id {base}{cursor_clause} \
         ORDER BY contract_id, token_id LIMIT ?"
    );
    let mut q = client.query(&sql);
    if let Some(k) = cursor {
        q = q.bind(k.contract_id).bind(&k.token_id);
    }
    q.bind(chunk).fetch_all::<NftKey>().await
}

// ---------------------------------------------------------------------------
// `nft-collection-name` — per-contract SEP-50 name() backfill (task 0340)
// ---------------------------------------------------------------------------

/// Candidate predicate: enrichment rows still lacking a collection name AND
/// whose contract has NO ledger-sourced name — the `name()` RPC is a FALLBACK
/// only (task 0340 redirect). The parser now captures the OZ NFT collection
/// name into `soroban_contract_metadata` from the ledger (Fix A / #330) and the
/// read path serves it via COALESCE (Fix B / #331), so those contracts need no
/// RPC. This drains only the ledger-uncovered remainder — hand-rolled contracts
/// (empty instance storage, name baked in WASM). The drain walks DISTINCT
/// contracts; the per-contract INSERT-SELECT re-applies the predicate, so a
/// re-run is a no-op for already-stamped rows (idempotent).
///
/// `contract_id` here is the `nft_enrichment` surrogate; the ledger exclusion
/// maps surrogate → StrKey via `soroban_contracts` (`soroban_contract_metadata`
/// is keyed by StrKey, RMT — `argMax(name, version)` reads the latest).
const NFT_COLLECTION_BASE: &str = "FROM nft_enrichment FINAL WHERE ifNull(collection_name, '') = '' \
     AND contract_id NOT IN ( \
         SELECT sc.id FROM soroban_contracts sc WHERE sc.contract_id IN ( \
             SELECT contract_id FROM soroban_contract_metadata \
             GROUP BY contract_id HAVING ifNull(argMax(name, version), '') != '' \
         ) \
     )";

async fn run_nft_collection_name(
    client: &Client,
    fetcher: Arc<NftTokenUriFetcher>,
    args: CollectionNameArgs,
    mp: &indicatif::MultiProgress,
) -> Result<BackfillReport, clickhouse::error::Error> {
    let started = Instant::now();
    let mut report = BackfillReport::new("nft-collection-name");

    // Bar total = DISTINCT contracts (progress is per contract, not per row).
    let total = client
        .query(&format!(
            "SELECT uniqExact(contract_id) {NFT_COLLECTION_BASE}"
        ))
        .fetch_one::<u64>()
        .await?;
    let total = args.limit.map_or(total, |l| total.min(l as u64));
    let bar = make_bar(mp, total);
    let sem = Arc::new(Semaphore::new(args.concurrency));

    // Single fetch, no chunk loop — the candidate set is contracts (~tens),
    // not tokens.
    let ids = client
        .query(&format!(
            "SELECT DISTINCT contract_id {NFT_COLLECTION_BASE} ORDER BY contract_id LIMIT ?"
        ))
        .bind(args.limit.map_or(i64::MAX, |l| l as i64))
        .fetch_all::<i64>()
        .await?;

    let mut handles = Vec::with_capacity(ids.len());
    for contract_id in ids {
        let sem = sem.clone();
        let fetcher = fetcher.clone();
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore never closed");
            let res = backfill_contract_collection_name(&client, contract_id, &fetcher).await;
            (contract_id.to_string(), res)
        }));
    }
    for h in handles {
        collect_join(&mut report, &bar, h.await);
    }

    bar.finish();
    report.duration_ms = started.elapsed().as_millis();
    Ok(report)
}

// ---------------------------------------------------------------------------
// `status` — per-side-table coverage counts
// ---------------------------------------------------------------------------

/// Per-side-table coverage. "untried" = candidate keys with no enrichment row
/// yet (existence = "tried", ADR 0050). Per column: "real" = non-empty
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
    /// `(all_real, partial, all_sentinel)` per-ROW classification — the
    /// per-column split can't tell "2 partial rows" from "1 all-real + 1
    /// all-sentinel" (same column tallies), so count rows directly. `all_real`
    /// = every column non-empty; `all_sent` = every column `''`/NULL; partial =
    /// the rest. (NFT `collection_name` is often legitimately empty, so a
    /// name+media-only NFT lands in `partial`, not `all_real`.)
    async fn row_class(
        c: &Client,
        table: &str,
        all_real: &str,
        all_sent: &str,
    ) -> Result<(u64, u64, u64), clickhouse::error::Error> {
        let total = cnt(c, &format!("SELECT count() FROM {table} FINAL")).await?;
        let real = cnt(c, &format!("SELECT countIf({all_real}) FROM {table} FINAL")).await?;
        let sent = cnt(c, &format!("SELECT countIf({all_sent}) FROM {table} FINAL")).await?;
        Ok((real, total.saturating_sub(real).saturating_sub(sent), sent))
    }

    let a_cand = cnt(
        client,
        "SELECT count() FROM assets FINAL WHERE asset_type = 1",
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

    let (a_row_real, a_row_part, a_row_sent) = row_class(
        client,
        "asset_enrichment",
        "coalesce(icon_url, '') != '' AND coalesce(name, '') != ''",
        "coalesce(icon_url, '') = '' AND coalesce(name, '') = ''",
    )
    .await?;
    let (n_row_real, n_row_part, n_row_sent) = row_class(
        client,
        "nft_enrichment",
        "coalesce(name, '') != '' AND coalesce(media_url, '') != '' AND coalesce(collection_name, '') != ''",
        "coalesce(name, '') = '' AND coalesce(media_url, '') = '' AND coalesce(collection_name, '') = ''",
    )
    .await?;

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
    println!("\nrows: {a_row_real} all-real · {a_row_part} partial · {a_row_sent} all-sentinel");

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
    println!("\nrows: {n_row_real} all-real · {n_row_part} partial · {n_row_sent} all-sentinel");

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
    bar: &indicatif::ProgressBar,
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
    // Tier-1 advances; Tier-2 tail carries the running outcome tally. Driven
    // single-threaded from the join-drain — no contention with the fan-out.
    bar.inc(1);
    bar.set_message(format!(
        "real {} · sentinel {} · transient {} · db {}",
        report.enriched, report.sentinel, report.unreachable, report.db_failed
    ));
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
    duration_ms: u128,
}

impl BackfillReport {
    fn new(kind: &'static str) -> Self {
        Self {
            kind,
            processed: 0,
            enriched: 0,
            sentinel: 0,
            unreachable: 0,
            db_failed: 0,
            duration_ms: 0,
        }
    }
}

/// Fold one `enrich_*` outcome into the report. `Real`/`Sentinel` are the two
/// successful terminal writes (counted separately); `Transient` = unreachable
/// upstream (retry later — `enrich_*` logs it at the call site with the key +
/// `reason="transient"`, so the report keeps only the count); `Database` =
/// ClickHouse error (logged).
fn tally(report: &mut BackfillReport, key: &str, res: Result<EnrichOutcome, EnrichError>) {
    report.processed += 1;
    match res {
        Ok(EnrichOutcome::Real) => report.enriched += 1,
        Ok(EnrichOutcome::Sentinel) => report.sentinel += 1,
        Err(EnrichError::Transient(_)) => report.unreachable += 1,
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
    } else {
        // Per-key detail (which key, what, why) is in the run's logs — every
        // failure logs `reason=…` + the key (in the `#[instrument]` span). The
        // report keeps only the counts (no redundant bounded sample).
        println!(
            "⚠ {} transient (retry) · {} db-failed — grep the run log by key / `reason=` for detail.",
            r.unreachable, r.db_failed
        );
    }
}

#[cfg(test)]
#[path = "tests/main_tests.rs"]
mod tests;
