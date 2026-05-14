//! backfill-runner — production-grade Stellar pubnet backfill to Postgres.
//!
//! Source: `aws-public-blockchain/v1.1/stellar/ledgers/pubnet/` (unsigned).
//! Sink:   Postgres, ADR 0027 schema, via
//!         `indexer::handler::process::process_ledger` (parse-and-persist).

mod bootstrap;
mod dashboard;
mod error;
mod ingest;
mod partition;
mod resume;
mod rpc_snapshot;
mod run;
mod sink;
mod status;
mod sync;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// Default local scratch dir. CLI `--temp-dir` or `BACKFILL_TEMP_DIR`
/// overrides. Single source of truth — `run` and `status` both receive
/// it via their `execute` args, no duplicated constant.
const DEFAULT_TEMP_DIR: &str = ".temp/backfill-runner";

/// Which parallel store to write to. Defaults to `postgres` so existing
/// invocations (CI scripts, runbooks, the aws-public-blockchain workflow)
/// keep working byte-for-byte without edits. `clickhouse` writes are
/// currently **stubbed** — the parse pipeline runs end-to-end but no
/// rows are persisted (task 0205, ADR 0044).
#[derive(Copy, Clone, Debug, ValueEnum)]
enum Target {
    Postgres,
    Clickhouse,
}

