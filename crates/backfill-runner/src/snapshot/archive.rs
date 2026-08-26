//! History-archive transport — manifest, buckets, framed XDR.
//!
//! Everything in this module is about GETTING the bytes and turning them into
//! [`SnapshotRecord`]s. Nothing here knows what a trustline is, what our
//! tables look like, or which record wins when a key appears twice; that is
//! [`crate::snapshot::network_state`]'s job.
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

use std::io::{BufReader, Read, Seek, SeekFrom, Write};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use stellar_xdr::{BucketEntry, Frame, Limited, Limits, ReadXdr};
use tracing::info;

use crate::error::BackfillError;

/// Public SDF history archive for pubnet. Content-addressed: every downloaded
/// bucket is SHA-256-checked against the manifest's hash, so a truncated or
/// substituted file fails loudly. The manifest itself is taken on trust
/// (TLS + SDF) — verifying it against the consensus-signed ledger header was
/// built, live-verified once, and then deliberately removed as over-engineering
/// (2026-08-20 review); resurrect from git history if a mirror is ever added.
pub const PUBNET_ARCHIVE: &str = "https://history.stellar.org/prd/core-live/core_live_001";

/// Ledgers per checkpoint. The archive publishes one checkpoint every 64
/// ledgers, at sequences ≡ 63 (mod 64) — stellar-core `docs/history.md`.
const CHECKPOINT_FREQUENCY: u32 = 64;

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

/// HTTP client for archive reads. `reqwest`'s default is NO timeout, so a
/// stalled connection on a 2.47 GB bucket would hang the pass forever. The
/// budget is per-request and generous: the largest bucket takes ~3.5 minutes on
/// a good link, so 30 minutes tolerates a very slow one without hanging.
pub fn archive_client() -> Result<reqwest::Client, BackfillError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1800))
        .connect_timeout(std::time::Duration::from_secs(30))
        .user_agent("soroban-block-explorer/backfill-runner/0.1")
        .build()
        .map_err(|e| BackfillError::Incomplete(format!("archive client build failed: {e}")))
}

