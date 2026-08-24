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
//! ## Validated against the reference implementation (2026-08-18)
//!
//! Our ordering and dedup were checked against `stellar/go`'s
//! `ingest.NewCheckpointChangeReader` (`ingest/checkpoint_change_reader.go`),
//! the reference reader for this artifact. Same level order (0→10, `curr`
//! before `snap`, zero hashes skipped), same DEADENTRY-as-tombstone rule. We
//! are deliberately MORE conservative on INITENTRY — see `stream_bucket`.
//!
//! - **`next` is correctly ignored.** Each manifest level also carries `next`,
//!   the descriptor of an in-flight merge. It is not committed state: Go's
//!   `BucketList.Hash()` folds only `curr` and `snap`. Note the live manifest
//!   currently shows `state: 0` on every level, but historical checkpoints do
//!   not — never infer "next is always idle" from today's file.
//! - **Shadow buckets are not a concern.** CAP-0025 (protocol 12, 2019)
//!   removed them; they only ever existed inside an in-flight merge, never in
//!   a committed `curr`/`snap`. A modern reader sees a superset of the old
//!   behaviour.
//! - **`hotArchiveBuckets` is deliberately not read.** CAP-0062 (protocol 23)
//!   added a second bucket list for evicted Soroban `PERSISTENT` entries. An
//!   eviction DELETES the entry from the live bucket list, so `currentBuckets`
//!   alone remains the authority on what is live and this reader classifies an
//!   evicted holding as gone — correct. What the hot archive would ADD is the
//!   ability to say "archived but restorable" instead of "gone", a display
//!   distinction (task 0463's T8 question, resolved in our favour).
//!
//! ## Staleness contract
//!
//! The snapshot is correct at its checkpoint ledger and stale the moment it
//! lands. Anything written from it must be versioned on each entry's own
//! `lastModifiedLedgerSeq` so live parser writes win regardless of load order
//! — never on a window boundary, which is the defect task 0492 documents.

use std::io::{BufReader, Read, Seek, SeekFrom, Write};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use stellar_xdr::{BucketEntry, Frame, Limited, Limits, ReadXdr};
use tracing::info;

use db_clickhouse::persist::ids;

use crate::error::BackfillError;

/// Public SDF history archive for pubnet. Content-addressed: every downloaded
/// bucket is SHA-256-checked against the manifest's hash, so a truncated or
/// substituted file fails loudly. The manifest itself is taken on trust
/// (TLS + SDF) — verifying it against the consensus-signed ledger header was
/// built, live-verified once, and then deliberately removed as over-engineering
/// (2026-08-20 review); resurrect from git history if a mirror is ever added.
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
    let arr = body
        .get("currentBuckets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no currentBuckets".into()))?;
    let mut hashes = Vec::new();
    for level in arr {
        for slot in ["curr", "snap"] {
            let h = level
                .get(slot)
                .and_then(serde_json::Value::as_str)
                .unwrap_or(EMPTY);
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

/// Load a bucket list from a previously written `manifest.json` instead of the
/// live archive.
///
/// The manifest advances every 64 ledgers (~5 min) and a full pass takes ~6, so
/// a run that re-fetches it decodes a DIFFERENT snapshot than the dry-run
/// reported on. That makes `summary.txt` describe a run that never happened —
/// the opposite of the reviewability the dry-run exists to provide. Pinning is
/// what makes `--execute` run the thing that was reviewed, and what makes a
/// failed run resumable against the same state.
pub fn bucket_list_from_manifest(path: &std::path::Path) -> Result<BucketList, BackfillError> {
    let body = std::fs::read_to_string(path)
        .map_err(|e| BackfillError::Incomplete(format!("read {}: {e}", path.display())))?;
    let v: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| BackfillError::Incomplete(format!("manifest {}: {e}", path.display())))?;
    let checkpoint_ledger = v
        .get("checkpoint_ledger")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no checkpoint_ledger".into()))?
        as u32;
    let hashes: Vec<String> = v
        .get("buckets")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| BackfillError::Incomplete("manifest has no buckets".into()))?
        .iter()
        .filter_map(|h| h.as_str().map(str::to_string))
        .collect();
    if hashes.is_empty() {
        return Err(BackfillError::Incomplete(
            "pinned manifest lists no buckets — refusing to run a no-op that reports success"
                .into(),
        ));
    }
    info!(
        checkpoint_ledger,
        buckets = hashes.len(),
        "bucket list PINNED from manifest"
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
    expect_hash: Option<&str>,
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
    if let Some(want) = expect_hash {
        let mut gz = GzDecoder::new(&tmp);
        let mut digest = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = gz
                .read(&mut buf)
                .map_err(|e| BackfillError::Incomplete(format!("bucket verify read: {e}")))?;
            if n == 0 {
                break;
            }
            digest.update(&buf[..n]);
        }
        let got = hex::encode(digest.finalize());
        if got != want {
            return Err(BackfillError::Incomplete(format!(
                "bucket content hash mismatch: manifest says {want}, decompressed bytes \
                 hash to {got} — a substituted or truncated bucket, NOT a decode problem"
            )));
        }
        tmp.seek(SeekFrom::Start(0))
            .map_err(|e| BackfillError::Incomplete(format!("bucket rewind failed: {e}")))?;
    }

    stream_bucket(BufReader::new(&tmp), f)?;
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// First-wins deduplication — records to DISTINCT entries (task 0463 step 3b)
// ---------------------------------------------------------------------------

/// A holding keyed the way OUR `balances` table keys it: the two `cityhash`
/// surrogates. Deriving these locally is what makes the comparison possible at
/// all — the surrogates are deterministic hashes of the StrKey, not database
/// counters, so the snapshot can be translated into our key space without a
/// single lookup.
///
/// Note this is NOT computable in ClickHouse SQL: `cityHash64()` there is the
/// 64-bit CityHash variant, a different algorithm from the low half of the
/// 128-bit one `ids::` uses (see that module's header). The translation has to
/// happen in Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HoldingKey {
    pub holder_id: i64,
    pub asset_id: i64,
}

