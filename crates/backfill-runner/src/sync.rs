//! `aws s3 sync` driver for one partition.
//!
//! Unit of work: one whole 64k-ledger partition, downloaded via the AWS CLI
//! into the local temp directory. The CLI subprocess is deliberate — `aws
//! s3 sync` is the right tool here (listing, parallel GETs, dedup, resume
//! on partial downloads), and reimplementing it against `aws-sdk-s3` is
//! not justified (see ADR context in task 0145).
//!
//! Stage A resume — there is no marker, no manifest, no file-count check.
//! `aws s3 sync` is **itself idempotent**: a second call against an already
//! complete local dir is a LIST + no GETs (seconds, not minutes). So we
//! just always run it. If the previous run crashed mid-sync, the partial
//! dir gets filled in by the next sync on its own. The real resume filter
//! for duplicate work lives in Stage B (the `ledgers` table).
//!
//! ## Task 0225 — S3 archive lag detection
//!
//! `aws s3 sync` against a partition that's still being uploaded to the
//! public archive silently returns exit 0 with a partial local copy.
//! Indexing then panics on the first missing local file (`ingest.rs:176`).
//! To prevent that, [`sync_partition`] now post-validates the local file
//! count against [`Partition::ledger_count`] and probes S3 directly via
//! [`S3Driver::object_count`] to disambiguate "S3 archive lag" (skip
//! gracefully) from "local sync failed despite full S3" (retry, then
//! error). See task 0225 + [`SyncOutcome`].

use std::path::Path;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::process::Command;
use tracing::{info, warn};

use crate::error::BackfillError;
use crate::partition::Partition;

// Retry policy for the `aws s3 sync` subprocess (task 0145 decision).
// Hardcoded — not operator-tunable; change the constants if the numbers drift.
const RETRY_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
const RETRY_MULTIPLIER: u32 = 2;

/// Outcome of [`sync_partition`].
///
/// - [`SyncOutcome::Complete`] — local folder has exactly
///   [`Partition::ledger_count`] `.xdr.zst` files; safe to index.
/// - [`SyncOutcome::S3Incomplete`] — S3 itself is missing files for this
///   partition (archive lag for the live tail of the chain). Caller
///   should log + skip. Operator reruns the range once S3 catches up.
///
/// A genuine local sync failure (S3 complete but local stayed partial
/// after a retry) is reported via `Err(BackfillError::PartitionSyncFailed)`
/// — the partition contract is "either Complete on disk or hard error",
/// the [`SyncOutcome`] surface only carries the operationally-skippable
/// case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    /// Local folder is fully populated; proceed to indexing.
    Complete,
    /// S3 archive lag; partition is incomplete upstream. Skip + warn.
    S3Incomplete {
        local: usize,
        s3: usize,
        need: usize,
    },
}

/// Driver abstraction over the two S3 operations [`sync_partition`]
/// needs: pull a partition's objects to disk, and count objects on S3
/// without downloading them. Lifted to a trait so the orchestrator is
/// unit-testable against a `MockS3Driver` without spawning `aws s3 …`
/// subprocesses.
///
/// The production impl is [`AwsCliS3Driver`] — wraps the existing
/// `aws s3 sync` + `aws s3 ls` invocations. Tests use a hand-rolled
/// mock that records calls and fabricates files / canned counts.
// `async_trait` marks each generated method `#[must_use]` while returning a
// boxed future, which is already must-use; clippy 1.99 flags the pair.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait S3Driver: Send + Sync {
    /// Pull the partition's objects into `local`. Returns `Ok` on
    /// exit 0; retries inside the impl are an implementation detail
    /// (the orchestrator may also retry at the [`SyncOutcome`] layer).
    async fn sync_partition_files(
        &self,
        partition: &Partition,
        local: &Path,
    ) -> Result<(), BackfillError>;

    /// Count `.xdr.zst` objects under the partition's S3 prefix
    /// without downloading. Used by [`sync_partition`] to distinguish
    /// "S3 archive lag" from "local sync failed".
    async fn object_count(&self, partition: &Partition) -> Result<usize, BackfillError>;
}

/// Production [`S3Driver`] backed by the AWS CLI (`aws s3 sync` and
/// `aws s3 ls --recursive`). Anonymous credentials
/// (`--no-sign-request`) — the public Stellar archive doesn't require
/// auth.
pub struct AwsCliS3Driver;

#[async_trait]
impl S3Driver for AwsCliS3Driver {
    async fn sync_partition_files(
        &self,
        partition: &Partition,
        local: &Path,
    ) -> Result<(), BackfillError> {
        run_sync_with_retry(partition, local).await.map(|_| ())
    }

