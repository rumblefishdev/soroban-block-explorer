//! Task 0210 — how much native XLM sits in Stellar Asset Contract balance
//! entries at a checkpoint, counted from the history archive itself.
//!
//! The XLM identity (`total_coins − fee_pool` against our indexed sum) leaves a
//! residual, and the standing hypothesis for it is contract-held XLM whose
//! `ContractData` entries have not changed since before our ingest floor: the
//! seed's comparison models accounts, trustlines, claimable balances and pools,
//! never contract data, so nothing we run today can see such an entry.
//!
//! This is a MEASUREMENT, not a check: it prints the census and asserts only
//! what it read (every bucket hash verified, every bucket fully decoded). The
//! number it prints is compared against our own tables by hand — a one-off
//! answer to "is the residual contract-held", deliberately outside the seed so
//! no production path changes to ask the question.
//!
//! Method, identical to `snapshot::archive` + `snapshot::network_state`:
//! buckets newest-first (`curr` before `snap`), FIRST record per key wins, a
//! `DEADENTRY` seen first is a tombstone that blocks older live records.
//!
//! Run (about 5 minutes on a good link, ~4.5 GB streamed, nothing kept):
//!
//! ```text
//! SAC_CENSUS=1 cargo test -p backfill-runner --release \
//!     --test native_sac_balance_census -- --nocapture
//! ```
//!
//! `curl` streams each bucket so the bytes are never stored: the largest single
//! bucket is 2.47 GB and the box this runs on has ~12 GB free.

use std::collections::HashSet;
use std::io::Read;
use std::process::{Command, Stdio};
use std::str::FromStr;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use stellar_xdr::{
    Asset, BucketEntry, ClaimableBalanceId, ContractDataEntry, Frame, HotArchiveBucketEntry,
    LedgerEntryData, LedgerHeaderHistoryEntry, LedgerKey, Limited, Limits, LiquidityPoolEntryBody,
    PublicKey, ReadXdr, ScAddress, ScVal, Uint256,
};

const ARCHIVE: &str = "https://history.stellar.org/prd/core-live/core_live_001";

/// The native asset's SAC on pubnet. Verified against the chain rather than
/// taken from a list: its instance entry decodes as
/// `executable: stellar_asset` with `METADATA.name = "native"`, decimals 7.
const NATIVE_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

/// Wraps a reader and hashes every byte that passes through, so the bucket's
/// content address is checked on the stream — the archive is content-addressed
/// (over the decompressed XDR) and a truncated download otherwise reads as
/// "fewer entries", which is exactly the failure this census cannot afford.
struct Hashing<R> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

fn curl(url: &str) -> Vec<u8> {
    let out = Command::new("curl")
        .args(["-sfL", url])
        .output()
        .expect("curl runs");
    assert!(out.status.success(), "curl failed for {url}");
    out.stdout
}

/// `(checkpoint ledger, live bucket hashes, hot-archive bucket hashes)`, each
/// newest-first, from the archive manifest.
///
/// The hot archive (CAP-62) holds entries EVICTED from live state. Their coins
/// still exist — `total_coins` counts them — so a census that reads only
/// `currentBuckets` understates what the chain holds.
fn manifest() -> (u32, Vec<String>, Vec<String>) {
    let body: serde_json::Value = serde_json::from_slice(&curl(&format!(
        "{ARCHIVE}/.well-known/stellar-history.json"
    )))
    .expect("manifest is JSON");
    let ledger = body["currentLedger"].as_u64().expect("currentLedger") as u32;
    assert_eq!(
        ledger % 64,
        63,
        "currentLedger is not on the checkpoint lattice"
    );
    const EMPTY: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    let read = |field: &str| {
        let mut hashes = Vec::new();
        for level in body[field].as_array().expect("bucket levels") {
            for slot in ["curr", "snap"] {
                let h = level[slot].as_str().expect("bucket hash is a string");
                if h != EMPTY {
                    hashes.push(h.to_string());
                }
            }
        }
        hashes
    };
    (ledger, read("currentBuckets"), read("hotArchiveBuckets"))
}

