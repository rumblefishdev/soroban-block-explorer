//! Parse + persist a single ledger from a local file, and drive the
//! sequential indexing of a whole partition. Thin glue over existing
//! crates — all write-path logic lives in
//! `indexer::handler::process::process_ledger`.
//!
//! The caller is responsible for producing the files on disk (via
//! `aws s3 sync` — see the `sync` module).

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use indexer::handler::HandlerError;
use tracing::{info, warn};

use crate::dashboard::Dashboard;
use crate::error::BackfillError;
use crate::partition::Partition;
use crate::sink::Sink;

/// Per-ledger parse + persist timings. Decompression isn't timed —
/// it's deterministic work on a fixed input and not a useful diagnostic
/// signal relative to parse/persist (task 0145 decision).
#[derive(Debug, Clone, Copy, Default)]
pub struct LedgerTimings {
    pub bytes: usize,
    pub parse_ms: u128,
    pub persist_ms: u128,
}

impl LedgerTimings {
    /// Total time attributable to this ledger (parse + persist). Used
    /// for min/max aggregation at the partition / run level.
    pub fn total_ms(&self) -> u128 {
        self.parse_ms + self.persist_ms
    }
}

/// Aggregate produced by `index_partition`. Powers the partition-end
/// log line and the run-level summary. Missing / failed ledgers are
/// **not** tracked — both panic (task 0145 debug-first stance), so the
/// only non-indexed bucket is "already in DB, skipped".
#[derive(Debug, Clone, Default)]
pub struct PartitionStats {
    pub indexed: usize,
    pub skipped_completed: usize,
    pub total_bytes: u64,
    pub parse_total_ms: u128,
    pub persist_total_ms: u128,
    /// Min / max per-ledger total_ms (parse + persist). `None` when the
    /// partition indexed zero ledgers (all already in DB, or empty).
    pub min_ledger_ms: Option<u128>,
    pub max_ledger_ms: Option<u128>,
    pub wall_clock: Duration,
}

/// Read, decompress, deserialize, and persist a single ledger file.
///
/// `partition_start` is passed in explicitly so the structured log event
/// carries enough context to answer "which partition owned this ledger?"
/// without re-parsing the filename.
///
/// Streams the ledger into `writer` (the open partition writer handle)
/// rather than the bare sink — task 0206 collapsed per-ledger persist
/// into the ClickHouse partition-writer lifecycle.
pub async fn ingest_ledger_from_file(
    path: &Path,
    writer: &mut crate::sink::PartitionWriterHandle,
    seq: u32,
    partition_start: u32,
) -> Result<LedgerTimings, BackfillError> {
    let compressed = tokio::fs::read(path).await?;
    let bytes = compressed.len();

    // Emit context BEFORE the parse/persist steps that can panic (task
    // 0145 debug-first stance). Without this, a panic in `process_ledger`
    // or the `deserialize_batch` invariant `assert_eq!` below leaves the
    // operator with just a backtrace — the `ledger ingested` event only
    // fires on the success path, so the feral ledger's seq / partition
    // would be invisible in the log stream. This line is the last one
    // written before any panic, guaranteeing the context is always
    // visible.
    info!(seq, partition = partition_start, bytes, "ledger starting");

    let xdr_bytes = xdr_parser::decompress_zstd(&compressed).map_err(HandlerError::from)?;

    let parse_start = Instant::now();
    let batch = xdr_parser::deserialize_batch(&xdr_bytes).map_err(HandlerError::from)?;
    let parse_ms = parse_start.elapsed().as_millis();

    // Public Stellar archive layout is one ledger per `.xdr.zst` file
    // (file named after the seq). If that ever changes the per-ledger
    // `info!` event below would log `seq` for a multi-ledger batch and
    // mislead. Debug-first stance → assert the invariant instead of
    // silently looping.
    assert_eq!(
        batch.ledger_close_metas.len(),
        1,
        "expected 1 ledger per file in public archive layout; got {} at {}",
        batch.ledger_close_metas.len(),
        path.display()
    );

    let persist_start = Instant::now();
    for meta in batch.ledger_close_metas.iter() {
        writer.write_ledger(meta).await?;
    }
    let persist_ms = persist_start.elapsed().as_millis();

    info!(
        seq,
        partition = partition_start,
        bytes,
        parse_ms,
        persist_ms,
        "ledger ingested"
    );

    Ok(LedgerTimings {
        bytes,
        parse_ms,
        persist_ms,
    })
}

/// Release an indexed ledger's file, unless `--keep-partitions` asked for it
/// to stay (see [`index_partition`]).
///
/// A failure is logged, not propagated: the partition folder is deleted
/// wholesale when the partition ends anyway, so the only cost of a failed
/// unlink is the space this was meant to reclaim early — not a reason to
/// abandon a partition that is otherwise indexing fine.
async fn drop_indexed_file(path: &Path, keep_files: bool, partition_start: u32, seq: u32) {
    if keep_files {
        return;
    }
    if let Err(err) = tokio::fs::remove_file(path).await {
        warn!(
            partition = partition_start,
            seq,
            %err,
            "could not drop indexed ledger file; the folder keeps it until the partition ends"
        );
    }
}

