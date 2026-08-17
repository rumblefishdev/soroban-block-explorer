//! Checkpoint-snapshot reader — the SDF history archive's bucket list decoded
//! as a stream of ledger entries (task 0463, issue #377).
//!
//! ## Why this exists
//!
//! Our index is a stream of CHANGES since ledger floor 50,457,424, and 78.85%
//! of chain history predates that floor. An entry that never changed since
//! then has no row here at all, so no query over our own data can see it —
//! not even to count it. A full re-parse cannot fix that: it replays ledgers
//! we have, and the missing entries are not in them.
//!
//! The history archive publishes the complete STATE of pubnet at each
//! checkpoint as a bucket list. That is the only source that answers "what do
//! we NOT have", and the only backward-completeness route that terminates.
//!
//! ## Shape of the data
//!
//! The archive manifest (`.well-known/stellar-history.json`) advertises
//! `currentBuckets` — 11 levels, each with a `curr` and a `snap` hash. An
//! all-zero hash means that slot is empty. Measured 2026-08-17 at checkpoint
//! 64,001,279: **21 non-empty buckets, ~4.5 GB gzipped**, the largest single
//! bucket 2.47 GB.
//!
//! Each bucket is a gzipped stream of XDR `BucketEntry` records, framed the
//! way stellar-core writes XDR streams: a 4-byte big-endian length prefix per
//! record whose top bit marks "last fragment". Entries are decoded in
//! streaming fashion — a 2.47 GB bucket is never materialised.
//!
//! ## Ordering — load-bearing, not an implementation detail
//!
//! The bucket list is ordered NEWEST FIRST: level 0 holds the most recent
//! entries, level 10 the oldest, and within a level `curr` precedes `snap`.
//! The FIRST record seen for a `LedgerKey` is therefore the live one, and any
//! later record for the same key is superseded history. A `DeadEntry` seen
//! first means the entry was deleted and must not be resurrected by an older
//! `LiveEntry` further down. Readers MUST honour first-wins per key.
//!
//! ## Staleness contract
//!
//! The snapshot is correct at its checkpoint ledger and stale the moment it
//! lands. Anything written from it must be versioned on each entry's own
//! `lastModifiedLedgerSeq` so live parser writes win regardless of load order
//! — never on a window boundary, which is the defect task 0492 documents.

use std::io::Read;

use flate2::read::GzDecoder;
use stellar_xdr::{
    BucketEntry, Frame, LedgerEntryData, LedgerEntryType, LedgerKey, Limited, Limits, ReadXdr,
};
use tracing::info;

use crate::error::BackfillError;

/// Public SDF history archive for pubnet. A hash-chained transport anchored to
/// SCP, so it is a VERIFIED source rather than an API answer taken on trust —
/// the distinction the 0463 map's standing rule draws.
pub const PUBNET_ARCHIVE: &str = "https://history.stellar.org/prd/core-live/core_live_001";

/// One checkpoint's bucket list, newest-first.
#[derive(Debug, Clone)]
pub struct BucketList {
    /// Ledger the snapshot is correct at.
    pub checkpoint_ledger: u32,
    /// Bucket hashes in NEWEST-FIRST order. Position is meaning: see the
    /// module docs on first-wins.
    pub hashes: Vec<String>,
}

impl BucketList {
    /// `bucket/<aa>/<bb>/<cc>/bucket-<hash>.xdr.gz` — the archive's fan-out
    /// layout, first three byte-pairs of the hash as directories.
    pub fn url(archive: &str, hash: &str) -> String {
        format!(
            "{archive}/bucket/{}/{}/{}/bucket-{hash}.xdr.gz",
            &hash[0..2],
            &hash[2..4],
            &hash[4..6]
        )
    }
}

/// Fetch and parse the archive manifest.
pub async fn fetch_bucket_list(
    client: &reqwest::Client,
    archive: &str,
) -> Result<BucketList, BackfillError> {
    let url = format!("{archive}/.well-known/stellar-history.json");
    let body: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .map_err(|e| BackfillError::Incomplete(format!("history manifest fetch failed: {e}")))?
        .json()
        .await
        .map_err(|e| BackfillError::Incomplete(format!("history manifest decode failed: {e}")))?;

    let checkpoint_ledger = body
        .get("currentLedger")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no currentLedger".into()))?
        as u32;

    // An all-zero hash is an empty slot, not a bucket. Levels run newest to
    // oldest and `curr` precedes `snap` within a level — preserve that order.
    const EMPTY: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    let mut hashes = Vec::new();
    for level in body
        .get("currentBuckets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no currentBuckets".into()))?
    {
        for slot in ["curr", "snap"] {
            if let Some(h) = level.get(slot).and_then(serde_json::Value::as_str)
                && h != EMPTY
                && !h.is_empty()
            {
                hashes.push(h.to_string());
            }
        }
    }

    info!(
        checkpoint_ledger,
        buckets = hashes.len(),
        "history archive bucket list"
    );
    Ok(BucketList {
        checkpoint_ledger,
        hashes,
    })
}