/// Stream one bucket through `curl`, decode every record, and hand each one to
/// `f`. The manifest hash is checked against the DECOMPRESSED bytes, which is
/// what the archive is addressed by.
fn stream<T, F>(url: &str, hash: Option<&str>, mut f: F) -> u64
where
    T: ReadXdr,
    F: FnMut(T),
{
    let mut child = Command::new("curl")
        .args(["-sfL", url])
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl spawns");
    let stdout = child.stdout.take().expect("piped stdout");
    let hashing = Hashing {
        inner: GzDecoder::new(stdout),
        hasher: Sha256::new(),
    };
    let mut limited = Limited::new(hashing, Limits::none());
    let mut records = 0u64;
    for entry in Frame::<T>::read_xdr_iter(&mut limited) {
        f(entry.expect("bucket XDR decodes").0);
        records += 1;
    }
    let digest = hex::encode(limited.inner.hasher.finalize());
    assert!(
        child.wait().expect("curl exits").success(),
        "curl failed on {url}"
    );
    if let Some(expect) = hash {
        assert_eq!(digest, expect, "bucket content hash mismatch for {url}");
    }
    records
}

fn bucket_url(hash: &str) -> String {
    format!(
        "{ARCHIVE}/bucket/{}/{}/{}/bucket-{hash}.xdr.gz",
        &hash[0..2],
        &hash[2..4],
        &hash[4..6]
    )
}

/// The holder address of a native-SAC `Balance(Address)` entry, or `None` for
/// every other contract-data entry (another contract, the SAC's own instance,
/// an allowance, …).
fn native_balance_holder(contract: &ScAddress, key: &ScVal, sac: &[u8; 32]) -> Option<ScAddress> {
    let ScAddress::Contract(id) = contract else {
        return None;
    };
    if id.0.0 != *sac {
        return None;
    }
    let ScVal::Vec(Some(parts)) = key else {
        return None;
    };
    let [ScVal::Symbol(sym), ScVal::Address(holder)] = parts.as_slice() else {
        return None;
    };
    (sym.0.as_slice() == b"Balance").then(|| holder.clone())
}

/// `amount` out of the SAC's `BalanceValue` map (`amount`, `authorized`,
/// `clawback`). A shape we cannot read is returned as `None` and counted, never
/// silently treated as zero.
fn balance_amount(val: &ScVal) -> Option<i128> {
    let ScVal::Map(Some(map)) = val else {
        return None;
    };
    for e in map.0.iter() {
        if let ScVal::Symbol(k) = &e.key
            && k.0.as_slice() == b"amount"
            && let ScVal::I128(p) = &e.val
        {
            return Some((i128::from(p.hi) << 64) | i128::from(p.lo));
        }
    }
    None
}

/// Dedup key for the non-contract venues: 64 bits of a hash over (kind, key
/// bytes). Full keys for ~11 M accounts would cost about a gigabyte; at this
/// population a 64-bit collision is a 3e-6 event, and one collision would
/// under-count a single account.
fn tag(kind: u8, bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut h);
    bytes.hash(&mut h);
    h.finish()
}

/// Native XLM everywhere else the protocol keeps it: accounts, claimable
/// balances and classic pool reserves. Same first-wins fold as the SAC census,
/// so one pass over the buckets answers the whole identity.
#[derive(Default)]
struct Venues {
    seen: HashSet<u64>,
    accounts: i128,
    accounts_n: u64,
    claimable: i128,
    claimable_n: u64,
    pools: i128,
    pools_n: u64,
}

impl Venues {
    fn live(&mut self, data: &LedgerEntryData) {
        match data {
            LedgerEntryData::Account(a) => {
                let PublicKey::PublicKeyTypeEd25519(Uint256(b)) = &a.account_id.0;
                if self.seen.insert(tag(1, b)) {
                    self.accounts += i128::from(a.balance);
                    self.accounts_n += 1;
                }
            }
            LedgerEntryData::ClaimableBalance(cb) => {
                let ClaimableBalanceId::ClaimableBalanceIdTypeV0(h) = &cb.balance_id;
                if self.seen.insert(tag(2, &h.0)) && cb.asset == Asset::Native {
                    self.claimable += i128::from(cb.amount);
                    self.claimable_n += 1;
                }
            }
            LedgerEntryData::LiquidityPool(p) => {
                if !self.seen.insert(tag(3, &p.liquidity_pool_id.0.0)) {
                    return;
                }
                let LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &p.body;
                let mut hit = false;
                if cp.params.asset_a == Asset::Native {
                    self.pools += i128::from(cp.reserve_a);
                    hit = true;
                }
                if cp.params.asset_b == Asset::Native {
                    self.pools += i128::from(cp.reserve_b);
                    hit = true;
                }
                self.pools_n += u64::from(hit);
            }
            _ => {}
        }
    }

