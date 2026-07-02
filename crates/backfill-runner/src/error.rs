//! Typed errors for the backfill runner.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackfillError {
    /// Pool initialization failed — not an indexer error, a config/boot error.
    #[error("database pool init: {0}")]
    Db(#[from] sqlx::Error),

    /// ClickHouse client / query failure (task 0205, `--target clickhouse`).
    #[error("clickhouse: {0}")]
    Ch(#[from] clickhouse::error::Error),

    /// ClickHouse stub persist failure (task 0205). The stub itself only
    /// returns `Ok`, but the type is wired in so the follow-up that lands
    /// real INSERTs doesn't widen `BackfillError` again.
    #[error("clickhouse persist: {0}")]
    ChPersist(#[from] db_clickhouse::SchemaError),

    #[error("indexer: {0}")]
    Indexer(#[from] indexer::handler::HandlerError),

    /// Soroban RPC failure during the `upgradeable-backfill` WASM fetch
    /// (task 0327). Constructed via `#[from]`.
    #[error("rpc: {0}")]
    Rpc(#[from] crate::rpc_snapshot::RpcError),

    /// A backfill pass could not resolve every target it set out to (task 0327
    /// `upgradeable-backfill` hard-fails rather than silently leaving gaps).
    #[error("backfill incomplete: {0}")]
    Incomplete(String),

    /// Local filesystem / subprocess I/O failure (create_dir_all, File::create,
    /// read_dir, spawning `aws`, etc.). Constructed via `#[from]`.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// `aws s3 sync` subprocess exited non-zero after all retry attempts were
    /// exhausted. Carries enough context (partition + exit code + stderr tail)
    /// to diagnose without re-running.
    #[error("aws s3 sync failed for partition {partition} (exit {exit_code}): {stderr}")]
    AwsSyncFailed {
        partition: u32,
        exit_code: i32,
        stderr: String,
    },

    /// Task 0225: post-sync local file count is below `PARTITION_SIZE`
    /// despite S3 reporting the partition as fully archived. Re-sync
    /// did not fix it. Distinct from `AwsSyncFailed` because the
    /// subprocess itself returned exit 0 — the failure is on our side
    /// (disk full, permissions, network glitch that AWS CLI swallowed,
    /// etc.).
    #[error(
        "partition {partition_start} sync incomplete after retry: local={local} s3={s3} need={need}"
    )]
    PartitionSyncFailed {
        partition_start: u32,
        local: usize,
        s3: usize,
        need: usize,
    },

    /// Task 0225: `aws s3 ls --recursive` failed when probing whether
    /// the partition is fully archived on S3 (used to disambiguate
    /// "S3 archive lag" from "local sync failed"). Operator must
    /// investigate — we don't fall back to "assume complete" because
    /// that just defers the panic to `ingest.rs`.
    #[error("aws s3 ls failed for partition {partition_start}: {source}")]
    S3LsFailed {
        partition_start: u32,
        #[source]
        source: std::io::Error,
    },
}