#[derive(Parser)]
#[command(name = "backfill-runner", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Which store to write to. Defaults to `postgres`.
    #[arg(long, value_enum, default_value = "postgres")]
    target: Target,

    /// PostgreSQL connection string. Required when `--target postgres`.
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,

    /// ClickHouse HTTP endpoint (e.g. `http://localhost:8123`).
    /// Overrides `CLICKHOUSE_URL` for the duration of the run when
    /// `--target clickhouse` is set. Other ClickHouse env vars
    /// (`CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`, `CLICKHOUSE_DATABASE`)
    /// are picked up by `db_clickhouse::Config::from_env()` as usual.
    #[arg(long, env = "CLICKHOUSE_URL")]
    clickhouse_url: Option<String>,

    /// Soroban RPC endpoint (e.g.
    /// `https://soroban-rpc.mainnet.stellar.gateway.fm`). Used by the
    /// CH bootstrap step (task 0214, audit §E06) to fetch live
    /// `AccountEntry` state for accounts referenced in the window
    /// but never updated inside it. Optional — when unset, the
    /// bootstrap step is skipped and accounts persist as the
    /// participants-driven skeleton rows. PG target ignores this flag.
    #[arg(long, env = "SOROBAN_RPC_URL")]
    soroban_rpc_url: Option<String>,

    /// Local scratch directory for `aws s3 sync` output. Each partition
    /// lands under `<temp-dir>/<HEX>--<start>-<end>/` and (by default)
    /// is deleted after it indexes successfully.
    #[arg(long, env = "BACKFILL_TEMP_DIR", default_value = DEFAULT_TEMP_DIR)]
    temp_dir: PathBuf,

    /// Keep each partition's local folder on disk after it finishes
    /// indexing. Default: delete (bounds disk at ~2 × partition_size).
    ///
    /// Intended for iteration / debugging — most useful when re-running
    /// the same range repeatedly (e.g. `--target clickhouse` stub work):
    /// the next `aws s3 sync` against a fully-populated folder is a cheap
    /// LIST instead of an 11.6 GB / 60 s re-download per partition.
    /// **Do not pass this for a real backfill** — disk grows linearly
    /// with the number of indexed partitions.
    #[arg(long)]
    keep_partitions: bool,

    /// Enable per-ledger and per-partition progress logs. Without this
    /// flag only warnings are shown during the run; the final summary
    /// (and the `status` table) prints either way.
    #[arg(long, short)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Run the backfill for a sequence range.
    Run {
        /// First ledger sequence (inclusive).
        #[arg(long)]
        start: u32,

        /// Last ledger sequence (inclusive).
        #[arg(long)]
        end: u32,
    },

    /// Report ingested / missing ledgers for a range.
    Status {
        /// First ledger sequence (inclusive).
        #[arg(long)]
        start: u32,

        /// Last ledger sequence (inclusive).
        #[arg(long)]
        end: u32,
    },

    /// Run only the account-state bootstrap RPC pass on an existing CH
    /// dataset. Top-ups `sequence_number = 0` skeletons via Soroban RPC
    /// `getLedgerEntries`. Idempotent — skips accounts where the
    /// in-window parser path already filled real state. Useful when an
    /// earlier `Run` was invoked without `--soroban-rpc-url` and left
    /// the dataset with elevated skeleton counts.
    ///
    /// CH-only — Postgres target short-circuits with an info log
    /// (PG's account-state population is independent per task 0119).
    Bootstrap {
        /// First ledger sequence (inclusive). Used by the discovery
        /// query's `transaction_participants` JOIN.
        #[arg(long)]
        start: u32,

        /// Last ledger sequence (inclusive). The snapshot stamp is
        /// `end + 1` to win under RMT(last_seen_ledger) — matches the
        /// per-window `Run` behaviour.
        #[arg(long)]
        end: u32,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Shared MultiProgress so the tracing writer and the run-level progress
    // bar coordinate: every tracing write suspends the bar, renders the log
    // line, then redraws the bar on the last line. Without this the bar
    // "streams" — each redraw appears on a new line below the previous log,
    // leaving a trail instead of one sticky bar at the bottom.
    let mp = indicatif::MultiProgress::new();
    // Type annotation is load-bearing — `IndicatifWriter::new` returns
    // `IndicatifWriter<W>` where `W` defaults to `Stderr` only via the
    // `Default` bound on a separate constructor; here Rust can't infer
    // it from `with_writer` downstream. Drop the annotation and
    // tracing-subscriber's `init()` fails with E0283.
    let writer: tracing_indicatif::writer::IndicatifWriter<tracing_indicatif::writer::Stderr> =
        tracing_indicatif::writer::IndicatifWriter::new(mp.clone());

    let filter = if cli.verbose { "info" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .init();

    // Errors currently panic (see task 0145, debug-first decision). The
    // subcommand entrypoints still return `Result` for pool / IO wiring;
    // `.expect` converts any residual Err into an immediate panic with a
    // clear message and no graceful-exit path.
    let sink = build_sink(
        cli.target,
        cli.database_url.as_deref(),
        cli.clickhouse_url.as_deref(),
    );

    match cli.command {
        Command::Run { start, end } => run::execute(
            &sink,
            &cli.temp_dir,
            start,
            end,
            cli.keep_partitions,
            cli.soroban_rpc_url.as_deref(),
            &mp,
        )
        .await
        .expect("backfill run failed"),
        Command::Status { start, end } => status::execute(&sink, start, end)
            .await
            .expect("status failed"),
        Command::Bootstrap { start, end } => {
            let rpc_url = cli.soroban_rpc_url.as_deref().unwrap_or_else(|| {
                panic!("bootstrap subcommand requires --soroban-rpc-url (or SOROBAN_RPC_URL env)")
            });
            let stats = bootstrap::bootstrap_account_state(&sink, Some(rpc_url), start, end)
                .await
                .expect("bootstrap failed");
            // Print one-line summary on stdout so operators piping into
            // shell scripts see the result without grep'ing tracing
            // output.
            println!(
                "bootstrap completed: discovered={} fetched={} staged={} rpc_batches={} rpc_errors={}",
                stats.discovered,
                stats.fetched,
                stats.staged_accounts,
                stats.rpc_batches,
                stats.rpc_errors,
            );
        }
    }
}

/// Build the `Sink` for the chosen target. Panics loudly at startup if
/// the URL required for the chosen target is missing — same posture as
/// the existing pre-flight panics.
///
/// The CH side reads remaining ClickHouse env vars (user, password,
/// database) via `db_clickhouse::Config::from_env`; the `--clickhouse-url`
/// CLI flag already overrides `CLICKHOUSE_URL` for the URL field
/// because clap is reading the same env var.
fn build_sink(
    target: Target,
    database_url: Option<&str>,
    clickhouse_url: Option<&str>,
) -> sink::Sink {
    match target {
        Target::Postgres => {
            let url = database_url.unwrap_or_else(|| {
                panic!("--target postgres requires --database-url or DATABASE_URL env")
            });
            let pool = db::pool::create_pool(url).expect("failed to construct Postgres pool");
            sink::Sink::Postgres(pool)
        }
        Target::Clickhouse => {
            let mut cfg = db_clickhouse::Config::from_env();
            if let Some(url) = clickhouse_url {
                cfg.url = url.to_string();
            }
            sink::Sink::Clickhouse(db_clickhouse::client(&cfg))
        }
    }
}
