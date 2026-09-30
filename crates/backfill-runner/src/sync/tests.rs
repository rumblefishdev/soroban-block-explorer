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
