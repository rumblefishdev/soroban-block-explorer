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
//! count against `PARTITION_SIZE` and probes S3 directly via
//! [`S3Driver::object_count`] to disambiguate "S3 archive lag" (skip
//! gracefully) from "local sync failed despite full S3" (retry, then
//! error). See task 0225 + [`SyncOutcome`].

use std::path::Path;
use std::time::{Duration, Instant};

use async_trait::async_trait;
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

/// Outcome of [`sync_partition`].
///
/// - [`SyncOutcome::Complete`] — local folder has exactly
///   `PARTITION_SIZE` `.xdr.zst` files; safe to index.
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
/// Post-sync the local file count is validated against `PARTITION_SIZE`:
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

    // Fast path: if the local folder already holds the full partition
    // (PARTITION_SIZE `.xdr.zst` files), skip the `aws s3 sync`
    // subprocess entirely. Public-archive partitions are immutable
    // once closed, so a complete local snapshot is authoritative.
    if let Some((file_count, total_bytes)) = local_partition_complete(&local).await? {
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

    if file_count == PARTITION_SIZE as usize {
        info!(
            partition = partition.start,
            sync_duration_ms = duration.as_millis(),
            file_count,
            total_bytes,
            "partition sync complete"
        );
        return Ok(SyncOutcome::Complete);
    }

    // Post-sync count is short of `PARTITION_SIZE`. Probe S3 to learn
    // whether the gap is upstream (archive lag) or local (network
    // glitch / disk issue that AWS CLI swallowed silently).
    let s3_count = driver.object_count(partition).await?;
    if s3_count < PARTITION_SIZE as usize {
        warn!(
            partition = partition.start,
            local_files = file_count,
            s3_files = s3_count,
            need = PARTITION_SIZE,
            "S3 archive lag — partition incomplete on S3, skipping; rerun \
             this range once the upstream archive catches up"
        );
        return Ok(SyncOutcome::S3Incomplete {
            local: file_count,
            s3: s3_count,
            need: PARTITION_SIZE as usize,
        });
    }

    // S3 has the full partition but our local copy is short. Retry once
    // — fresh subprocess, fresh internal `aws s3 sync` retry budget.
    warn!(
        partition = partition.start,
        local_files = file_count,
        s3_files = s3_count,
        need = PARTITION_SIZE,
        "local sync partial despite full S3 — retrying"
    );
    driver.sync_partition_files(partition, &local).await?;
    let (file_count_retry, total_bytes_retry) = dir_stats(&local).await?;
    if file_count_retry == PARTITION_SIZE as usize {
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
        need: PARTITION_SIZE as usize,
    })
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

    // -------------------------------------------------------------------
    // Task 0225 — orchestrator decision-tree tests
    //
    // Covers `sync_partition`'s four outcomes (Complete, S3Incomplete,
    // PartitionSyncFailed, S3LsFailed) via a hand-rolled MockS3Driver.
    // No AWS CLI, no network. Each test fabricates files in the partition
    // local folder to match the queued sync step, then asserts the
    // outcome + call counts.
    //
    // Tests fabricate exactly `PARTITION_SIZE` files for "Complete"
    // outcomes (64 000 empty 0-byte files; ~1 s on tmpfs/APFS). This is
    // a deliberate accept since the orchestrator's validation compares
    // against the global `PARTITION_SIZE` constant — there's no
    // per-test target.
    // -------------------------------------------------------------------
    use std::sync::Mutex as StdMutex;

    /// Queued behaviour for the next [`S3Driver::sync_partition_files`] call.
    enum SyncStep {
        /// Fabricate `count` empty `.xdr.zst` files in `local`.
        Writes(usize),
        /// Return `Err(AwsSyncFailed)` — the subprocess "failed".
        #[allow(dead_code)]
        Fails,
    }

    /// Queued behaviour for the next [`S3Driver::object_count`] call.
    enum LsStep {
        /// Return this canned count.
        Returns(usize),
        /// Return `Err(S3LsFailed)`.
        Fails,
    }

    #[derive(Default)]
    struct MockState {
        sync_queue: Vec<SyncStep>,
        ls_queue: Vec<LsStep>,
        sync_calls: usize,
        ls_calls: usize,
    }

    struct MockS3Driver {
        state: StdMutex<MockState>,
    }

    impl MockS3Driver {
        fn new(sync_queue: Vec<SyncStep>, ls_queue: Vec<LsStep>) -> Self {
            Self {
                state: StdMutex::new(MockState {
                    sync_queue,
                    ls_queue,
                    sync_calls: 0,
                    ls_calls: 0,
                }),
            }
        }

        fn sync_calls(&self) -> usize {
            self.state.lock().unwrap().sync_calls
        }

        fn ls_calls(&self) -> usize {
            self.state.lock().unwrap().ls_calls
        }
    }

    #[async_trait]
    impl S3Driver for MockS3Driver {
        async fn sync_partition_files(
            &self,
            partition: &Partition,
            local: &std::path::Path,
        ) -> Result<(), BackfillError> {
            let step = {
                let mut state = self.state.lock().unwrap();
                state.sync_calls += 1;
                assert!(
                    !state.sync_queue.is_empty(),
                    "MockS3Driver: sync_partition_files called more times than queued"
                );
                state.sync_queue.remove(0)
            };
            match step {
                SyncStep::Writes(count) => {
                    fabricate_files(local, count).await;
                    Ok(())
                }
                SyncStep::Fails => Err(BackfillError::AwsSyncFailed {
                    partition: partition.start,
                    exit_code: 1,
                    stderr: "mock sync failure".into(),
                }),
            }
        }

        async fn object_count(&self, partition: &Partition) -> Result<usize, BackfillError> {
            let step = {
                let mut state = self.state.lock().unwrap();
                state.ls_calls += 1;
                assert!(
                    !state.ls_queue.is_empty(),
                    "MockS3Driver: object_count called more times than queued"
                );
                state.ls_queue.remove(0)
            };
            match step {
                LsStep::Returns(count) => Ok(count),
                LsStep::Fails => Err(BackfillError::S3LsFailed {
                    partition_start: partition.start,
                    source: std::io::Error::other("mock ls failure"),
                }),
            }
        }
    }

    async fn fabricate_files(local: &std::path::Path, count: usize) {
        tokio::fs::create_dir_all(local)
            .await
            .expect("create local dir");
        // Count existing `.xdr.zst` files so a retry call APPENDS new
        // ones rather than overwriting earlier indices. Without this
        // the retry-success test silently regresses to a no-op.
        let mut existing = 0usize;
        let mut entries = tokio::fs::read_dir(local).await.expect("read local dir");
        while let Some(entry) = entries.next_entry().await.expect("next entry") {
            if entry.file_name().to_string_lossy().ends_with(".xdr.zst") {
                existing += 1;
            }
        }
        for i in 0..count {
            let idx = existing + i;
            let path = local.join(format!("mock-{idx:08X}.xdr.zst"));
            tokio::fs::write(&path, b"")
                .await
                .expect("write fixture file");
        }
    }

    fn test_partition() -> Partition {
        // Literal Soroban-era start ledger so the partition's S3 + local
        // folder names are realistic. The orchestrator never reaches the
        // network when the driver is mocked.
        Partition::from_ledger(50_457_424)
    }

    #[tokio::test]
    async fn sync_complete_happy_path() {
        let tmp = tempfile::tempdir().unwrap();
        let partition = test_partition();
        let driver = MockS3Driver::new(
            vec![SyncStep::Writes(PARTITION_SIZE as usize)],
            vec![/* ls never called */],
        );

        let outcome = sync_partition(&driver, &partition, tmp.path())
            .await
            .expect("happy path returns Ok");

        assert_eq!(outcome, SyncOutcome::Complete);
        assert_eq!(driver.sync_calls(), 1, "exactly one sync call");
        assert_eq!(
            driver.ls_calls(),
            0,
            "no S3 ls when sync wrote a full partition"
        );

        // Sanity: the local dir contains PARTITION_SIZE files.
        let local = partition.local_folder(tmp.path());
        let entries = std::fs::read_dir(&local).unwrap().count();
        assert_eq!(entries, PARTITION_SIZE as usize);
    }

    #[tokio::test]
    async fn s3_incomplete_skips_gracefully() {
        let tmp = tempfile::tempdir().unwrap();
        let partition = test_partition();
        let partial = (PARTITION_SIZE as usize) * 7 / 10; // 70 % of partition

        let driver = MockS3Driver::new(
            vec![SyncStep::Writes(partial)],
            vec![LsStep::Returns(partial)],
        );

        let outcome = sync_partition(&driver, &partition, tmp.path())
            .await
            .expect("graceful skip is Ok");

        match outcome {
            SyncOutcome::S3Incomplete { local, s3, need } => {
                assert_eq!(local, partial);
                assert_eq!(s3, partial);
                assert_eq!(need, PARTITION_SIZE as usize);
            }
            other => panic!("expected S3Incomplete, got {other:?}"),
        }
        assert_eq!(driver.sync_calls(), 1);
        assert_eq!(driver.ls_calls(), 1);
    }

    #[tokio::test]
    async fn local_partial_s3_complete_retries() {
        let tmp = tempfile::tempdir().unwrap();
        let partition = test_partition();
        let partial = (PARTITION_SIZE as usize) * 7 / 10;

        let driver = MockS3Driver::new(
            // First sync writes 70 %; retry writes the remaining 30 %
            // (cumulative file set reaches 100 %).
            vec![
                SyncStep::Writes(partial),
                SyncStep::Writes(PARTITION_SIZE as usize - partial),
            ],
            vec![LsStep::Returns(PARTITION_SIZE as usize)],
        );

        let outcome = sync_partition(&driver, &partition, tmp.path())
            .await
            .expect("retry success is Ok");

        assert_eq!(outcome, SyncOutcome::Complete);
        assert_eq!(driver.sync_calls(), 2, "first sync + one retry");
        assert_eq!(driver.ls_calls(), 1, "exactly one S3 ls probe");
    }

    #[tokio::test]
    async fn retry_doesnt_fix_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let partition = test_partition();
        let partial = (PARTITION_SIZE as usize) * 7 / 10;

        let driver = MockS3Driver::new(
            // Both attempts write only 70 % cumulatively — retry adds 0.
            vec![SyncStep::Writes(partial), SyncStep::Writes(0)],
            vec![LsStep::Returns(PARTITION_SIZE as usize)],
        );

        let err = sync_partition(&driver, &partition, tmp.path())
            .await
            .expect_err("retry that doesn't help is hard error");

        match err {
            BackfillError::PartitionSyncFailed {
                partition_start,
                local,
                s3,
                need,
            } => {
                assert_eq!(partition_start, partition.start);
                assert_eq!(local, partial);
                assert_eq!(s3, PARTITION_SIZE as usize);
                assert_eq!(need, PARTITION_SIZE as usize);
            }
            other => panic!("expected PartitionSyncFailed, got {other:?}"),
        }
        assert_eq!(driver.sync_calls(), 2);
        assert_eq!(driver.ls_calls(), 1);
    }

    #[tokio::test]
    async fn s3_ls_failure_propagates() {
        let tmp = tempfile::tempdir().unwrap();
        let partition = test_partition();
        let partial = (PARTITION_SIZE as usize) * 7 / 10;

        let driver = MockS3Driver::new(vec![SyncStep::Writes(partial)], vec![LsStep::Fails]);

        let err = sync_partition(&driver, &partition, tmp.path())
            .await
            .expect_err("ls failure surfaces");

        match err {
            BackfillError::S3LsFailed {
                partition_start,
                source: _,
            } => {
                assert_eq!(partition_start, partition.start);
            }
            other => panic!("expected S3LsFailed, got {other:?}"),
        }
        assert_eq!(driver.sync_calls(), 1, "no retry attempted after ls fail");
        assert_eq!(driver.ls_calls(), 1);
    }
}