/// First-wins state of one entry. `live == false` means the newest record for
/// this key was a `DeadEntry`, i.e. the entry is deleted as of the checkpoint.
#[derive(Debug, Clone, Copy)]
pub struct SnapEntry {
    pub live: bool,
    /// The entry's OWN `lastModifiedLedgerSeq`. Zero for a dead entry, which
    /// carries only a key. Anything seeded from this versions on it — never on
    /// a window boundary (the task 0492 defect).
    pub ledger: u32,
    /// Stroops, exactly as the ledger carries them. Zero for a dead entry.
    pub amount: i64,
    /// Set during the comparison when a row on our side matched this key.
    /// Whatever stays false at the end is what the network has and we do not.
    pub matched: bool,
}

impl SnapEntry {
    fn live(ledger: u32, amount: i64) -> Self {
        Self {
            live: true,
            ledger,
            amount,
            matched: false,
        }
    }
    fn dead() -> Self {
        Self {
            live: false,
            ledger: 0,
            amount: 0,
            matched: false,
        }
    }
}

/// One classified record, in our key space. Entry types we do not model are
/// counted, not silently dropped.
enum SnapItem {
    /// An `AccountEntry` carries TWO facts: the account exists, and it holds
    /// this much native XLM. Native is not a trustline — it lives on the
    /// account — so "absent from the snapshot" for a native holding means the
    /// ACCOUNT is gone. That is the ~52k merged-account ghost case.
    Account {
        holder_id: i64,
        entry: SnapEntry,
        /// Only built when the caller asked for details (the seed pass).
        detail: Option<Box<AccountDetail>>,
    },
    /// A classic credit trustline, keyed onto our surrogate pair.
    Trustline { key: HoldingKey, entry: SnapEntry },
    /// A pool-share trustline. Same ledger entry type, DIFFERENT table on our
    /// side (`lp_positions`), so it is tallied apart rather than folded into
    /// the `balances` comparison and counted as a phantom.
    PoolShare {
        holder_id: i64,
        pool_id: [u8; 32],
        entry: SnapEntry,
    },
}