/// Checkpoints are published every 64 ledgers, at sequences ≡ 63 (mod 64)
/// (stellar-core `docs/history.md`).
fn is_checkpoint(ledger: u32) -> bool {
    ledger % CHECKPOINT_FREQUENCY == CHECKPOINT_FREQUENCY - 1
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
        // Same reason the bucket fetch does this: without it a 404 or CDN
        // error page reaches the JSON parser and surfaces as "manifest decode
        // failed" — an archive outage reported as a format problem.
        .error_for_status()
        .map_err(|e| BackfillError::Incomplete(format!("history manifest HTTP status: {e}")))?
        .json()
        .await
        .map_err(|e| BackfillError::Incomplete(format!("history manifest decode failed: {e}")))?;

    let checkpoint_ledger = body
        .get("currentLedger")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no currentLedger".into()))?
        as u32;

    // A `currentLedger` off the checkpoint lattice is not a checkpoint, so the
    // bucket list beside it is not a checkpoint's state — and every ledger
    // number this run stamps into `closed_at_ledger` would be a fiction. Cheap
    // check, load-bearing conclusion.
    if !is_checkpoint(checkpoint_ledger) {
        return Err(BackfillError::Incomplete(format!(
            "manifest currentLedger {checkpoint_ledger} is not a checkpoint (checkpoints are ≡ {} mod {CHECKPOINT_FREQUENCY}) — this is not a checkpoint bucket list",
            CHECKPOINT_FREQUENCY - 1
        )));
    }

    // An all-zero hash is an empty slot, not a bucket. Levels run newest to
    // oldest and `curr` precedes `snap` within a level — preserve that order.
    const EMPTY: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    let arr = body
        .get("currentBuckets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no currentBuckets".into()))?;
    let mut hashes = Vec::new();
    for level in arr {
        for slot in ["curr", "snap"] {
            // NOT `unwrap_or(EMPTY)`: an absent or non-string slot would then
            // be indistinguishable from a legitimately empty one, and the run
            // would proceed with fewer buckets than the checkpoint has — the
            // one input whose short read is catastrophic (see the floors in
            // `open_snapshot`). Real manifests always carry both keys, so this
            // arm is only reached when the manifest is not what we assume.
            let h = level
                .get(slot)
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    BackfillError::Incomplete(format!(
                        "manifest currentBuckets level is missing a string `{slot}` — \
                     the manifest is not the shape this reader assumes"
                    ))
                })?;
            // A malformed hash produces a 404 on the bucket URL, which reads as
            // "the archive is down" rather than "the manifest is wrong". Fail
            // here, where the cause is still visible.
            if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(BackfillError::Incomplete(format!(
                    "manifest currentBuckets hash is not 64 hex chars: {h:?}"
                )));
            }
            if h != EMPTY {
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
            //
            // `stellar/go`'s reader deliberately does NOT record INITENTRY keys
            // in its seen-set, leaning on the CAP-0020 invariant that an
            // INITENTRY implies no older entry for that key survives. That is a
            // MEMORY optimisation resting on an invariant, not a correctness
            // rule. Treating both alike is strictly more conservative and stays
            // right even if the invariant is ever violated — do not "fix" this
            // to match Go.
            BucketEntry::Liveentry(e) | BucketEntry::Initentry(e) => {
                f(SnapshotRecord::Live(Box::new(e)))?;
            }
            // A tombstone. Seen first for a key it means deleted, and an older
            // LIVEENTRY below must not resurrect it — same rule `stellar/go`
            // implements with its `visitedLedgerKeys` set.
            BucketEntry::Deadentry(k) => f(SnapshotRecord::Dead(Box::new(k)))?,
            // Bucket metadata carries the protocol version, not ledger state.
            // It is REQUIRED to be the first record; anywhere else means we are
            // not reading what we think we are, so fail rather than skip.
            BucketEntry::Metaentry(_) => {
                if count != 1 {
                    return Err(BackfillError::Incomplete(format!(
                        "METAENTRY at record {count}, expected only at record 1 \
                         — bucket format is not what this reader assumes"
                    )));
                }
            }
        }
    }
    Ok(count)
}