    async fn object_count(&self, partition: &Partition) -> Result<usize, BackfillError> {
        let s3 = partition.s3_folder();
        let output = Command::new("aws")
            .arg("s3")
            .arg("ls")
            .arg("--recursive")
            .arg("--no-sign-request")
            .arg(&s3)
            .output()
            .await
            .map_err(|source| BackfillError::S3LsFailed {
                partition_start: partition.start,
                source,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BackfillError::S3LsFailed {
                partition_start: partition.start,
                source: std::io::Error::other(format!(
                    "aws s3 ls exited {:?}: {}",
                    output.status.code(),
                    stderr.chars().take(500).collect::<String>(),
                )),
            });
        }

        // Each output line looks like:
        //   2024-02-21 10:42:18  187234  FCFC83FF--50560000-50623999/FCFC83FF--50560000.xdr.zst
        // Count lines whose last whitespace-separated token ends in
        // `.xdr.zst`. Robust against future column additions.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let count = stdout
            .lines()
            .filter(|line| {
                line.split_whitespace()
                    .next_back()
                    .is_some_and(|tok| tok.ends_with(".xdr.zst"))
            })
            .count();
        Ok(count)
    }
}

/// Sync one partition from S3 to `temp_dir`. Idempotent by virtue of
/// `aws s3 sync` itself — a second call over a fully-synced dir is a
/// cheap LIST with no GETs.
///
/// Post-sync the local file count is validated against
/// [`Partition::ledger_count`] — `PARTITION_SIZE`, fewer in the genesis
/// partition:
///
/// - **Match** → [`SyncOutcome::Complete`]. Safe to index.
/// - **Local partial, S3 complete** → retry sync once. Still partial →
///   `Err(BackfillError::PartitionSyncFailed)`.
/// - **Local partial, S3 also partial** → [`SyncOutcome::S3Incomplete`].
///   Caller logs + skips. Operator reruns once S3 catches up.
///
/// `aws s3 ls` failure → `Err(BackfillError::S3LsFailed)`. We do **not**
/// fall back to "assume complete" — that would just defer the panic
/// to `ingest.rs:176`. Operator must investigate.
pub async fn sync_partition(
    driver: &dyn S3Driver,
    partition: &Partition,
    temp_dir: &Path,
) -> Result<SyncOutcome, BackfillError> {
    let local = partition.local_folder(temp_dir);
    tokio::fs::create_dir_all(&local).await?;

    let need = partition.ledger_count();

    // Fast path: if the local folder already holds the full partition
    // (`need` `.xdr.zst` files), skip the `aws s3 sync`
    // subprocess entirely. Public-archive partitions are immutable
    // once closed, so a complete local snapshot is authoritative.
    if let Some((file_count, total_bytes)) = local_partition_complete(&local, need).await? {
        info!(
            partition = partition.start,
            file_count,
            total_bytes,
            "partition local folder already complete — skipping aws s3 sync"
        );
        return Ok(SyncOutcome::Complete);
    }

    // First sync attempt.
    let start = Instant::now();
    driver.sync_partition_files(partition, &local).await?;
    let duration = start.elapsed();
    let (file_count, total_bytes) = dir_stats(&local).await?;

    if file_count == need {
        info!(
            partition = partition.start,
            sync_duration_ms = duration.as_millis(),
            file_count,
            total_bytes,
            "partition sync complete"
        );
        return Ok(SyncOutcome::Complete);
    }

    // Post-sync count is short of `need`. Probe S3 to learn
    // whether the gap is upstream (archive lag) or local (network
    // glitch / disk issue that AWS CLI swallowed silently).
    let s3_count = driver.object_count(partition).await?;
    if s3_count < need {
        warn!(
            partition = partition.start,
            local_files = file_count,
            s3_files = s3_count,
            need,
            "S3 archive lag — partition incomplete on S3, skipping; rerun \
             this range once the upstream archive catches up"
        );
        return Ok(SyncOutcome::S3Incomplete {
            local: file_count,
            s3: s3_count,
            need,
        });
    }

    // S3 has the full partition but our local copy is short. Retry once
    // — fresh subprocess, fresh internal `aws s3 sync` retry budget.
    warn!(
        partition = partition.start,
        local_files = file_count,
        s3_files = s3_count,
        need,
        "local sync partial despite full S3 — retrying"
    );
    driver.sync_partition_files(partition, &local).await?;
    let (file_count_retry, total_bytes_retry) = dir_stats(&local).await?;
    if file_count_retry == need {
        info!(
            partition = partition.start,
            file_count = file_count_retry,
            total_bytes = total_bytes_retry,
            "partition sync complete after retry"
        );
        return Ok(SyncOutcome::Complete);
    }

    Err(BackfillError::PartitionSyncFailed {
        partition_start: partition.start,
        local: file_count_retry,
        s3: s3_count,
        need,
    })
}