/// Everything an `AccountEntry` carries beyond its native balance: identity for
/// dimension stubs, signers + thresholds for `account_entry_state`.
#[derive(Debug)]
pub struct AccountDetail {
    pub strkey: String,
    pub seq_num: i64,
    pub home_domain: String,
    /// `[master, low, med, high]`, raw from the XDR.
    pub thresholds: [u8; 4],
    pub flags: u32,
    /// `(signer strkey, weight, type name)`. The MASTER KEY IS NOT HERE — its
    /// weight is thresholds byte 0. Horizon synthesizes a master entry into its
    /// signers array; the ledger does not carry one, and neither do we.
    pub signers: Vec<(String, u32, &'static str)>,
}

/// The deduplicated snapshot, ready to compare against our tables.
///
/// ONE fold of the bucket stream serves both consumers: the comparison
/// (`snapshot_report`) and the write (`snapshot_seed`). They previously kept
/// separate state types with separate first-wins logic — two chances to
/// disagree about what the network says.
#[derive(Debug, Default)]
pub struct SnapshotState {
    /// `holder_id` → the account's existence and its NATIVE holding.
    pub accounts: std::collections::HashMap<i64, SnapEntry>,
    /// Classic credit trustlines.
    pub trustlines: std::collections::HashMap<HoldingKey, SnapEntry>,
    /// Pool shares, kept separate — see [`SnapItem::PoolShare`].
    pub pool_shares: std::collections::HashMap<(i64, [u8; 32]), SnapEntry>,
    /// Per-account identity, signers and thresholds, for `account_entry_state`.
    pub account_details: std::collections::HashMap<i64, AccountDetail>,
    /// asset surrogate → `(code, issuer strkey)`, for `assets` dimension stubs.
    /// Bounded by network asset cardinality (~391k), not trustline count.
    pub asset_registry: std::collections::HashMap<i64, (String, String)>,
    /// Records of entry types this comparison does not model (offers, contract
    /// data, TTL, …). Reported so the pass never looks more complete than it is.
    pub unmodelled: u64,
    /// Records superseded by an earlier (newer) record for the same key. This
    /// is the number first-wins actually suppressed.
    pub superseded: u64,
}

impl SnapshotState {
    /// Insert under FIRST-WINS: the bucket list is ordered newest-first, so the
    /// first record seen for a key is the live state and every later one is
    /// history. A `DeadEntry` seen first must NOT be overwritten by an older
    /// `LiveEntry` further down — that is exactly the resurrection bug this
    /// whole effort is about, in the source format.
    fn absorb(&mut self, item: SnapItem) {
        use std::collections::hash_map::Entry;
        macro_rules! first_wins {
            ($map:expr, $k:expr, $v:expr) => {
                match $map.entry($k) {
                    Entry::Vacant(slot) => {
                        slot.insert($v);
                    }
                    Entry::Occupied(_) => self.superseded += 1,
                }
            };
        }
        match item {
            SnapItem::Account {
                holder_id,
                entry,
                detail,
            } => {
                // The detail must follow the SAME first-wins rule as the entry,
                // or a superseded older AccountEntry could overwrite the live
                // signer set — the resurrection bug, one field deeper.
                if !self.accounts.contains_key(&holder_id)
                    && let Some(d) = detail
                {
                    self.account_details.insert(holder_id, *d);
                }
                first_wins!(self.accounts, holder_id, entry)
            }
            SnapItem::Trustline { key, entry } => first_wins!(self.trustlines, key, entry),
            SnapItem::PoolShare {
                holder_id,
                pool_id,
                entry,
            } => {
                first_wins!(self.pool_shares, (holder_id, pool_id), entry)
            }
        }
    }

    /// Classify one decoded record and fold it in. The single entry point the
    /// report and the write share, so they cannot disagree about what the
    /// snapshot says.
    pub fn absorb_record(&mut self, rec: &SnapshotRecord) {
        match classify(rec) {
            Some(item) => {
                if let SnapItem::Trustline { key, .. } = &item
                    && let Some((code, issuer)) = trustline_identity(rec)
                {
                    self.asset_registry
                        .entry(key.asset_id)
                        .or_insert((code, issuer));
                }
                self.absorb(item);
            }
            None => self.unmodelled += 1,
        }
    }

