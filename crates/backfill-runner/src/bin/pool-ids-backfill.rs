//! Pool-ids backfill worker — task 0266, **Phase 1 (pool_ids only)**.
//!
//! Re-parses historical ledgers from local `.xdr.zst` files (synced from the
//! public S3 archive, same layout as `backfill-runner`'s sync) and INSERTs
//! **only** the corrected `operations_appearances` rows — those of
//! transactions whose path-payment / offer ops crossed a liquidity pool, now
//! carrying `pool_ids` (task 0261). Everything else the parse produces
//! (events, nfts_pending, participants, …) is **discarded**, so:
//!
//! * the 0221 SAC leak is never re-emitted into drained partitions, and
//! * live ingest is untouched — backfilling OLD ledgers is RMT-key-disjoint
//!   from live ingest of NEW ledgers (different `(ledger, tx, app_order)`),
//!   so the two never collide and no ingestion pause is needed.
//!
//! **Correctness of the targeted write.** `stage::prepare` is reused verbatim,
//! so the 0163 fold identity matches live ingest exactly. Adding `pool_ids`
//! to the fold identity only ever *splits* groups, so the new fold's key set
//! is a superset of the old one for an affected tx — re-inserting the tx's
//! **full** fold replaces every old key (RMT, same sort key) and adds the new
//! ones; no orphan rows are left behind.
//!
//! `gross_volume_a` is **out of scope here** (Phase 2 — gated on the 0279
//! asset-A attribution decision).
//!
//! ## Preconditions
//! - Ledger files already synced to `--local-dir` (run `backfill-runner`'s
//!   `aws s3 sync`, or the partition sync, out of band).
//! - `0268` schema applied on the target CH (`pool_ids Array(FixedString(32))`).
//! - `oa_pool_seek` projection dropped on the target (avoids per-part rebuild).
//!
//! ## Usage
//! ```text
//! CLICKHOUSE_URL=... pool-ids-backfill \
//!     --start 50457424 --end 55499999 \
//!     --local-dir /var/tmp/backfill \
//!     --watermark /var/tmp/pool-ids.watermark \
//!     --throttle-ms 200
//! ```

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Parser;
use db_clickhouse::persist::PartitionWriter;
use db_clickhouse::persist::rows::OperationAppearanceRow;
use db_clickhouse::persist::stage::{self, StagedLedger};
use tracing::{error, info, warn};

/// Public-archive S3 partition size (folder granularity). Independent of the
/// CH table's 500k `intDiv(ledger_sequence, …)` partitioning.
const PARTITION_SIZE: u32 = 64_000;

/// Op-type discriminants whose `pool_ids` come from result claim atoms
/// (path payments 2/13 + offers 3/4/12 — `domain::OperationType`). LP
/// deposit/withdraw (22/23) also carry `pool_ids`, but from the op body and
/// already correct in CH, so they must NOT trigger a re-write on their own.
const ATOM_OP_TYPES: [i16; 5] = [2, 13, 3, 4, 12];