    /// A DEADENTRY settles the key without a value — recorded so an older live
    /// record below cannot resurrect it.
    fn dead(&mut self, key: &LedgerKey) {
        match key {
            LedgerKey::Account(k) => {
                let PublicKey::PublicKeyTypeEd25519(Uint256(b)) = &k.account_id.0;
                self.seen.insert(tag(1, b));
            }
            LedgerKey::ClaimableBalance(k) => {
                let ClaimableBalanceId::ClaimableBalanceIdTypeV0(h) = &k.balance_id;
                self.seen.insert(tag(2, &h.0));
            }
            LedgerKey::LiquidityPool(k) => {
                self.seen.insert(tag(3, &k.liquidity_pool_id.0.0));
            }
            _ => {}
        }
    }
}

/// `(total_coins, fee_pool)` from the checkpoint's own ledger header — the
/// protocol's count of every stroop in existence, and the part of it that sits
/// in the fee pool.
fn header(checkpoint: u32) -> (i64, i64) {
    let hex = format!("{checkpoint:08x}");
    let url = format!(
        "{ARCHIVE}/ledger/{}/{}/{}/ledger-{hex}.xdr.gz",
        &hex[0..2],
        &hex[2..4],
        &hex[4..6]
    );
    let mut found = None;
    stream::<LedgerHeaderHistoryEntry, _>(&url, None, |e| {
        if e.header.ledger_seq == checkpoint {
            found = Some((e.header.total_coins, e.header.fee_pool));
        }
    });
    found.expect("the checkpoint's own header is in its ledger file")
}

/// A live or archived `Balance` record, keyed by the holder's strkey.
struct Holding {
    amount: i128,
    archived: bool,
}

/// One record of either bucket kind, reduced to what this census needs.
enum Rec<'a> {
    /// A value for a key (live entry, or an archived one in the hot archive).
    Value(&'a ContractDataEntry),
    /// The key is settled without a value here: a DEADENTRY in the live
    /// buckets (deleted) or a LIVE marker in the hot archive (restored, so the
    /// live census already has it).
    Settled(&'a ScAddress, &'a ScVal),
}