    pub fn live_accounts(&self) -> usize {
        self.accounts.values().filter(|e| e.live).count()
    }
    pub fn live_trustlines(&self) -> usize {
        self.trustlines.values().filter(|e| e.live).count()
    }
    pub fn live_pool_shares(&self) -> usize {
        self.pool_shares.values().filter(|e| e.live).count()
    }
}

/// The surrogate `asset_id` for a trustline asset, or `None` for a pool share
/// (which is not an `assets` row at all).
///
/// Matches the writer's derivation exactly — same canonical code normalizer,
/// same `credit_asset_id` formula. A divergence here would not fail loudly; it
/// would silently key every asset differently and report the entire network as
/// missing, so the two call sites are deliberately spelled the same way.
fn trustline_asset_id(asset: &stellar_xdr::TrustLineAsset) -> Option<i64> {
    use stellar_xdr::TrustLineAsset as A;
    match asset {
        // A native "trustline" does not exist on chain; native lives on the
        // AccountEntry. Kept total rather than unreachable.
        A::Native => Some(ids::NATIVE_ASSET_ID),
        A::CreditAlphanum4(a) => Some(ids::credit_asset_id(
            &xdr_parser::asset_code::asset_code_str(a.asset_code.as_slice()),
            &a.issuer.to_string(),
        )),
        A::CreditAlphanum12(a) => Some(ids::credit_asset_id(
            &xdr_parser::asset_code::asset_code_str(a.asset_code.as_slice()),
            &a.issuer.to_string(),
        )),
        A::PoolShare(_) => None,
    }
}

/// `(code, issuer strkey)` of a live classic trustline record — the identity
/// behind the surrogate, needed only for `assets` dimension stubs.
fn trustline_identity(rec: &SnapshotRecord) -> Option<(String, String)> {
    use stellar_xdr::{LedgerEntryData as D, TrustLineAsset as A};
    let SnapshotRecord::Live(e) = rec else {
        return None;
    };
    let D::Trustline(t) = &e.data else {
        return None;
    };
    match &t.asset {
        A::CreditAlphanum4(a) => Some((
            xdr_parser::asset_code::asset_code_str(a.asset_code.as_slice()),
            a.issuer.to_string(),
        )),
        A::CreditAlphanum12(a) => Some((
            xdr_parser::asset_code::asset_code_str(a.asset_code.as_slice()),
            a.issuer.to_string(),
        )),
        A::Native | A::PoolShare(_) => None,
    }
}

/// Build the per-account detail the seed needs (signers, thresholds, identity).
fn account_detail(a: &stellar_xdr::AccountEntry) -> AccountDetail {
    AccountDetail {
        strkey: a.account_id.to_string(),
        seq_num: i64::from(a.seq_num.clone()),
        home_domain: String::from_utf8_lossy(a.home_domain.as_slice()).to_string(),
        thresholds: a.thresholds.0,
        flags: a.flags,
        signers: a
            .signers
            .iter()
            .map(|sg| {
                (
                    sg.key.to_string(),
                    sg.weight,
                    // The live writer's own helper — one vocabulary, enforced
                    // by the compiler rather than promised in a comment.
                    xdr_parser::ledger_entry_changes::signer_type_name(&sg.key),
                )
            })
            .collect(),
    }
}

/// Classify one decoded record into our key space, or `None` for an entry type
/// this comparison does not model.
fn classify(rec: &SnapshotRecord) -> Option<SnapItem> {
    use stellar_xdr::{LedgerEntryData as D, LedgerKey as K, TrustLineAsset as A};
    match rec {
        SnapshotRecord::Live(e) => match &e.data {
            D::Account(a) => Some(SnapItem::Account {
                holder_id: ids::address_id(&a.account_id.to_string()),
                entry: SnapEntry::live(e.last_modified_ledger_seq, a.balance),
                detail: Some(Box::new(account_detail(a))),
            }),
            D::Trustline(t) => {
                let holder_id = ids::address_id(&t.account_id.to_string());
                let entry = SnapEntry::live(e.last_modified_ledger_seq, t.balance);
                match (&t.asset, trustline_asset_id(&t.asset)) {
                    (A::PoolShare(p), _) => Some(SnapItem::PoolShare {
                        holder_id,
                        pool_id: p.0.0,
                        entry,
                    }),
                    (_, Some(asset_id)) => Some(SnapItem::Trustline {
                        key: HoldingKey {
                            holder_id,
                            asset_id,
                        },
                        entry,
                    }),
                    (_, None) => None,
                }
            }
            _ => None,
        },
        SnapshotRecord::Dead(k) => match k.as_ref() {
            K::Account(a) => Some(SnapItem::Account {
                holder_id: ids::address_id(&a.account_id.to_string()),
                entry: SnapEntry::dead(),
                detail: None,
            }),
            K::Trustline(t) => {
                let holder_id = ids::address_id(&t.account_id.to_string());
                match (&t.asset, trustline_asset_id(&t.asset)) {
                    (A::PoolShare(p), _) => Some(SnapItem::PoolShare {
                        holder_id,
                        pool_id: p.0.0,
                        entry: SnapEntry::dead(),
                    }),
                    (_, Some(asset_id)) => Some(SnapItem::Trustline {
                        key: HoldingKey {
                            holder_id,
                            asset_id,
                        },
                        entry: SnapEntry::dead(),
                    }),
                    (_, None) => None,
                }
            }
            _ => None,
        },
    }
}

// ---------------------------------------------------------------------------
// The verdict — ONE rule, rendered by the compare pass and acted on by the seed
// ---------------------------------------------------------------------------

/// Floor on the our-rows read, shared by the compare and the seed. A short
/// read (wrong database, a dropped key slice) is indistinguishable from a real
/// one downstream: every missing row becomes an unmatched snapshot entry, i.e.
/// a phantom network gap the seed would INSERT as a live holding. The real
/// population measured 48.6M distinct (holder, asset) pairs — sit just under.
pub(crate) const MIN_OUR_ROWS: u64 = 40_000_000;

/// One of our deduplicated `balances` rows, exactly the SELECT list of
/// `snapshot_seed::slice_sql`. Shared by the report and the write so the
/// two consumers cannot drift into different column meanings.
#[derive(Debug, Clone, Copy, clickhouse::Row, serde::Deserialize)]
pub struct OurRow {
    pub holder_id: i64,
    pub asset_id: i64,
    pub amount: i128,
    pub last_updated_ledger: i64,
    pub closed_at_ledger: i64,
}

/// What the snapshot says about one of our rows. The compare pass COUNTS these;
/// the seed ACTS on them. They were two separate implementations that had
/// already drifted — the report an operator signs off on did not predict what
/// `--execute` would write. One rule, two consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Our writer already stamped this closed, and the snapshot agrees (or is
    /// older). Nothing to do.
    AlreadyClosed,
    /// We stamped it CLOSED, but the network holds it LIVE at a NEWER ledger.
    /// Our closure is wrong and the holding is invisible on the page — the
    /// costliest outcome in this whole comparison, because it hides funds a
    /// user actually has. Before this verdict existed the row was marked
    /// matched and dropped, so it was never re-inserted and never reported.
    ClosedButLive,
    /// Both sides hold it and agree.
    Agree,
    /// Both hold it, amounts differ, and the SNAPSHOT is strictly newer — the
    /// seed heals these.
    HealFromSnapshot,
    /// Both hold it, amounts differ, and our row is strictly newer. The live
    /// parser knows better; report only.
    DivergentOursNewer,
    /// Both hold it at the SAME ledger and the amounts still differ. One of us
    /// parsed that ledger wrong, and it cannot be us-versus-them freshness —
    /// so this is a defect signal, not routine drift. Reported apart and never
    /// auto-healed: guessing a winner would bury the evidence.
    DivergentSameLedger,
    /// Both hold it with equal amounts, but our ledger is behind the entry's.
    Stale,
    /// The network does not have it and we hold zero — a real closure.
    Closure,
    /// The network does not have it and we hold a POSITIVE amount. Never folded
    /// into `Closure`: it is the signal of an ingestion gap, and treating it as
    /// routine would let a false number pass as a resolved one.
    Ghost,
    /// The network does not have it, but our row is NEWER than the checkpoint —
    /// the snapshot is the stale side. Closing it would delete a holding the
    /// network created in the gap.
    NewerThanCheckpoint,
}