#[derive(Parser, Debug)]
#[command(
    about = "Task 0266 Phase 1 — backfill operations_appearances.pool_ids from re-parsed XDR"
)]
struct Args {
    /// First ledger (inclusive).
    #[arg(long)]
    start: u32,
    /// Last ledger (inclusive).
    #[arg(long)]
    end: u32,
    /// Directory holding synced `.xdr.zst` files (backfill-runner sync layout).
    #[arg(long)]
    local_dir: PathBuf,
    /// Resume watermark file: highest fully-done ledger; ledgers ≤ it are skipped.
    #[arg(long)]
    watermark: Option<PathBuf>,
    /// Parse + filter but do NOT insert (prints the would-write row counts).
    #[arg(long)]
    dry_run: bool,
    /// Sleep this many ms between 64k partitions to throttle the live CH.
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

/// Keep only the op rows belonging to a tx that crossed a pool via a
/// path-payment / offer op. Returns the **full fold** of each affected tx.
fn affected_op_rows(staged: StagedLedger) -> Vec<OperationAppearanceRow> {
    let crossed: HashSet<i64> = staged
        .op_rows
        .iter()
        .filter(|r| ATOM_OP_TYPES.contains(&r.op_type) && !r.pool_ids.is_empty())
        .map(|r| r.transaction_id)
        .collect();
    if crossed.is_empty() {
        return Vec::new();
    }
    staged
        .op_rows
        .into_iter()
        .filter(|r| crossed.contains(&r.transaction_id))
        .collect()
}

/// Read one ledger file → parse → stage → filter to affected op rows.
async fn affected_rows_for_file(
    path: &Path,
) -> Result<Vec<OperationAppearanceRow>, Box<dyn std::error::Error>> {
    let compressed = tokio::fs::read(path).await?;
    let xdr = xdr_parser::decompress_zstd(&compressed)?;
    let batch = xdr_parser::deserialize_batch(&xdr)?;
    assert_eq!(
        batch.ledger_close_metas.len(),
        1,
        "expected 1 ledger per file (public archive layout); got {} at {}",
        batch.ledger_close_metas.len(),
        path.display()
    );
    let meta = &batch.ledger_close_metas[0];
    let parsed = indexer::handler::process::parse_ledger(meta);
    let staged = stage::prepare(
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
    )?;
    Ok(affected_op_rows(staged))
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

    let mut done_through = read_watermark(&args.watermark);
    if let Some(d) = done_through {
        info!(done_through = d, "resuming from watermark");
    }

    let client = db_clickhouse::client(&db_clickhouse::Config::from_env());

    let mut pstart = args.start - (args.start % PARTITION_SIZE);
    let mut total_rows = 0usize;
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
        let mut failed: Option<Box<dyn std::error::Error>> = None;

        for seq in lo..=pend {
            if done_through.is_some_and(|d| seq <= d) {
                continue;
            }
            let path = local_ledger_path(&args.local_dir, seq);
            match affected_rows_for_file(&path).await {
                Ok(rows) => {
                    rows_in_part += rows.len();
                    if !args.dry_run && !rows.is_empty() {
                        let staged = StagedLedger {
                            ledger_sequence: i64::from(seq),
                            op_rows: rows,
                            ..Default::default()
                        };
                        if let Err(e) = writer.write_ledger(staged).await {
                            failed = Some(Box::new(e));
                            break;
                        }
                    }
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

        done_through = Some(pend);
        write_watermark(&args.watermark, pend);
        total_rows += rows_in_part;
        info!(
            partition = pstart,
            first = lo,
            last = pend,
            rows = rows_in_part,
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
        total_rows,
        start = args.start,
        end = args.end,
        dry_run = args.dry_run,
        "backfill range complete"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(tx_id: i64, app_order: i16, op_type: i16, pools: &[[u8; 32]]) -> OperationAppearanceRow {
        OperationAppearanceRow {
            transaction_id: tx_id,
            application_order: app_order,
            op_type,
            source_id: None,
            destination_id: None,
            contract_id: None,
            asset_code: String::new(),
            asset_issuer_id: None,
            pool_ids: pools.to_vec(),
            amount: 1,
            ledger_sequence: 100,
        }
    }

    fn staged(rows: Vec<OperationAppearanceRow>) -> StagedLedger {
        StagedLedger {
            ledger_sequence: 100,
            op_rows: rows,
            ..Default::default()
        }
    }

    #[test]
    fn keeps_full_fold_of_path_payment_crossing_tx() {
        // tx 1: path payment crossing pool A (app_order 1) + a sibling payment
        // (app_order 2, no pool). Both rows must be re-written (full fold).
        let rows = vec![
            op(1, 1, 13, &[[0xAA; 32]]), // PathPaymentStrictSend, crossed pool
            op(1, 2, 1, &[]),            // sibling Payment in the same tx
        ];
        let kept = affected_op_rows(staged(rows));
        assert_eq!(kept.len(), 2, "full fold of the affected tx is re-written");
    }

    #[test]
    fn excludes_lp_deposit_only_tx() {
        // tx 2: only an LP deposit (op_type 22) — carries pool_ids from the op
        // body, already correct in CH, must NOT trigger a re-write.
        let rows = vec![op(2, 1, 22, &[[0xBB; 32]])];
        let kept = affected_op_rows(staged(rows));
        assert!(
            kept.is_empty(),
            "LP deposit alone is not an atom-op crossing"
        );
    }

    #[test]
    fn excludes_order_book_only_path_payment() {
        // tx 3: path payment that filled only against the order book → no pool.
        let rows = vec![op(3, 1, 2, &[])];
        let kept = affected_op_rows(staged(rows));
        assert!(kept.is_empty());
    }

    #[test]
    fn mixed_ledger_keeps_only_crossing_tx_rows() {
        let rows = vec![
            op(1, 1, 13, &[[0xAA; 32]]), // tx1 crosses → keep
            op(1, 2, 1, &[]),            // tx1 sibling → keep
            op(2, 1, 22, &[[0xBB; 32]]), // tx2 LP deposit only → drop
            op(3, 1, 2, &[]),            // tx3 order-book path payment → drop
        ];
        let kept = affected_op_rows(staged(rows));
        let tx_ids: HashSet<i64> = kept.iter().map(|r| r.transaction_id).collect();
        assert_eq!(tx_ids, HashSet::from([1]));
        assert_eq!(kept.len(), 2);
    }
}