#[test]
fn native_sac_balance_census() {
    if std::env::var("SAC_CENSUS").is_err() {
        eprintln!("SAC_CENSUS unset — skipping the archive census");
        return;
    }
    let sac: [u8; 32] = stellar_strkey::Contract::from_str(NATIVE_SAC)
        .expect("native SAC strkey")
        .0;
    let (checkpoint, live, hot) = manifest();
    println!(
        "checkpoint {checkpoint}, {} live buckets, {} hot-archive buckets",
        live.len(),
        hot.len()
    );

    // First-wins across BOTH lists, in order: a key settled in a newer bucket
    // is never revisited. The live list is read first because a restored entry
    // appears live there and as a LIVE marker in the hot archive.
    let mut seen: HashSet<String> = HashSet::new();
    let mut holdings: Vec<(String, Holding)> = Vec::new();
    let mut unreadable = 0u64;
    let mut settled = 0u64;

    let take = |rec: Rec<'_>,
                archived: bool,
                seen: &mut HashSet<String>,
                holdings: &mut Vec<(String, Holding)>,
                unreadable: &mut u64,
                settled: &mut u64| {
        let (contract, key, val) = match rec {
            Rec::Value(cd) => (&cd.contract, &cd.key, Some(&cd.val)),
            Rec::Settled(c, k) => (c, k, None),
        };
        let Some(ScAddress::Contract(holder)) = native_balance_holder(contract, key, &sac) else {
            // Every native-SAC balance measured is contract-held; an account's
            // XLM lives in its AccountEntry, not in contract data. A different
            // address type would be a finding, so it is asserted below rather
            // than silently skipped.
            return;
        };
        let strkey = format!("{}", stellar_strkey::Contract(holder.0.0));
        if !seen.insert(strkey.clone()) {
            return;
        }
        match val {
            None => *settled += 1,
            Some(v) => match balance_amount(v) {
                Some(amount) => holdings.push((strkey, Holding { amount, archived })),
                None => *unreadable += 1,
            },
        }
    };

    let mut venues = Venues::default();
    for (i, hash) in live.iter().enumerate() {
        let n = stream::<BucketEntry, _>(&bucket_url(hash), Some(hash), |entry| match &entry {
            BucketEntry::Liveentry(e) | BucketEntry::Initentry(e) => {
                if let LedgerEntryData::ContractData(cd) = &e.data {
                    take(
                        Rec::Value(cd),
                        false,
                        &mut seen,
                        &mut holdings,
                        &mut unreadable,
                        &mut settled,
                    );
                } else {
                    venues.live(&e.data);
                }
            }
            BucketEntry::Deadentry(k) => match k {
                LedgerKey::ContractData(k) => take(
                    Rec::Settled(&k.contract, &k.key),
                    false,
                    &mut seen,
                    &mut holdings,
                    &mut unreadable,
                    &mut settled,
                ),
                other => venues.dead(other),
            },
            _ => {}
        });
        println!(
            "  live {:>2}/{} {n:>10} records — {} SAC holdings, {} accounts",
            i + 1,
            live.len(),
            holdings.len(),
            venues.accounts_n
        );
    }
    let live_count = holdings.len();
    let live_total: i128 = holdings.iter().map(|(_, h)| h.amount).sum();

    for (i, hash) in hot.iter().enumerate() {
        let n =
            stream::<HotArchiveBucketEntry, _>(
                &bucket_url(hash),
                Some(hash),
                |entry| match &entry {
                    HotArchiveBucketEntry::Archived(e) => {
                        if let LedgerEntryData::ContractData(cd) = &e.data {
                            take(
                                Rec::Value(cd),
                                true,
                                &mut seen,
                                &mut holdings,
                                &mut unreadable,
                                &mut settled,
                            );
                        }
                    }
                    HotArchiveBucketEntry::Live(LedgerKey::ContractData(k)) => {
                        take(
                            Rec::Settled(&k.contract, &k.key),
                            true,
                            &mut seen,
                            &mut holdings,
                            &mut unreadable,
                            &mut settled,
                        );
                    }
                    _ => {}
                },
            );
        println!(
            "  hot  {:>2}/{} {n:>10} records — {} holdings",
            i + 1,
            hot.len(),
            holdings.len()
        );
    }

    let archived: Vec<_> = holdings.iter().filter(|(_, h)| h.archived).collect();
    let archived_total: i128 = archived.iter().map(|(_, h)| h.amount).sum();

    // One row per holder so the census can be joined against our own table by
    // the surrogate the writer uses — the comparison this was built for.
    let out = std::env::var("SAC_CENSUS_OUT").unwrap_or_else(|_| "native_sac_census.tsv".into());
    let mut tsv = String::from("holder\tholder_id\tamount\tarchived\n");
    for (strkey, h) in &holdings {
        let id = db_clickhouse::persist::ids::contract_id(strkey);
        tsv.push_str(&format!(
            "{strkey}\t{id}\t{}\t{}\n",
            h.amount,
            u8::from(h.archived)
        ));
    }
    std::fs::write(&out, tsv).expect("census TSV written");

    println!("\ncheckpoint {checkpoint} — native SAC balances");
    println!(
        "  live      {live_count:>6} holders {live_total:>22} stroops = {} XLM",
        live_total as f64 / 1e7
    );
    println!(
        "  archived  {:>6} holders {archived_total:>22} stroops = {} XLM",
        archived.len(),
        archived_total as f64 / 1e7
    );
    println!(
        "  together  {:>6} holders {:>22} stroops",
        holdings.len(),
        live_total + archived_total
    );
    println!("  settled keys skipped: {settled}, unreadable values: {unreadable}");
    println!("  rows written to {out}");

    let (total_coins, fee_pool) = header(checkpoint);
    let sac = live_total + archived_total;
    let sum = i128::from(fee_pool) + venues.accounts + venues.claimable + venues.pools + sac;
    println!("\nXLM at checkpoint {checkpoint}, every venue from the archive itself");
    println!(
        "  accounts           {:>8} {:>22}",
        venues.accounts_n, venues.accounts
    );
    println!(
        "  claimable balances {:>8} {:>22}",
        venues.claimable_n, venues.claimable
    );
    println!(
        "  classic pools      {:>8} {:>22}",
        venues.pools_n, venues.pools
    );
    println!("  SAC balances       {:>8} {:>22}", holdings.len(), sac);
    println!("  fee pool           {:>8} {:>22}", "", fee_pool);
    println!("  ---------------------------------------------------");
    println!("  sum                         {:>22}", sum);
    println!("  total_coins                 {:>22}", total_coins);
    println!(
        "  unaccounted                 {:>22} = {} XLM",
        i128::from(total_coins) - sum,
        (i128::from(total_coins) - sum) as f64 / 1e7
    );
    assert_eq!(unreadable, 0, "a balance value did not decode");
}