/// The single classification rule. `snap` is the snapshot's entry for this key
/// (`None` = the network has no such entry at all); it is marked matched here,
/// so whatever stays unmatched afterwards is exactly what the network holds and
/// we do not.
pub fn verdict(row: &OurRow, snap: Option<&mut SnapEntry>, checkpoint: u32) -> Verdict {
    if row.closed_at_ledger != 0 {
        // Mark it matched: the entry IS accounted for on our side, and leaving
        // it unmatched would count our own closure as a network gap and
        // re-insert it as live.
        //
        // But "accounted for" is not "correctly closed". If the network holds
        // the entry LIVE at a ledger newer than our closure, our closure is the
        // stale fact and the holding is hidden. Marking matched without this
        // check suppressed exactly that case from both the report and the
        // re-insert — silently, and permanently.
        if let Some(e) = snap {
            e.matched = true;
            if e.live && i64::from(e.ledger) > row.last_updated_ledger {
                return Verdict::ClosedButLive;
            }
        }
        return Verdict::AlreadyClosed;
    }
    match snap {
        Some(e) if e.live => {
            e.matched = true;
            if i128::from(e.amount) != row.amount {
                match i64::from(e.ledger).cmp(&row.last_updated_ledger) {
                    std::cmp::Ordering::Greater => Verdict::HealFromSnapshot,
                    std::cmp::Ordering::Less => Verdict::DivergentOursNewer,
                    // Same ledger, different amount — freshness cannot explain
                    // it, so one of the two parsers is wrong.
                    std::cmp::Ordering::Equal => Verdict::DivergentSameLedger,
                }
            } else if row.last_updated_ledger < i64::from(e.ledger) {
                Verdict::Stale
            } else {
                Verdict::Agree
            }
        }
        // Present but DEAD, or absent entirely — both mean the network does not
        // hold this relationship.
        other => {
            if let Some(e) = other {
                e.matched = true;
            }
            // `>=`, not `>`: a row last touched AT the checkpoint would be
            // written back at that same ledger, and ReplacingMergeTree resolves
            // an equal version arbitrarily — the closure might lose and the
            // ghost survive, nondeterministically and undetectably. Ceding a
            // handful of legitimate closures is the fail-safe direction.
            if row.last_updated_ledger >= i64::from(checkpoint) {
                Verdict::NewerThanCheckpoint
            } else if row.amount == 0 {
                Verdict::Closure
            } else {
                Verdict::Ghost
            }
        }
    }
}

