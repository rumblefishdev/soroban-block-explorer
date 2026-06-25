//! Soroban token-metadata backfill worker — task 0304 (follow-up to 0297).
//!
//! Re-parses historical ledgers from local `.xdr.zst` files (synced from the
//! public S3 archive, same layout as `backfill-runner`'s sync — reuse task
//! 0266's already-synced `--local-dir`) and INSERTs the on-chain Soroban token
//! metadata side table populated by task 0297:
//!
//! * **`soroban_contract_metadata`** — `{contract_id, name, symbol, decimals,
//!   version}` recovered from each token contract's instance-storage `METADATA`
//!   struct. The table is created empty and only filled by live ingest going
//!   forward; the pre-0297 GREEN full backfill never wrote it, so every contract
//!   deployed/updated before metadata went live has no row until this runs.
//!
//! Everything else the parse produces (operations, events, nfts_pending, …) is
//! **discarded** (targeted write): each ledger is handed to `PartitionWriter`
//! as a `StagedLedger` with only `metadata_rows` set, so `write_rows` leaves the
//! other 13 tables' inserts unopened (`writer.rs` early-returns on an empty vec)
//! — zero parts, zero queries on them.
//!
//! ## Why it's safe against current data
//!
//! `soroban_contract_metadata` is `ReplacingMergeTree(version)` with `version =
//! observed ledger` (`stage::build_metadata_rows`). On merge, CH keeps the
//! max-`version` row per `contract_id`; reads use `FINAL`. A re-parsed historical
//! observation can therefore only **lose** to a newer live observation or **fill**
//! a contract live never saw — it never clobbers a newer row. Idempotent and
//! re-runnable. This worker is intended to run during the task 0281 maintenance
//! window with live ingest stopped, so there are no concurrent writes at all; the
//! RMT-version semantics are the backstop, not the primary guard.
//!
//! No `ledgers` commit marker is written (empty `ledger_rows`), so the full
//! backfill's `ledgers`-table resume state is untouched. This worker resumes off
//! its own `--watermark` file instead.
//!
//! ## Range invariant
//!
//! Set `--end` to the ledger where live ingest paused (`L_stop`) so the backfill
//! range `[50_457_424, L_stop]` meets live's prior coverage with no hole. A
//! contract whose latest metadata change falls in a gap `(end, live_start)` would
//! otherwise keep a stale/missing row. Overlap is free (idempotent).
//!
//! ## Preconditions
//! - Ledger files already synced to `--local-dir` (backfill-runner sync layout).
//! - Prod CH `users.d/timeouts.xml` raises `http_receive_timeout`: metadata is the
//!   sparsest table, so the long-held partition insert can sit idle between rows;
//!   the per-query `with_setting` in the writer does not propagate on CH 26.3 (see
//!   `writer.rs::apply_bulk_ingest_settings`). Should already be set from 0206/0266.
//! - `STELLAR_NETWORK_PASSPHRASE` set — `parse_ledger` derives the network id.
//!
//! ## Env
//! - `STELLAR_NETWORK_PASSPHRASE` (required)
//! - `CLICKHOUSE_URL` / `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` /
//!   `CLICKHOUSE_DATABASE` (`db_clickhouse::Config::from_env`)
//!
//! ## Usage
//! ```text
//! STELLAR_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015" \
//! CLICKHOUSE_URL=... metadata-backfill \
//!     --start 50457424 --end <L_stop> \
//!     --local-dir /var/tmp/backfill \
//!     --watermark /var/tmp/metadata.watermark \
//!     --throttle-ms 0
//! ```
//!
//! Parallelise across 8 workers on disjoint `--start/--end` ranges, each with its
//! own `--watermark` file (RMT dedups on merge regardless of order). The targeted
//! write opens one insert per worker, so 8 concurrent workers stay well within the
//! CH default `max_concurrent_queries`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Parser;
use db_clickhouse::persist::PartitionWriter;
use db_clickhouse::persist::rows::SorobanContractMetadataRow;
use db_clickhouse::persist::stage::{self, StagedLedger};
use tracing::{error, info, warn};

/// Public-archive S3 partition size (folder granularity). Independent of the CH
/// table's partitioning.
const PARTITION_SIZE: u32 = 64_000;