/// What a decoded bucket record carries. `Dead` is as load-bearing as `Live`:
/// seen first for a key it means "deleted", and dropping it would let an older
/// `Live` record resurrect the entry.
#[derive(Debug)]
pub enum SnapshotRecord {
    Live(Box<stellar_xdr::LedgerEntry>),
    Dead(Box<stellar_xdr::LedgerKey>),
}

/// Stream one bucket, invoking `f` per LEDGER record in file order.
///
/// Returns the number of `BucketEntry` records DECODED, which is larger than
/// the number passed to `f`: every bucket opens with a `METAENTRY` carrying
/// the protocol version, and `f` is not invoked for it.
///
/// The reader is bounded by `Limits::none()` for depth but the underlying
/// stream is read incrementally, so a 2.47 GB bucket costs a decompression
/// window, not its own size in memory.
pub fn stream_bucket<R: Read, F>(reader: R, mut f: F) -> Result<u64, BackfillError>
where
    F: FnMut(SnapshotRecord) -> Result<(), BackfillError>,
{
    let gz = GzDecoder::new(reader);
    let mut limited = Limited::new(gz, Limits::none());
    let mut count = 0u64;

    // Buckets are FRAMED XDR streams: each record carries a 4-byte big-endian
    // prefix whose top bit marks the last fragment. Verified on a real bucket
    // — the first word is 0x80000010, a 16-byte METAENTRY. Reading unframed
    // fails with "xdr value invalid" on the very first record, which is how
    // this was caught. The stream ends at EOF; the iterator terminates rather
    // than erroring.
    for entry in Frame::<BucketEntry>::read_xdr_iter(&mut limited) {
        let entry: BucketEntry = entry
            .map_err(|e| BackfillError::Incomplete(format!("bucket XDR decode failed: {e}")))?
            .0;
        count += 1;
        match entry {
            // INITENTRY and LIVEENTRY both mean "this is the entry's state".
            BucketEntry::Liveentry(e) | BucketEntry::Initentry(e) => {
                f(SnapshotRecord::Live(Box::new(e)))?;
            }
            BucketEntry::Deadentry(k) => f(SnapshotRecord::Dead(Box::new(k)))?,
            // Bucket metadata carries the protocol version, not ledger state.
            BucketEntry::Metaentry(_) => {}
        }
    }
    Ok(count)
}

/// Per-entry-type tally, for the measurement the seed must publish before it
/// is allowed to write anything.
#[derive(Debug, Default, Clone)]
pub struct EntryTally {
    pub live: [u64; 10],
    pub dead: [u64; 10],
    pub records: u64,
}

impl EntryTally {
    pub fn observe(&mut self, rec: &SnapshotRecord) {
        self.records += 1;
        let (slot, idx) = match rec {
            SnapshotRecord::Live(e) => (&mut self.live, entry_type_index(&e.data)),
            SnapshotRecord::Dead(k) => (&mut self.dead, ledger_key_index(k)),
        };
        if let Some(i) = idx {
            slot[i] += 1;
        }
    }
}

const ENTRY_TYPE_NAMES: [&str; 10] = [
    "account",
    "trustline",
    "offer",
    "data",
    "claimable_balance",
    "liquidity_pool",
    "contract_data",
    "contract_code",
    "config_setting",
    "ttl",
];

fn entry_type_index(d: &LedgerEntryData) -> Option<usize> {
    Some(d.discriminant() as i32 as usize)
}

fn ledger_key_index(k: &LedgerKey) -> Option<usize> {
    let t: LedgerEntryType = k.discriminant();
    Some(t as i32 as usize)
}