/// Look up the snapshot entry for one of our rows. Native lives on the
/// `AccountEntry`, everything else is a trustline — the split both consumers
/// need, spelled once.
pub fn snap_entry_for<'a>(state: &'a mut SnapshotState, row: &OurRow) -> Option<&'a mut SnapEntry> {
    if row.asset_id == ids::NATIVE_ASSET_ID {
        state.accounts.get_mut(&row.holder_id)
    } else {
        state.trustlines.get_mut(&HoldingKey {
            holder_id: row.holder_id,
            asset_id: row.asset_id,
        })
    }
}

/// One opened snapshot pass: the bucket list this run decoded and the
/// deduplicated state folded out of it.
///
/// Every consumer needs the same four steps in the same order — build the HTTP
/// client, resolve the bucket list (pinned or freshest), announce the
/// checkpoint, stream every bucket into a [`SnapshotState`]. Spelled once here
/// so two commands cannot drift into decoding the archive differently.
pub struct SnapshotPass {
    pub list: BucketList,
    pub state: SnapshotState,
}

/// Open a snapshot pass.
///
/// `pinned_manifest` decodes a previously recorded bucket list instead of the
/// freshest checkpoint. Optional by design: the newest checkpoint is complete
/// by construction (the archive's `.well-known` manifest is stellar-core's
/// atomic commit point, written LAST), so the pin exists for exact
/// reproduction of an earlier run, not for safety.
pub async fn open_snapshot(
    pinned_manifest: Option<&std::path::Path>,
    label: &str,
) -> Result<SnapshotPass, BackfillError> {
    let started = std::time::Instant::now();
    let http = archive_client()?;
    let list = match pinned_manifest {
        Some(path) => bucket_list_from_manifest(path)?,
        None => fetch_bucket_list(&http, PUBNET_ARCHIVE).await?,
    };
    println!(
        "checkpoint ledger {} — {} buckets{label}",
        list.checkpoint_ledger,
        list.hashes.len()
    );

    let take = list.hashes.len();
    let state = build_state(&http, &list, SnapshotState::default(), |i, bytes, secs| {
        println!("  [{:>2}/{take}] {bytes:>10} B  {secs:>6.1}s", i + 1);
    })
    .await?;
    report_state(
        &state,
        list.checkpoint_ledger,
        started.elapsed().as_secs_f64(),
    );

    Ok(SnapshotPass { list, state })
}