/// Cheap pre-check: if `local` already contains exactly `need`
/// `.xdr.zst` files, return their `(count, total_bytes)` so the caller
/// can short-circuit the `aws s3 sync` subprocess. Returns `None` for
/// anything else (missing dir, partial dir, extra files) — the safe
/// default is "run the sync".
///
/// Safety: public-archive partitions are immutable once their end
/// ledger is written, so file-count parity with `need` is
/// sufficient to declare the local snapshot authoritative for the
/// closed partitions a backfill covers. The "current" (in-progress)
/// partition cannot match this check by construction.
async fn local_partition_complete(
    dir: &Path,
    need: usize,
) -> Result<Option<(usize, u64)>, BackfillError> {
    count_complete_partition(dir, need).await
}

/// Inner generic helper exposed for unit tests so they can exercise the
/// completeness logic against a small fixture without materialising
/// 64 000 files on disk per test. Production calls go through
/// `local_partition_complete` with the partition's ledger count.
async fn count_complete_partition(
    dir: &Path,
    expected: usize,
) -> Result<Option<(usize, u64)>, BackfillError> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let mut count = 0usize;
    let mut bytes = 0u64;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".xdr.zst") {
            continue;
        }
        let meta = entry.metadata().await?;
        count += 1;
        bytes += meta.len();
    }
    if count == expected {
        Ok(Some((count, bytes)))
    } else {
        Ok(None)
    }
}

/// Run `aws s3 sync` with exponential backoff. Returns the duration of
/// the **successful** attempt — retries are operator-visible via `warn!`
/// events and don't contaminate the reported sync time.
async fn run_sync_with_retry(
    partition: &Partition,
    local: &Path,
) -> Result<Duration, BackfillError> {
    let mut delay = RETRY_BASE_DELAY;
    for attempt in 1..=RETRY_ATTEMPTS {
        let start = Instant::now();
        match run_sync_once(partition, local).await {
            Ok(()) => return Ok(start.elapsed()),
            Err(err) if attempt == RETRY_ATTEMPTS => return Err(err),
            Err(err) => {
                warn!(
                    partition = partition.start,
                    attempt,
                    error = %err,
                    retry_in_secs = delay.as_secs(),
                    "aws s3 sync failed, retrying"
                );
                tokio::time::sleep(delay).await;
                delay = (delay.saturating_mul(RETRY_MULTIPLIER)).min(RETRY_MAX_DELAY);
            }
        }
    }
    unreachable!("retry loop exits via return")
}

/// Spawn one `aws s3 sync` invocation. Returns `Ok(())` on exit 0,
/// `Err(AwsSyncFailed)` on any non-zero exit (caller layers retry).
async fn run_sync_once(partition: &Partition, local: &Path) -> Result<(), BackfillError> {
    let s3 = partition.s3_folder();

    info!(
        partition = partition.start,
        s3 = %s3,
        local = %local.display(),
        "running aws s3 sync"
    );

    let output = Command::new("aws")
        .arg("s3")
        .arg("sync")
        .arg(&s3)
        .arg(local)
        .arg("--no-sign-request")
        .arg("--quiet")
        .output()
        .await?;

    if output.status.success() {
        return Ok(());
    }

    // Trim stderr so the error message stays log-friendly. Full output is
    // already in the subprocess's own streams if `--quiet` is dropped.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stderr.chars().take(2_000).collect::<String>();

    Err(BackfillError::AwsSyncFailed {
        partition: partition.start,
        exit_code: output.status.code().unwrap_or(-1),
        stderr: trimmed,
    })
}

/// Count `.xdr.zst` files and sum their bytes in a synced partition dir.
/// Non-ledger files are ignored.
async fn dir_stats(dir: &Path) -> Result<(usize, u64), BackfillError> {
    let mut entries = tokio::fs::read_dir(dir).await?;
    let mut count = 0usize;
    let mut bytes = 0u64;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".xdr.zst") {
            warn!("skipping non-ledger file: {}", name);
            continue;
        }
        let meta = entry.metadata().await?;
        count += 1;
        bytes += meta.len();
    }
    Ok((count, bytes))
}

#[cfg(test)]
mod tests;