#[derive(Parser, Debug)]
#[command(
    about = "Task 0304 — backfill soroban_contract_metadata from re-parsed XDR (task 0297 side table)"
)]
struct Args {
    /// First ledger (inclusive). Soroban era starts at 50_457_424. Required (no
    /// default) so an 8-way parallel launch can't silently overlap ranges on a
    /// forgotten `--start`.
    #[arg(long)]
    start: u32,
    /// Last ledger (inclusive). Set to the live-ingest pause ledger (L_stop).
    #[arg(long)]
    end: u32,
    /// Directory holding synced `.xdr.zst` files (backfill-runner sync layout).
    #[arg(long)]
    local_dir: PathBuf,
    /// Resume watermark file: highest fully-done ledger; ledgers ≤ it are skipped.
    #[arg(long)]
    watermark: Option<PathBuf>,
    /// Parse + count but do NOT insert (prints the would-write row counts).
    #[arg(long)]
    dry_run: bool,
    /// Sleep this many ms between 64k partitions to throttle the CH.
    #[arg(long, default_value_t = 0)]
    throttle_ms: u64,
}

/// `seq` → local file path, mirroring `backfill-runner`'s `partition.rs`:
/// `{local_dir}/{HEX(MAX-pstart)}--{pstart}-{pend}/{HEX(MAX-seq)}--{seq}.xdr.zst`.
fn local_ledger_path(local_dir: &Path, seq: u32) -> PathBuf {
    let pstart = seq - (seq % PARTITION_SIZE);
    let pend = pstart + PARTITION_SIZE - 1;
    let folder_hex = format!("{:08X}", u32::MAX - pstart);
    let file_hex = format!("{:08X}", u32::MAX - seq);
    local_dir
        .join(format!("{folder_hex}--{pstart}-{pend}"))
        .join(format!("{file_hex}--{seq}.xdr.zst"))
}

/// Read one ledger file → parse → extract token-metadata rows.
///
/// `Ok(None)` means the file is missing (transient archive gap): the caller
/// warns + skips and freezes the watermark so a re-run re-does it. Any other
/// error (parse / decompress) aborts the partition — a data-shape bug, not a
/// gap.
async fn rows_for_file(
    path: &Path,
) -> Result<Option<Vec<SorobanContractMetadataRow>>, Box<dyn std::error::Error>> {
    let compressed = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Box::new(e)),
    };
    let xdr = xdr_parser::decompress_zstd(&compressed)?;
    let batch = xdr_parser::deserialize_batch(&xdr)?;
    if batch.ledger_close_metas.len() != 1 {
        // A malformed / non-single-ledger file is a hard error, not a missing-
        // file skip: return Err so the caller aborts the partition gracefully
        // (writer.abort()) instead of panicking past the cleanup mid-run.
        return Err(format!(
            "expected 1 ledger per file (public archive layout); got {} at {}",
            batch.ledger_close_metas.len(),
            path.display()
        )
        .into());
    }
    let meta = &batch.ledger_close_metas[0];
    let parsed = indexer::handler::process::parse_ledger(meta);
    // `build_metadata_rows` already did SAC filtering + version stamping in the
    // producer; the full staging fold is skipped (we only need this one table).
    Ok(Some(stage::build_metadata_rows(
        &parsed.contract_metadata_writes,
    )))
}

fn read_watermark(path: &Option<PathBuf>) -> Option<u32> {
    let p = path.as_ref()?;
    let s = std::fs::read_to_string(p).ok()?;
    s.trim().parse::<u32>().ok()
}