/// Stream every bucket and fold it into a deduplicated [`SnapshotState`].
///
/// Memory is the distinct-entry count, not the record count: ~124M records
/// collapse onto the accounts + trustlines the network actually holds.
pub async fn build_state(
    client: &reqwest::Client,
    list: &BucketList,
    mut state: SnapshotState,
    mut on_bucket: impl FnMut(usize, u64, f64),
) -> Result<SnapshotState, BackfillError> {
    for (i, hash) in list.hashes.iter().enumerate() {
        let url = BucketList::url(PUBNET_ARCHIVE, hash);
        let t0 = std::time::Instant::now();
        let bytes = stream_bucket_from_url(client, &url, Some(hash), |rec| {
            state.absorb_record(&rec);
            Ok(())
        })
        .await?;
        on_bucket(i, bytes, t0.elapsed().as_secs_f64());
    }
    Ok(state)
}

/// DISTINCT-entry report after first-wins, printed by the compare pass before
/// it folds our rows in — the raw record count cannot be compared with our
/// tables, because a key appears in as many buckets as it has versions.
pub fn report_state(state: &SnapshotState, checkpoint_ledger: u32, secs: f64) {
    let dead_accounts = state.accounts.len() - state.live_accounts();
    let dead_trustlines = state.trustlines.len() - state.live_trustlines();
    println!(
        "
  DISTINCT entries at checkpoint {checkpoint_ledger} (first-wins applied)"
    );
    println!("  entity            live         deleted");
    println!(
        "  accounts     {:>10} {:>15}",
        state.live_accounts(),
        dead_accounts
    );
    println!(
        "  trustlines   {:>10} {:>15}",
        state.live_trustlines(),
        dead_trustlines
    );
    println!(
        "  pool shares  {:>10} {:>15}",
        state.live_pool_shares(),
        state.pool_shares.len() - state.live_pool_shares()
    );
    println!(
        "
  {} records superseded by a newer one for the same key",
        state.superseded
    );
    println!(
        "  {} records of entry types this comparison does not model          (offers, contract data, TTL, claimable balances, …)",
        state.unmodelled
    );
    println!("  {secs:.1}s");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(amount: i128, last_updated: i64, closed_at: i64) -> OurRow {
        OurRow {
            holder_id: 1,
            asset_id: 2,
            amount,
            last_updated_ledger: last_updated,
            closed_at_ledger: closed_at,
        }
    }

    fn live(amount: i64, ledger: u32) -> SnapEntry {
        SnapEntry {
            live: true,
            ledger,
            amount,
            matched: false,
        }
    }

    /// FIRST-WINS is the whole anti-resurrection guarantee: the bucket list is
    /// newest-first, so a DeadEntry seen first must not be overwritten by an
    /// older LiveEntry below it. Inverting this comparison would resurrect
    /// every closed trustline on the network, and nothing else in the suite
    /// would notice.
    #[test]
    fn first_wins_keeps_the_newest_record_and_a_tombstone_is_not_resurrected() {
        let mut st = SnapshotState::default();
        let key = HoldingKey {
            holder_id: 7,
            asset_id: 9,
        };
        st.absorb(SnapItem::Trustline {
            key,
            entry: SnapEntry::dead(),
        });
        st.absorb(SnapItem::Trustline {
            key,
            entry: live(500, 60_000_000),
        });
        st.absorb(SnapItem::Trustline {
            key,
            entry: live(900, 50_000_000),
        });

        let e = st.trustlines[&key];
        assert!(
            !e.live,
            "the tombstone was seen FIRST — an older live record must not revive it"
        );
        assert_eq!(
            st.superseded, 2,
            "both older records are superseded history"
        );
        assert_eq!(st.live_trustlines(), 0);
    }

    /// The verdict is the one rule the compare pass reports and the seed acts
    /// on. Each arm below is a production decision: mis-mapping any one of them
    /// either hides a live holding or writes a false number.
    #[test]
    fn verdict_covers_every_bucket() {
        const CP: u32 = 100;

        // Our writer already closed it — no correction, but the snapshot entry
        // is still MATCHED, or our own closure would be counted as a network
        // gap and re-inserted as live.
        let mut e = live(5, 90);
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&mut e), CP),
            Verdict::AlreadyClosed
        );
        assert!(
            e.matched,
            "an already-closed row must still claim its entry"
        );

        let mut e = live(5, 90);
        assert_eq!(verdict(&row(5, 90, 0), Some(&mut e), CP), Verdict::Agree);

        // Amounts differ: who is newer decides whether we adopt or keep.
        let mut e = live(5, 95);
        assert_eq!(
            verdict(&row(9, 90, 0), Some(&mut e), CP),
            Verdict::HealFromSnapshot
        );
        let mut e = live(5, 90);
        assert_eq!(
            verdict(&row(9, 95, 0), Some(&mut e), CP),
            Verdict::DivergentOursNewer
        );

        let mut e = live(5, 95);
        assert_eq!(verdict(&row(5, 90, 0), Some(&mut e), CP), Verdict::Stale);

        // Absent from the snapshot: zero is a closure, positive is a ghost —
        // never folded together, because the second means our data lies.
        assert_eq!(verdict(&row(0, 90, 0), None, CP), Verdict::Closure);
        assert_eq!(verdict(&row(42, 90, 0), None, CP), Verdict::Ghost);

        // Present but dead reads the same as absent.
        let mut dead = SnapEntry::dead();
        assert_eq!(
            verdict(&row(0, 90, 0), Some(&mut dead), CP),
            Verdict::Closure
        );

        // Our row is newer than the checkpoint, so the SNAPSHOT is the stale
        // side; closing here would delete a holding created in the gap.
        assert_eq!(
            verdict(&row(0, 101, 0), None, CP),
            Verdict::NewerThanCheckpoint
        );
        assert_eq!(
            verdict(&row(42, 101, 0), None, CP),
            Verdict::NewerThanCheckpoint
        );

        // AT the checkpoint counts as too fresh to close. Writing here would
        // land at the SAME RMT version as the row already in the table, and the
        // merge would pick a winner arbitrarily — the closure could lose and
        // the ghost survive, undetectably.
        assert_eq!(
            verdict(&row(0, i64::from(CP), 0), None, CP),
            Verdict::NewerThanCheckpoint,
            "a row last touched at the checkpoint must not be closed at that same version"
        );

        // Same ledger, different amount: freshness cannot arbitrate, so this is
        // a parser-defect signal and must not hide inside 'ours newer, kept'.
        let mut e = live(5, 90);
        assert_eq!(
            verdict(&row(9, 90, 0), Some(&mut e), CP),
            Verdict::DivergentSameLedger
        );
    }

    /// The costliest outcome this comparison can produce: our row says CLOSED
    /// while the network holds the entry LIVE at a newer ledger. Marking it
    /// matched without checking liveness suppressed it from the report AND from
    /// the re-insert, hiding funds a user actually has — permanently, silently.
    #[test]
    fn a_closure_the_network_contradicts_is_reported_not_swallowed() {
        const CP: u32 = 100;

        // Our closure at 90, network alive at 95 — our closure is the stale fact.
        let mut newer = live(5, 95);
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&mut newer), CP),
            Verdict::ClosedButLive,
            "the network says live at a NEWER ledger — our closure is wrong"
        );
        assert!(
            newer.matched,
            "still matched: pass 2 must not ALSO re-insert it blindly"
        );

        // Our closure at 90, network's evidence of life is older — our closure
        // stands, and this is the ordinary case.
        let mut older = live(5, 85);
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&mut older), CP),
            Verdict::AlreadyClosed
        );

        // Network agrees the entry is gone.
        let mut dead = SnapEntry::dead();
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&mut dead), CP),
            Verdict::AlreadyClosed
        );
        assert!(dead.matched, "our own closure is not a network gap");
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

        let mut state = SnapshotState::default();
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