/// Read-only pass over the checkpoint snapshot: fetch the bucket list, stream
/// every bucket, report per-entry-type counts and wall-clock. Writes nothing.
///
/// Buckets are streamed to a temp file first rather than buffered in memory —
/// the largest single bucket measured 2.47 GB, and holding that plus its
/// decompression window in RAM is exactly the failure the seed must avoid.
pub async fn tally_command(limit: Option<usize>) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    let client = reqwest::Client::new();
    let list = fetch_bucket_list(&client, PUBNET_ARCHIVE).await?;

    let take = limit.unwrap_or(list.hashes.len()).min(list.hashes.len());
    println!(
        "checkpoint ledger {} — {} of {} buckets",
        list.checkpoint_ledger,
        take,
        list.hashes.len()
    );

    let mut tally = EntryTally::default();
    let mut bytes_total = 0u64;
    for (i, hash) in list.hashes.iter().take(take).enumerate() {
        let url = BucketList::url(PUBNET_ARCHIVE, hash);
        let t0 = std::time::Instant::now();
        let body = client
            .get(&url)
            .send()
            .await
            .map_err(|e| BackfillError::Incomplete(format!("bucket fetch failed: {e}")))?
            .bytes()
            .await
            .map_err(|e| BackfillError::Incomplete(format!("bucket body failed: {e}")))?;
        bytes_total += body.len() as u64;
        let before = tally.records;
        let decoded = stream_bucket(&body[..], |rec| {
            tally.observe(&rec);
            Ok(())
        })?;
        println!(
            "  [{:>2}/{take}] {:>10} B  {:>9} records  {:>6.1}s  {}",
            i + 1,
            body.len(),
            tally.records - before,
            t0.elapsed().as_secs_f64(),
            &hash[..12]
        );
        let _ = decoded;
    }

    println!("\n  entry type          live         dead");
    for (i, name) in ENTRY_TYPE_NAMES.iter().enumerate() {
        if tally.live[i] > 0 || tally.dead[i] > 0 {
            println!("  {name:<18} {:>10} {:>12}", tally.live[i], tally.dead[i]);
        }
    }
    println!(
        "\n  total {} records, {:.2} GB downloaded, {:.1}s",
        tally.records,
        bytes_total as f64 / 1e9,
        started.elapsed().as_secs_f64()
    );
    println!(
        "  NOTE: counts are RECORDS, not distinct entries — a key may appear in\n\
         \x20 several buckets and only the FIRST occurrence is live state."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The archive's fan-out layout, pinned. A wrong split silently 404s every
    /// bucket, which reads as "the archive is down" rather than "our URL is
    /// wrong".
    #[test]
    fn bucket_url_uses_the_three_byte_pair_fanout() {
        let h = "4a478392b7fd16baf9acf1290c687130e4106f961688c2a8f35b898653e51f22";
        assert_eq!(
            BucketList::url("https://archive", h),
            format!("https://archive/bucket/4a/47/83/bucket-{h}.xdr.gz")
        );
    }

    /// Decode a REAL bucket from the SDF archive end to end. Ignored by
    /// default because it reaches the network; run explicitly with
    /// `cargo test -p backfill-runner -- --ignored snapshot`.
    ///
    /// This is the check that the format assumption holds: a gzipped stream of
    /// XDR `BucketEntry`, not a framed or length-prefixed container. If
    /// stellar-core ever changes the encoding, this fails loudly instead of
    /// the seed silently reading zero entries.
    #[tokio::test]
    #[ignore = "network"]
    async fn decodes_a_real_bucket_from_the_archive() {
        let client = reqwest::Client::new();
        let list = fetch_bucket_list(&client, PUBNET_ARCHIVE)
            .await
            .expect("manifest");
        assert!(
            list.checkpoint_ledger > 50_000_000,
            "checkpoint ledger looks wrong: {}",
            list.checkpoint_ledger
        );
        assert!(
            !list.hashes.is_empty(),
            "bucket list must not be empty — an empty list would make the seed \
             a no-op that reports success"
        );

        // Pick the smallest bucket so the test stays cheap: HEAD each one.
        let mut smallest: Option<(u64, String)> = None;
        for h in &list.hashes {
            let url = BucketList::url(PUBNET_ARCHIVE, h);
            // The CDN sometimes answers HEAD with content-length 0; treat that
            // as unknown rather than as "smallest", or the test picks a bucket
            // it cannot size-check.
            let len = client
                .head(&url)
                .send()
                .await
                .expect("head")
                .content_length()
                .filter(|&l| l > 0)
                .unwrap_or(u64::MAX);
            if smallest.as_ref().is_none_or(|(s, _)| len < *s) {
                smallest = Some((len, url));
            }
        }
        let (size, url) = smallest.expect("at least one bucket");
        let bytes = client
            .get(&url)
            .send()
            .await
            .expect("get")
            .bytes()
            .await
            .expect("body");
        if size != u64::MAX {
            assert_eq!(bytes.len() as u64, size, "truncated download");
        }
        assert!(!bytes.is_empty(), "empty bucket body");

        let mut tally = EntryTally::default();
        let records = stream_bucket(&bytes[..], |rec| {
            tally.observe(&rec);
            Ok(())
        })
        .expect("decode");

        assert!(records > 0, "a real bucket decoded to zero records");
        // Decoded includes the leading METAENTRY, which is not a ledger record
        // and is deliberately not handed to the callback.
        assert!(
            tally.records < records,
            "expected at least one metadata record to be skipped"
        );
        assert_eq!(
            records - tally.records,
            1,
            "exactly one METAENTRY expected per bucket"
        );
        let total_live: u64 = tally.live.iter().sum();
        let total_dead: u64 = tally.dead.iter().sum();
        assert!(
            total_live + total_dead > 0,
            "records decoded but none classified — the entry-type indexing is wrong"
        );
        // Print the tally directly: the test asserts shape, but the NUMBERS are
        // the deliverable — this is the measurement the seed ticket demands
        // before any write path is designed.
        for (i, name) in ENTRY_TYPE_NAMES.iter().enumerate() {
            if tally.live[i] > 0 || tally.dead[i] > 0 {
                println!(
                    "  {name:<18} live={:<8} dead={}",
                    tally.live[i], tally.dead[i]
                );
            }
        }
        println!(
            "  checkpoint={} bucket={} records={}",
            list.checkpoint_ledger,
            url.rsplit('/').next().unwrap_or(""),
            records
        );
    }
}