fn write_watermark(path: &Option<PathBuf>, ledger: u32) {
    if let Some(p) = path
        && let Err(e) = std::fs::write(p, ledger.to_string())
    {
        warn!(path = %p.display(), %e, "failed to persist watermark");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    assert!(args.start <= args.end, "--start must be ≤ --end");
    // Keep the partition loop's `pstart + PARTITION_SIZE` arithmetic within u32.
    assert!(args.end <= u32::MAX - PARTITION_SIZE, "--end out of range");

    let mut done_through = read_watermark(&args.watermark);
    if let Some(d) = done_through {
        info!(done_through = d, "resuming from watermark");
    }

    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());

    let mut pstart = args.start - (args.start % PARTITION_SIZE);
    let mut total_rows = 0usize;
    let mut total_skipped = 0usize;
    // ponytail: once a ledger file is missing we stop advancing the persisted
    // watermark for the rest of the run, so a re-run re-does from the last
    // fully-present partition once the gap is re-synced. Re-writes are idempotent
    // under RMT(version). Expected dead code on a complete archive.
    let mut watermark_frozen = false;
    while pstart <= args.end {
        let pend = (pstart + PARTITION_SIZE - 1).min(args.end);
        let lo = pstart.max(args.start);

        // Whole partition already behind the watermark → skip.
        if done_through.is_some_and(|d| pend <= d) {
            pstart += PARTITION_SIZE;
            continue;
        }

        let t = Instant::now();
        let mut writer = PartitionWriter::open(client.clone());
        let mut rows_in_part = 0usize;
        let mut skipped_in_part = 0usize;
        let mut failed: Option<Box<dyn std::error::Error>> = None;

        for seq in lo..=pend {
            if done_through.is_some_and(|d| seq <= d) {
                continue;
            }
            let path = local_ledger_path(&args.local_dir, seq);
            match rows_for_file(&path).await {
                Ok(Some(rows)) => {
                    rows_in_part += rows.len();
                    if !args.dry_run && !rows.is_empty() {
                        // Targeted write: only `metadata_rows` is set, so the
                        // writer opens no other table's insert. The RMT version
                        // is stamped inside `build_metadata_rows` (= observed
                        // ledger), not from `ledger_sequence` here.
                        let staged = StagedLedger {
                            ledger_sequence: i64::from(seq),
                            metadata_rows: rows,
                            ..Default::default()
                        };
                        if let Err(e) = writer.write_ledger(staged).await {
                            failed = Some(Box::new(e));
                            break;
                        }
                    }
                }
                Ok(None) => {
                    warn!(seq, path = %path.display(), "ledger file missing — skipping");
                    skipped_in_part += 1;
                }
                Err(e) => {
                    error!(seq, partition = pstart, %e, "ledger re-parse failed");
                    failed = Some(e);
                    break;
                }
            }
        }

        if let Some(e) = failed {
            writer.abort().await;
            return Err(e);
        }

        if args.dry_run {
            writer.abort().await;
        } else {
            writer.commit().await?;
        }

        total_rows += rows_in_part;
        total_skipped += skipped_in_part;

        if skipped_in_part > 0 {
            watermark_frozen = true;
            error!(
                partition = pstart,
                skipped = skipped_in_part,
                "missing ledger files in partition — watermark frozen here; re-sync the gap and re-run to fill"
            );
        }
        // Never advance the persisted watermark in --dry-run (nothing was
        // inserted): a dry-run sharing the real run's --watermark path would
        // otherwise mark the whole range done and the real run would skip
        // everything and write zero rows.
        if !args.dry_run && !watermark_frozen {
            done_through = Some(pend);
            write_watermark(&args.watermark, pend);
        }

        info!(
            partition = pstart,
            first = lo,
            last = pend,
            metadata_rows = rows_in_part,
            skipped = skipped_in_part,
            ms = t.elapsed().as_millis(),
            dry_run = args.dry_run,
            "partition done"
        );

        if args.throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(args.throttle_ms)).await;
        }
        pstart += PARTITION_SIZE;
    }

    info!(
        total_metadata_rows = total_rows,
        total_skipped,
        start = args.start,
        end = args.end,
        dry_run = args.dry_run,
        "metadata backfill range processed"
    );
    if total_skipped > 0 {
        // Non-zero exit so an orchestrator (the 0281-window runbook) can't
        // mistake a frozen-watermark partial run for a clean success.
        error!(
            total_skipped,
            "some ledger files were missing — re-sync those partitions and re-run; the watermark stopped at the last fully-present partition"
        );
        return Err(format!("{total_skipped} ledger files missing — backfill incomplete").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The archive path math is the bin's only non-trivial local logic; a wrong
    /// folder/file hex would silently read the wrong (or no) ledger. Guard it.
    #[test]
    fn local_ledger_path_matches_archive_layout() {
        // Literal expected path (NOT re-derived from the same formula) so a sign
        // or encoding flip in `local_ledger_path` is actually caught. seq
        // 50_457_424 sits in partition [50_432_000, 50_495_999].
        let got = local_ledger_path(Path::new("/var/tmp/backfill"), 50_457_424);
        assert_eq!(
            got,
            Path::new("/var/tmp/backfill/FCFE77FF--50432000-50495999/FCFE14AF--50457424.xdr.zst")
        );
    }
}
