//! Stellar public-archive S3 partition math.
//!
//! Layout: `{prefix}/{HEX}--{start}-{end}/{HEX}--{seq}.xdr.zst`, where
//! `prefix` is the network's folder (`v1.1/stellar/ledgers/pubnet` unless
//! `PUBLIC_ARCHIVE_PREFIX` says otherwise — see `xdr_parser::public_archive`),
//! `HEX = uppercase_hex(u32::MAX - seq_or_start)` zero-padded to 8 chars,
//! and each partition folder holds exactly `PARTITION_SIZE` ledgers — except
//! the genesis partition, which begins at `FIRST_CLOSED_LEDGER`.

use std::path::{Path, PathBuf};

pub const BUCKET: &str = xdr_parser::public_archive::PUBLIC_BUCKET;
pub const PARTITION_SIZE: u32 = 64_000;
/// Ledgers 0 and 1 have no close meta on any network, so no archive holds them.
pub const FIRST_CLOSED_LEDGER: u32 = 2;

/// S3 partition folder covering a given ledger sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    pub start: u32,
    pub end: u32,
    pub hex: String,
}

impl Partition {
    pub fn from_ledger(seq: u32) -> Self {
        let start = seq - (seq % PARTITION_SIZE);
        let end = start + PARTITION_SIZE - 1;
        let hex = format!("{:08X}", u32::MAX - start);
        Self { start, end, hex }
    }

    /// First ledger this partition holds: its `start`, or `FIRST_CLOSED_LEDGER` in
    /// the genesis partition.
    pub fn first_ledger(&self) -> u32 {
        self.start.max(FIRST_CLOSED_LEDGER)
    }

    /// Number of `.xdr.zst` files a complete copy of this partition holds.
    pub fn ledger_count(&self) -> usize {
        (self.end - self.first_ledger() + 1) as usize
    }

    /// S3 key prefix (no bucket, no scheme, no trailing slash):
    /// `v1.1/stellar/ledgers/pubnet/FC4DB5FF--62016000-62079999`.
    pub fn folder_key(&self) -> String {
        format!(
            "{}/{}--{}-{}",
            xdr_parser::public_archive::public_archive_prefix(),
            self.hex,
            self.start,
            self.end
        )
    }

    /// Full `s3://` URL for the partition folder, suitable as the source
    /// argument to `aws s3 sync`. Trailing slash is intentional — the AWS
    /// CLI treats it as "sync directory contents" rather than "sync one
    /// object".
    pub fn s3_folder(&self) -> String {
        format!("s3://{BUCKET}/{}/", self.folder_key())
    }

    /// Local directory where this partition's `.xdr.zst` files land after
    /// `aws s3 sync` — `{temp_dir}/{HEX}--{start}-{end}`.
    ///
    /// Directory name intentionally matches the S3 folder name so an
    /// operator can `ls` the temp dir and immediately see which partition
    /// each dir represents.
    pub fn local_folder(&self, temp_dir: &Path) -> PathBuf {
        temp_dir.join(format!("{}--{}-{}", self.hex, self.start, self.end))
    }

    /// Intersect this partition's `[first_ledger, end]` with a run's requested
    /// `[run_start, run_end]`. Returned bounds are inclusive.
    ///
    /// A partition at either edge of the run range may only partially
    /// overlap it — every site that loops ledgers or checks "is this
    /// partition fully in the DB" needs these clamped bounds. Centralized
    /// here so the inclusive math isn't duplicated (and re-debugged) at
    /// each call site.
    ///
    /// Caller must ensure `run_start <= run_end` and the partition at
    /// least partially overlaps the run range; otherwise the returned
    /// pair may have `first > last`.
    pub fn clamped(&self, run_start: u32, run_end: u32) -> (u32, u32) {
        (run_start.max(self.first_ledger()), run_end.min(self.end))
    }

    /// Local filesystem path for a single ledger within this partition's
    /// local folder: `{temp_dir}/{HEX}--{start}-{end}/{HEX}--{seq}.xdr.zst`.
    ///
    /// Filename layout mirrors the S3 layout 1:1 so `aws s3 sync` produces
    /// exactly these paths without transformation.
    pub fn local_ledger_path(&self, seq: u32, temp_dir: &Path) -> PathBuf {
        let file_hex = format!("{:08X}", u32::MAX - seq);
        self.local_folder(temp_dir)
            .join(format!("{file_hex}--{seq}.xdr.zst"))
    }
}

/// Enumerate the partitions covering `[start, end]` inclusive, in
/// ascending sequence order. Returns an empty `Vec` if `start > end`.
///
/// Each returned partition may overflow the requested range at its edges
/// — the caller is expected to clamp per-ledger iteration to `[start, end]`.
pub fn partitions_for_range(start: u32, end: u32) -> Vec<Partition> {
    let mut result = Vec::new();
    if start > end {
        return result;
    }
    let mut cursor = start;
    while cursor <= end {
        let p = Partition::from_ledger(cursor);
        let next = p.end + 1;
        result.push(p);
        cursor = next;
    }
    result
}

#[cfg(test)]
mod tests;
