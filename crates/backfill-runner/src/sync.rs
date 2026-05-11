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

use std::path::Path;
use std::time::{Duration, Instant};

use tokio::process::Command;
use tracing::{info, warn};

use crate::error::BackfillError;
use crate::partition::{PARTITION_SIZE, Partition};

// Retry policy for the `aws s3 sync` subprocess (task 0145 decision).
// Hardcoded — not operator-tunable; change the constants if the numbers drift.
const RETRY_ATTEMPTS: u32 = 3;
const RETRY_BASE_DELAY: Duration = Duration::from_secs(2);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(30);
const RETRY_MULTIPLIER: u32 = 2;

/// Sync one partition from S3 to `temp_dir`. Idempotent by virtue of
/// `aws s3 sync` itself — a second call over a fully-synced dir is a
/// cheap LIST with no GETs. Sync duration is surfaced via the
/// `partition sync complete` tracing event; no return value carries it.
pub async fn sync_partition(partition: &Partition, temp_dir: &Path) -> Result<(), BackfillError> {
    let local = partition.local_folder(temp_dir);
    tokio::fs::create_dir_all(&local).await?;

    // Fast path: if the local folder already holds the full partition
    // (PARTITION_SIZE `.xdr.zst` files), skip the `aws s3 sync`
    // subprocess entirely. Public-archive partitions are immutable
    // once closed, so a complete local snapshot is authoritative.
    //
    // Why this matters: the LIST half of `aws s3 sync` paginates 64
    // ListObjectsV2 calls (~30–40 s) even when zero GETs follow. For
    // iteration loops (`--keep-partitions`) that's pure overhead.
    if let Some((file_count, total_bytes)) = local_partition_complete(&local).await? {
        info!(
            partition = partition.start,
            file_count,
            total_bytes,
            "partition local folder already complete — skipping aws s3 sync"
        );
        return Ok(());
    }

    let duration = run_sync_with_retry(partition, &local).await?;
    let (file_count, total_bytes) = dir_stats(&local).await?;

    info!(
        partition = partition.start,
        sync_duration_ms = duration.as_millis(),
        file_count,
        total_bytes,
        "partition sync complete"
    );

    Ok(())
}

/// Cheap pre-check: if `local` already contains exactly `PARTITION_SIZE`
/// `.xdr.zst` files, return their `(count, total_bytes)` so the caller
/// can short-circuit the `aws s3 sync` subprocess. Returns `None` for
/// anything else (missing dir, partial dir, extra files) — the safe
/// default is "run the sync".
///
/// Safety: public-archive partitions are immutable once their end
/// ledger is written, so file-count parity with `PARTITION_SIZE` is
/// sufficient to declare the local snapshot authoritative for the
/// closed partitions a backfill covers. The "current" (in-progress)
/// partition cannot match this check by construction.
async fn local_partition_complete(dir: &Path) -> Result<Option<(usize, u64)>, BackfillError> {
    count_complete_partition(dir, PARTITION_SIZE as usize).await
}

/// Inner generic helper exposed for unit tests so they can exercise the
/// completeness logic against a small fixture without materialising
/// 64 000 files on disk per test. Production calls go through
/// `local_partition_complete` which fixes `expected = PARTITION_SIZE`.
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
mod tests {
    //! Stage A behavior tests — no network, no subprocess.
    //!
    //! The retry loop and the subprocess wiring are exercised end-to-end in
    //! the staging dry-run (task 0145 plan, Step 8). Here we lock the retry
    //! constants against the spec so drift is a compile-less signal.
    use super::*;

    #[test]
    fn retry_constants_match_spec() {
        // Lock in the numbers called out in task 0145: 3 attempts, 2s base,
        // ×2, 30s cap. Drift here is a silent regression of the operator
        // contract.
        assert_eq!(RETRY_ATTEMPTS, 3);
        assert_eq!(RETRY_BASE_DELAY, Duration::from_secs(2));
        assert_eq!(RETRY_MAX_DELAY, Duration::from_secs(30));
        assert_eq!(RETRY_MULTIPLIER, 2);
    }

    /// Fixture helper: create `count` `.xdr.zst` files of `bytes_each`
    /// bytes in a fresh tempdir and return its path. Also drops one
    /// non-ledger file so the filter in `count_complete_partition`
    /// actually has something to ignore.
    async fn fixture_dir(count: usize, bytes_each: usize) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create tempdir");
        for i in 0..count {
            let path = dir.path().join(format!("file-{i:08X}.xdr.zst"));
            tokio::fs::write(&path, vec![0u8; bytes_each])
                .await
                .expect("write fixture file");
        }
        // Non-ledger sibling — must be ignored by the .xdr.zst filter.
        tokio::fs::write(dir.path().join("README.txt"), b"ignore me")
            .await
            .expect("write sibling");
        dir
    }

    #[tokio::test]
    async fn local_partition_complete_returns_none_for_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let got = count_complete_partition(&missing, 3)
            .await
            .expect("missing dir is Ok(None), not an error");
        assert!(
            got.is_none(),
            "missing dir should produce None, got {got:?}"
        );
    }

    #[tokio::test]
    async fn local_partition_complete_returns_none_for_partial_dir() {
        let dir = fixture_dir(2, 7).await; // expected=3, have 2 → partial
        let got = count_complete_partition(dir.path(), 3).await.unwrap();
        assert!(
            got.is_none(),
            "partial dir should produce None, got {got:?}"
        );
    }

    #[tokio::test]
    async fn local_partition_complete_returns_some_for_exact_count() {
        let dir = fixture_dir(3, 17).await;
        let got = count_complete_partition(dir.path(), 3)
            .await
            .expect("readable dir")
            .expect("exact count yields Some");
        assert_eq!(got.0, 3, "file count");
        assert_eq!(got.1, 3 * 17, "summed bytes");
    }

    #[tokio::test]
    async fn local_partition_complete_returns_none_when_extra_files_push_over_count() {
        // 4 .xdr.zst files when only 3 expected → over → None.
        let dir = fixture_dir(4, 5).await;
        let got = count_complete_partition(dir.path(), 3).await.unwrap();
        assert!(
            got.is_none(),
            "extra files should produce None, got {got:?}"
        );
    }
}
