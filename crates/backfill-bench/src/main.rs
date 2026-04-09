//! Local backfill benchmark — streams XDR files from Stellar's public S3 bucket
//! and indexes them into a local PostgreSQL database.
//!
//! Usage:
//!   cargo run -p backfill-bench -- --start 62015000 --end 62015999

use chrono::Local;
use clap::Parser;
use sqlx::PgPool;
use std::time::Instant;
use tracing::{error, info, warn};

const BUCKET_BASE_URL: &str =
    "https://aws-public-blockchain.s3.us-east-2.amazonaws.com/v1.1/stellar/ledgers/pubnet";
const PARTITION_SIZE: u32 = 64000;

#[derive(Parser)]
#[command(name = "backfill-bench", about = "Local backfill benchmark")]
struct Args {
    /// First ledger to index (inclusive)
    #[arg(long)]
    start: u32,

    /// Last ledger to index (inclusive)
    #[arg(long)]
    end: u32,

    /// PostgreSQL connection string (default: DATABASE_URL env var)
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,
}

/// Build the HTTPS URL for a single ledger file on the Stellar public S3 bucket.
fn ledger_url(ledger: u32) -> String {
    let partition_start = ledger - (ledger % PARTITION_SIZE);
    let partition_end = partition_start + PARTITION_SIZE - 1;
    let partition_hex = format!("{:08X}", u32::MAX - partition_start);
    let file_hex = format!("{:08X}", u32::MAX - ledger);

    format!(
        "{}/{partition_hex}--{partition_start}-{partition_end}/{file_hex}--{ledger}.xdr.zst",
        BUCKET_BASE_URL
    )
}

/// Check if a ledger already exists in the database.
async fn ledger_exists(pool: &PgPool, sequence: u32) -> Result<bool, sqlx::Error> {
    let row =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM ledgers WHERE sequence = $1)")
            .bind(sequence as i64)
            .fetch_one(pool)
            .await?;

    Ok(row)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let args = Args::parse();

    if args.start > args.end {
        error!("--start ({}) must be <= --end ({})", args.start, args.end);
        std::process::exit(1);
    }

    let total_range = (args.end - args.start + 1) as usize;
    let start_time = Local::now();
    let timer = Instant::now();

    info!(
        start = args.start,
        end = args.end,
        total = total_range,
        "backfill starting at {}",
        start_time.format("%Y-%m-%d %H:%M:%S")
    );

    // Connect to local Postgres
    let pool = db::pool::create_pool(&args.database_url)?;
    info!("connected to database");

    let client = reqwest::Client::new();
    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut total_bytes = 0u64;
    let mut total_download_time = std::time::Duration::ZERO;

    for ledger in args.start..=args.end {
        // Skip if already indexed
        if ledger_exists(&pool, ledger).await? {
            skipped += 1;
            continue;
        }

        let url = ledger_url(ledger);

        // Download (measure time)
        let dl_start = Instant::now();
        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            warn!(ledger, status = %response.status(), "failed to download, skipping");
            skipped += 1;
            continue;
        }
        let compressed = response.bytes().await?;
        total_download_time += dl_start.elapsed();
        total_bytes += compressed.len() as u64;

        // Decompress
        let xdr_bytes = xdr_parser::decompress_zstd(&compressed)?;

        // Deserialize and process
        let batch = xdr_parser::deserialize_batch(&xdr_bytes)?;
        for ledger_meta in batch.ledger_close_metas.iter() {
            indexer::handler::process::process_ledger(ledger_meta, &pool).await?;
        }

        indexed += 1;

        // Progress log every 10 ledgers
        if indexed.is_multiple_of(10) {
            let elapsed = timer.elapsed();
            let avg_ms = elapsed.as_millis() as f64 / indexed as f64;
            info!(
                ledger,
                indexed,
                skipped,
                avg_ms = format!("{avg_ms:.0}"),
                "progress"
            );
        }
    }

    // Final report
    let end_time = Local::now();
    let elapsed = timer.elapsed();
    let avg_ms = if indexed > 0 {
        elapsed.as_millis() as f64 / indexed as f64
    } else {
        0.0
    };
    let avg_file_kb = if indexed > 0 {
        total_bytes as f64 / indexed as f64 / 1024.0
    } else {
        0.0
    };

    info!("=== Backfill complete ===");
    info!("Started:    {}", start_time.format("%Y-%m-%d %H:%M:%S"));
    info!("Finished:   {}", end_time.format("%Y-%m-%d %H:%M:%S"));
    info!("Range:      {} - {}", args.start, args.end);
    info!("Indexed:    {indexed}");
    info!("Skipped:    {skipped}");
    info!("Total script time:      {:.1}s", elapsed.as_secs_f64());
    info!(
        "S3 download total time: {:.1}s",
        total_download_time.as_secs_f64()
    );
    info!("Avg per ledger:         {avg_ms:.0} ms");
    info!("Avg file size:          {avg_file_kb:.1} KB");

    Ok(())
}