/// Sequentially index all ledgers in `partition` that fall within
/// `[range_start, range_end]`, skipping any sequence already present in
/// `completed`.
///
/// Assumes the partition has been synced to disk. A missing file here
/// means sync produced an incomplete dir (rare archive hole or a sync
/// bug) — we panic rather than warn-and-continue, per task 0145's
/// debug-first stance. Parse / persist errors similarly propagate and
/// panic at the top-level.
///
/// Emits `partition indexing started` / `partition indexing complete`
/// at info level when `--verbose` is on.
///
/// **Each ledger file is deleted the moment its rows are staged** — or the
/// moment it is skipped as already-in-DB (unless `keep_files`). The folder
/// therefore shrinks as it is parsed instead of
/// standing at full size until the partition ends, which is what bounds a
/// worker's disk footprint: with the next partition prefetching in parallel,
/// the peak drops from two whole partitions (~27 GB) to one plus a
/// shrinking remainder. Task 0488, after a backfill filled the production
/// box's filesystem and cost ClickHouse its write space.
///
/// Safe to delete this early: a crash mid-partition loses only the rows
/// since the last commit, and the restart re-syncs whatever is missing from
/// S3 before re-parsing — the same path a partial sync already takes.
#[allow(clippy::too_many_arguments)]
pub async fn index_partition(
    partition: &Partition,
    temp_dir: &Path,
    sink: &Sink,
    range_start: u32,
    range_end: u32,
    completed: &HashSet<u32>,
    dashboard: &Dashboard,
    keep_files: bool,
) -> Result<PartitionStats, BackfillError> {
    let (first, last) = partition.clamped(range_start, range_end);

    info!(
        partition = partition.start,
        first, last, "partition indexing started"
    );

    let wall_start = Instant::now();
    let mut stats = PartitionStats::default();

    // Open the partition writer once for the whole loop. It holds the
    // 14 long-lived `Insert<RowT>` handles open across every ledger in
    // the range so
    // the server only sees one INSERT per table per partition (see
    // `db_clickhouse::persist::writer::PartitionWriter` docs).
    let mut writer = sink.open_partition();
    let loop_result: Result<(), BackfillError> = async {
        for seq in first..=last {
            let path = partition.local_ledger_path(seq, temp_dir);
            if completed.contains(&seq) {
                // Already in the DB, so its file is as releasable as one we
                // just indexed — and on the resume path it is most of the
                // partition. Skipping the unlink here would leave a
                // re-synced partial partition sitting at nearly full size.
                drop_indexed_file(&path, keep_files, partition.start, seq).await;
                stats.skipped_completed += 1;
                continue;
            }
            assert!(
                path.exists(),
                "ledger file missing post-sync: partition={} seq={} path={}",
                partition.start,
                seq,
                path.display()
            );
            let t = ingest_ledger_from_file(&path, &mut writer, seq, partition.start).await?;
            drop_indexed_file(&path, keep_files, partition.start, seq).await;
            stats.indexed += 1;
            stats.total_bytes += t.bytes as u64;
            stats.parse_total_ms += t.parse_ms;
            stats.persist_total_ms += t.persist_ms;

            let ledger_ms = t.total_ms();
            stats.min_ledger_ms = Some(stats.min_ledger_ms.map_or(ledger_ms, |m| m.min(ledger_ms)));
            stats.max_ledger_ms = Some(stats.max_ledger_ms.map_or(ledger_ms, |m| m.max(ledger_ms)));
            dashboard.record_ledger(t.parse_ms, t.persist_ms);
        }
        Ok(())
    }
    .await;

    match loop_result {
        Ok(()) => writer.commit().await?,
        Err(err) => {
            // Abort the writer first — drops in-flight CH inserts cleanly.
            writer.abort().await;
            return Err(err);
        }
    }

    stats.wall_clock = wall_start.elapsed();
    let wall_s = stats.wall_clock.as_secs_f64().max(0.001);
    let throughput = stats.indexed as f64 / wall_s;

    info!(
        partition = partition.start,
        indexed = stats.indexed,
        skipped_completed = stats.skipped_completed,
        total_bytes = stats.total_bytes,
        parse_total_ms = stats.parse_total_ms,
        persist_total_ms = stats.persist_total_ms,
        min_ledger_ms = stats.min_ledger_ms.unwrap_or(0),
        max_ledger_ms = stats.max_ledger_ms.unwrap_or(0),
        wall_clock_secs = format!("{:.1}", wall_s),
        throughput = format!("{:.2} ledgers/s", throughput),
        "partition indexing complete"
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flag's polarity is the one thing that can silently go wrong here:
    /// inverted, a run either keeps every file (the disk-full incident that
    /// task 0488 came from) or deletes the files `--keep-partitions` exists
    /// to preserve.
    #[tokio::test]
    async fn indexed_files_go_unless_keep_partitions_asked_for_them() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let dir = tmp.path();

        let dropped = dir.join("dropped.xdr.zst");
        tokio::fs::write(&dropped, b"x").await.expect("write");
        drop_indexed_file(&dropped, false, 64_000, 64_001).await;
        assert!(!dropped.exists(), "indexed file must be released");

        let kept = dir.join("kept.xdr.zst");
        tokio::fs::write(&kept, b"x").await.expect("write");
        drop_indexed_file(&kept, true, 64_000, 64_002).await;
        assert!(kept.exists(), "--keep-partitions must still keep it");

        // A file that is already gone is not an error worth failing on.
        drop_indexed_file(&dropped, false, 64_000, 64_001).await;
    }
}