/// Download one bucket to an unnamed temp file and stream-decode it from there.
///
/// The response body is NOT buffered in memory: the largest single bucket
/// measured 2.47 GB, and a first version of this that called `.bytes()` peaked
/// at 2.80 GB RSS on the full pass — the whole bucket plus its decompression
/// window. The seed adds a per-key dedup set on top of this, so the download
/// must not be the thing that owns the memory budget.
///
/// The temp file is unlinked on drop, so a crash mid-pass leaves nothing behind.
pub(crate) async fn stream_bucket_from_url<F>(
    client: &reqwest::Client,
    url: &str,
    expect_hash: &str,
    f: F,
) -> Result<u64, BackfillError>
where
    F: FnMut(SnapshotRecord) -> Result<(), BackfillError>,
{
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| BackfillError::Incomplete(format!("bucket fetch failed: {e}")))?
        // Without this a 404 body (XML) reaches the gzip reader and surfaces as
        // "bucket XDR decode failed" — the wrong cause, in the wrong layer.
        .error_for_status()
        .map_err(|e| BackfillError::Incomplete(format!("bucket HTTP status: {e}")))?;

    let mut tmp = tempfile::tempfile()
        .map_err(|e| BackfillError::Incomplete(format!("bucket temp file failed: {e}")))?;
    let mut bytes = 0u64;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| BackfillError::Incomplete(format!("bucket body failed: {e}")))?
    {
        bytes += chunk.len() as u64;
        tmp.write_all(&chunk)
            .map_err(|e| BackfillError::Incomplete(format!("bucket spill failed: {e}")))?;
    }
    tmp.seek(SeekFrom::Start(0))
        .map_err(|e| BackfillError::Incomplete(format!("bucket rewind failed: {e}")))?;

    // The archive is CONTENT-ADDRESSED, and the hash covers the DECOMPRESSED
    // XDR — `.gz` is transport the publisher adds, so hashing the downloaded
    // bytes fails on every bucket. Verified empirically against a real one:
    // gunzip then SHA-256 reproduces the file name exactly. Checking it turns
    // "we trust TLS and DNS" into "we checked", which is what lets this source
    // be called verified at all.
    let mut gz = GzDecoder::new(&tmp);
    let mut digest = Sha256::new();
    // `Sha256` is itself an `io::Write` sink, so the whole decompressed stream
    // goes through in one statement — no hand-driven buffer to check.
    std::io::copy(&mut gz, &mut digest)
        .map_err(|e| BackfillError::Incomplete(format!("bucket verify read: {e}")))?;
    let got = hex::encode(digest.finalize());
    if got != expect_hash {
        return Err(BackfillError::Incomplete(format!(
            "bucket content hash mismatch: manifest says {expect_hash}, decompressed bytes \
             hash to {got} — a substituted or truncated bucket, NOT a decode problem"
        )));
    }
    drop(gz);
    tmp.seek(SeekFrom::Start(0))
        .map_err(|e| BackfillError::Incomplete(format!("bucket rewind failed: {e}")))?;

    stream_bucket(BufReader::new(&tmp), f)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::network_state::NetworkState;

    /// The manifest's `currentLedger` must sit on the checkpoint lattice.
    /// Accepting an off-lattice value would mean decoding a bucket list that
    /// is not a checkpoint's state, and stamping fictional ledger numbers.
    ///
    /// Calls the production predicate, and the cases are chosen to pin the
    /// FREQUENCY rather than restate modular arithmetic: this test previously
    /// defined its own `l % CHECKPOINT_FREQUENCY == …` closure and asserted
    /// against that, so deleting the check in `fetch_bucket_list` left it
    /// green — and every case it used still passes at a frequency of 32.
    /// `!is_checkpoint(31)` is the one that fails if the constant moves.
    #[test]
    fn checkpoint_lattice_accepts_only_63_mod_64() {
        assert!(is_checkpoint(63), "the first checkpoint");
        assert!(
            is_checkpoint(64_102_079),
            "a real manifest value, probed 2026-08-24"
        );
        assert!(!is_checkpoint(64_102_080));
        assert!(!is_checkpoint(0));
        assert!(!is_checkpoint(31), "63 mod 64, not 31 mod 32");
    }

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
    /// This is the check that the format assumption holds: a gzipped, FRAMED
    /// stream of XDR `BucketEntry` — each record behind a 4-byte big-endian
    /// length prefix whose top bit marks the last fragment. (An earlier version
    /// of this comment claimed the opposite; decoding a real bucket is what
    /// settled it.) If stellar-core ever changes the encoding, this fails
    /// loudly instead of the seed silently reading zero entries.
    #[tokio::test]
    #[ignore = "network"]
    async fn decodes_a_real_bucket_from_the_archive() {
        let client = archive_client().expect("client");
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

        let mut state = NetworkState::default();
        let mut handed = 0u64;
        let records = stream_bucket(&bytes[..], |rec| {
            handed += 1;
            state.absorb_record(&rec);
            Ok(())
        })
        .expect("decode");

        assert!(records > 0, "a real bucket decoded to zero records");
        // Decoded includes the leading METAENTRY, which is not a ledger record
        // and is deliberately not handed to the callback.
        assert_eq!(
            records - handed,
            1,
            "exactly one METAENTRY expected per bucket"
        );
        assert!(
            state.accounts.len() + state.trustlines.len() + state.pool_shares.len() > 0
                || state.unmodelled > 0,
            "records decoded but none classified — the classification is wrong"
        );
        println!(
            "  checkpoint={} bucket={} records={} accounts={} trustlines={} \
             pool_shares={} unmodelled={}",
            list.checkpoint_ledger,
            url.rsplit('/').next().unwrap_or(""),
            records,
            state.accounts.len(),
            state.trustlines.len(),
            state.pool_shares.len(),
            state.unmodelled
        );
    }
}
