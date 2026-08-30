//! The network's state — records from the archive folded into DISTINCT live
//! entries, keyed the way our own tables key them (task 0463, issue #377).
//!
//! [`crate::snapshot::archive`] gets the bytes; this module decides what they
//! MEAN — every record becomes a [`NetFact`] keyed the way `balances` keys
//! it, folded into one [`NetworkState`] of [`NetHolding`]s.
//! [`crate::snapshot::verdict`] then compares one of our rows against it.
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

use db_clickhouse::persist::ids;

use crate::error::BackfillError;
use crate::snapshot::archive::{
    BucketList, PUBNET_ARCHIVE, SnapshotRecord, archive_client, fetch_bucket_list,
    stream_bucket_from_url,
};

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
pub struct NetHolding {
    pub live: bool,
    /// The entry's OWN `lastModifiedLedgerSeq`. Zero for a dead entry, which
    /// carries only a key. Anything seeded from this versions on it — never on
    /// a window boundary (the task 0492 defect).
    pub ledger: u32,
    /// What the holder holds, in the ledger's own units: stroops for a native
    /// XLM holding, the asset's own smallest unit for a trustline. XDR types
    /// both as `int64`. Zero for a dead entry.
    pub balance: i64,
    /// Set during the comparison when a row on our side matched this key.
    /// Whatever stays false at the end is what the network has and we do not.
    pub matched: bool,
}

impl NetHolding {
    pub(crate) fn live(ledger: u32, balance: i64) -> Self {
        Self {
            live: true,
            ledger,
            balance,
            matched: false,
        }
    }
    pub(crate) fn dead() -> Self {
        Self {
            live: false,
            ledger: 0,
            balance: 0,
            matched: false,
        }
    }
}

/// One classified record, in our key space. Entry types we do not model are
/// counted, not silently dropped.
enum NetFact {
    /// An `AccountEntry` carries TWO facts: the account exists, and it holds
    /// this much native XLM. Native is not a trustline — it lives on the
    /// account — so "absent from the snapshot" for a native holding means the
    /// ACCOUNT is gone. That is the ~52k merged-account ghost case.
    Account {
        holder_id: i64,
        entry: NetHolding,
        /// `None` only for a DEAD account — a tombstone carries a key, no entry.
        detail: Option<Box<AccountDetail>>,
    },
    /// A classic credit trustline, keyed onto our surrogate pair.
    Trustline { key: HoldingKey, entry: NetHolding },
    /// A pool-share trustline. Same ledger entry type, DIFFERENT table on our
    /// side (`lp_positions`), so it is tallied apart rather than folded into
    /// the `balances` comparison and counted as a phantom.
    PoolShare {
        holder_id: i64,
        pool_id: [u8; 32],
        entry: NetHolding,
    },
    /// A classic `LiquidityPoolEntry` (constant product is the only body the
    /// protocol defines).
    Pool { pool_id: [u8; 32], pool: NetPool },
}

/// First-wins state of one classic `LiquidityPoolEntry` — everything the LP
/// seed (task 0374 K4-6 follow-up) needs to stub a missing `liquidity_pools`
/// dimension row and its snapshot: a pre-floor pool untouched since the
/// ingest floor has NO row on our side at all, exactly like its holders.
#[derive(Debug, Clone)]
pub struct NetPool {
    pub live: bool,
    /// The entry's OWN `lastModifiedLedgerSeq`; 0 for a dead key.
    pub ledger: u32,
    /// `(asset_type, code, issuer strkey)` per leg, in the XDR vocabulary
    /// the classic pool columns store: 0 native, 1 alphanum4, 2 alphanum12.
    pub asset_a: (i16, String, String),
    pub asset_b: (i16, String, String),
    pub fee_bps: i32,
    pub reserve_a: i64,
    pub reserve_b: i64,
    pub total_shares: i64,
    pub trustline_count: i64,
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
/// ([`super::report`]) and the write ([`super::seed`]). They previously kept
/// separate state types with separate first-wins logic — two chances to
/// disagree about what the network says.
#[derive(Debug, Default)]
pub struct NetworkState {
    /// `holder_id` → the account's existence and its NATIVE holding.
    pub accounts: std::collections::HashMap<i64, NetHolding>,
    /// Classic credit trustlines.
    pub trustlines: std::collections::HashMap<HoldingKey, NetHolding>,
    /// Pool shares, kept separate — see [`NetFact::PoolShare`].
    pub pool_shares: std::collections::HashMap<(i64, [u8; 32]), NetHolding>,
    /// Classic `LiquidityPoolEntry` records — the pool-side truth the LP seed
    /// stubs missing `liquidity_pools` rows from. Same first-wins rule.
    pub pools: std::collections::HashMap<[u8; 32], NetPool>,
    /// Per-account identity, signers and thresholds, for `account_entry_state`.
    pub account_details: std::collections::HashMap<i64, AccountDetail>,
    /// asset surrogate → `(code, issuer strkey)`, for `assets` dimension stubs.
    /// Bounded by network asset cardinality (~391k), not trustline count.
    pub asset_registry: std::collections::HashMap<i64, (String, String)>,
    /// Records of entry types this comparison does not model (offers, contract
    /// data, TTL, …). Reported so the pass never looks more complete than it is.
    pub unmodelled: u64,
    /// TrustLineEntry records whose asset is NATIVE. The protocol forbids them
    /// (`changeTrust` on native is rejected), so this must stay 0 — but
    /// "impossible" folded into `unmodelled` would sit invisible among tens of
    /// millions of offers and contract data. Counted apart so a protocol change
    /// or a decode defect announces itself instead of being assumed away.
    pub native_trustlines: u64,
    /// Records superseded by an earlier (newer) record for the same key. This
    /// is the number first-wins actually suppressed.
    pub superseded: u64,
}

/// Keep the FIRST value seen for a key; count every later one as superseded.
///
/// The three maps have different key types, so this is generic — the same rule
/// applied identically to accounts, trustlines and pool shares, which is the
/// point: one place to read, one place to get it wrong.
fn first_wins<K: std::hash::Hash + Eq>(
    map: &mut std::collections::HashMap<K, NetHolding>,
    key: K,
    value: NetHolding,
    superseded: &mut u64,
) {
    use std::collections::hash_map::Entry;
    match map.entry(key) {
        Entry::Vacant(slot) => {
            slot.insert(value);
        }
        Entry::Occupied(_) => *superseded += 1,
    }
}

impl NetworkState {
    /// Insert under FIRST-WINS: the bucket list is ordered newest-first, so the
    /// first record seen for a key is the live state and every later one is
    /// history. A `DeadEntry` seen first must NOT be overwritten by an older
    /// `LiveEntry` further down — that is exactly the resurrection bug this
    /// whole effort is about, in the source format.
    fn absorb(&mut self, item: NetFact) {
        match item {
            NetFact::Account {
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
                first_wins(&mut self.accounts, holder_id, entry, &mut self.superseded);
            }
            NetFact::Trustline { key, entry } => {
                first_wins(&mut self.trustlines, key, entry, &mut self.superseded);
            }
            NetFact::PoolShare {
                holder_id,
                pool_id,
                entry,
            } => {
                first_wins(
                    &mut self.pool_shares,
                    (holder_id, pool_id),
                    entry,
                    &mut self.superseded,
                );
            }
            NetFact::Pool { pool_id, pool } => {
                // Same first-wins rule, spelled inline — `first_wins` is
                // typed over `NetHolding` and a `NetPool` carries more.
                use std::collections::hash_map::Entry;
                match self.pools.entry(pool_id) {
                    Entry::Vacant(slot) => {
                        slot.insert(pool);
                    }
                    Entry::Occupied(_) => self.superseded += 1,
                }
            }
        }
    }

    /// Classify one decoded record and fold it in. The single entry point the
    /// report and the write share, so they cannot disagree about what the
    /// snapshot says.
    pub fn absorb_record(&mut self, rec: &SnapshotRecord) {
        match classify(rec) {
            Some(item) => {
                if let NetFact::Trustline { key, .. } = &item
                    && let Some((code, issuer)) = trustline_identity(rec)
                {
                    self.asset_registry
                        .entry(key.asset_id)
                        .or_insert((code, issuer));
                }
                self.absorb(item);
            }
            None if is_native_trustline(rec) => self.native_trustlines += 1,
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
    pub fn live_pools(&self) -> usize {
        self.pools.values().filter(|p| p.live).count()
    }
}

/// A TrustLineEntry (live or dead) whose asset is NATIVE — the one shape
/// [`trustline_asset_id`] rejects that is not routed elsewhere by [`classify`].
///
/// Spelled out rather than inferred from "`classify` returned `None`": that
/// would make the count depend on the ORDER of `classify`'s match arms, where
/// `A::PoolShare` is handled before the `None` fallthrough. Reorder those arms
/// and every pool share would silently become a native trustline in the report.
fn is_native_trustline(rec: &SnapshotRecord) -> bool {
    use stellar_xdr::{LedgerEntryData as D, LedgerKey as K, TrustLineAsset as A};
    match rec {
        SnapshotRecord::Live(e) => {
            matches!(&e.data, D::Trustline(t) if matches!(t.asset, A::Native))
        }
        SnapshotRecord::Dead(k) => {
            matches!(k.as_ref(), K::Trustline(t) if matches!(t.asset, A::Native))
        }
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
        // AccountEntry, and `verdict::holding_for` routes every native row of
        // ours to `accounts`. Mapping this to NATIVE_ASSET_ID would file such a
        // record into `trustlines`, where nothing can ever claim it — it would
        // read as a missing trustline and insert a SECOND native row for that
        // holder. `None` counts it as unmodelled, like every other entry type
        // this comparison does not handle: still total, and wrong in the
        // direction that is merely visible instead of the one that corrupts.
        A::Native => None,
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

/// `(asset_type, code, issuer strkey)` in the classic pool-column vocabulary
/// — the XDR `AssetType` discriminants the `liquidity_pools` pair columns
/// store (0 native / 1 alphanum4 / 2 alphanum12).
fn pool_leg(asset: &stellar_xdr::Asset) -> (i16, String, String) {
    use stellar_xdr::Asset as A;
    match asset {
        A::Native => (0, String::new(), String::new()),
        A::CreditAlphanum4(a) => (
            1,
            xdr_parser::asset_code::asset_code_str(a.asset_code.as_slice()),
            a.issuer.to_string(),
        ),
        A::CreditAlphanum12(a) => (
            2,
            xdr_parser::asset_code::asset_code_str(a.asset_code.as_slice()),
            a.issuer.to_string(),
        ),
    }
}

/// Classify one decoded record into our key space, or `None` for an entry type
/// this comparison does not model.
fn classify(rec: &SnapshotRecord) -> Option<NetFact> {
    use stellar_xdr::{LedgerEntryData as D, LedgerKey as K, TrustLineAsset as A};
    match rec {
        SnapshotRecord::Live(e) => match &e.data {
            D::LiquidityPool(p) => {
                // Constant product is the only body the protocol defines; a
                // future variant would fail this let and fall to unmodelled
                // rather than decode wrong.
                let stellar_xdr::LiquidityPoolEntryBody::LiquidityPoolConstantProduct(cp) = &p.body;
                let params = &cp.params;
                Some(NetFact::Pool {
                    pool_id: p.liquidity_pool_id.0.0,
                    pool: NetPool {
                        live: true,
                        ledger: e.last_modified_ledger_seq,
                        asset_a: pool_leg(&params.asset_a),
                        asset_b: pool_leg(&params.asset_b),
                        fee_bps: params.fee,
                        reserve_a: cp.reserve_a,
                        reserve_b: cp.reserve_b,
                        total_shares: cp.total_pool_shares,
                        trustline_count: cp.pool_shares_trust_line_count,
                    },
                })
            }
            D::Account(a) => Some(NetFact::Account {
                holder_id: ids::address_id(&a.account_id.to_string()),
                entry: NetHolding::live(e.last_modified_ledger_seq, a.balance),
                detail: Some(Box::new(account_detail(a))),
            }),
            D::Trustline(t) => {
                let holder_id = ids::address_id(&t.account_id.to_string());
                let entry = NetHolding::live(e.last_modified_ledger_seq, t.balance);
                match (&t.asset, trustline_asset_id(&t.asset)) {
                    (A::PoolShare(p), _) => Some(NetFact::PoolShare {
                        holder_id,
                        pool_id: p.0.0,
                        entry,
                    }),
                    (_, Some(asset_id)) => Some(NetFact::Trustline {
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
            K::LiquidityPool(p) => Some(NetFact::Pool {
                pool_id: p.liquidity_pool_id.0.0,
                pool: NetPool {
                    live: false,
                    ledger: 0,
                    asset_a: (0, String::new(), String::new()),
                    asset_b: (0, String::new(), String::new()),
                    fee_bps: 0,
                    reserve_a: 0,
                    reserve_b: 0,
                    total_shares: 0,
                    trustline_count: 0,
                },
            }),
            K::Account(a) => Some(NetFact::Account {
                holder_id: ids::address_id(&a.account_id.to_string()),
                entry: NetHolding::dead(),
                detail: None,
            }),
            K::Trustline(t) => {
                let holder_id = ids::address_id(&t.account_id.to_string());
                match (&t.asset, trustline_asset_id(&t.asset)) {
                    (A::PoolShare(p), _) => Some(NetFact::PoolShare {
                        holder_id,
                        pool_id: p.0.0,
                        entry: NetHolding::dead(),
                    }),
                    (_, Some(asset_id)) => Some(NetFact::Trustline {
                        key: HoldingKey {
                            holder_id,
                            asset_id,
                        },
                        entry: NetHolding::dead(),
                    }),
                    (_, None) => None,
                }
            }
            _ => None,
        },
    }
}

/// Resolve a bucket list and fold every bucket into a deduplicated
/// [`NetworkState`]: build the HTTP client, take the freshest
/// checkpoint, stream all 21 buckets, print the distinct-entry report.
///
/// The checkpoint is always the freshest the archive advertises. That is
/// complete by construction — stellar-core writes the `.well-known` manifest
/// LAST, as an atomic commit point, and discards a failed publication rather
/// than exposing half of it.
///
/// Memory is the distinct-entry count, not the record count: ~124M records
/// collapse onto the accounts + trustlines the network actually holds.
pub(crate) async fn open_snapshot(
    label: &str,
) -> Result<(BucketList, NetworkState, String), BackfillError> {
    let started = std::time::Instant::now();
    let http = archive_client()?;
    let list = fetch_bucket_list(&http, PUBNET_ARCHIVE).await?;
    let n_buckets = list.hashes.len();
    println!(
        "checkpoint ledger {} — {n_buckets} buckets{label}",
        list.checkpoint_ledger
    );

    let mut state = NetworkState::default();
    for (i, hash) in list.hashes.iter().enumerate() {
        let url = BucketList::url(PUBNET_ARCHIVE, hash);
        let t0 = std::time::Instant::now();
        let bytes = stream_bucket_from_url(&http, &url, hash, |rec| {
            state.absorb_record(&rec);
            Ok(())
        })
        .await?;
        println!(
            "  [{:>2}/{n_buckets}] {bytes:>10} B  {:>6.1}s",
            i + 1,
            t0.elapsed().as_secs_f64()
        );
    }
    // The snapshot is the input whose short read is CATASTROPHIC, and until
    // now it was the only input without a floor. Our side has two
    // (`MIN_OUR_ROWS`, the dimension-id floors) on the argument that a short
    // read is indistinguishable from a real one downstream — but a short read
    // of OURS over-inserts, which the live writer's newer rows correct, while a
    // short read of the SNAPSHOT sends every unlisted key into the verdict's
    // absence arm: tens of millions of live holdings zeroed and closed at the
    // checkpoint version, which outranks every row already in the table.
    //
    // What this actually guards is worth being precise about, because the
    // obvious threat is NOT the one left. A bad download fails on the
    // per-bucket SHA-256; a 404 on `error_for_status`; a manifest in an
    // unexpected shape on the missing-slot check in `fetch_bucket_list`. What
    // survives all three is a decode that SUCCEEDS and recognises less than it
    // should — a protocol change `classify` does not model, or a regression in
    // our own dedup. Every byte verifies, nothing errors, and the maps come
    // back thin. That is OUR failure mode, not the archive's, and this is the
    // only thing that catches it.
    //
    // Floors sit 2-3x below the measured population (2026-08-18: 21 buckets,
    // 10,863,731 live accounts, 32,344,912 live trustlines), so they cannot
    // fire on a shrinking network — only on a pass that stopped seeing things.
    const MIN_BUCKETS: usize = 10;
    const MIN_LIVE_ACCOUNTS: usize = 5_000_000;
    const MIN_LIVE_TRUSTLINES: usize = 15_000_000;
    let (accounts, trustlines) = (state.live_accounts(), state.live_trustlines());
    if n_buckets < MIN_BUCKETS || accounts < MIN_LIVE_ACCOUNTS || trustlines < MIN_LIVE_TRUSTLINES {
        return Err(BackfillError::Incomplete(format!(
            "snapshot looks short: {n_buckets} buckets, {accounts} live accounts, \
             {trustlines} live trustlines (floors {MIN_BUCKETS} / {MIN_LIVE_ACCOUNTS} / \
             {MIN_LIVE_TRUSTLINES}) — refusing to read the gap as network-wide closures"
        )));
    }

    let source_report = report_state(
        &state,
        list.checkpoint_ledger,
        started.elapsed().as_secs_f64(),
    );
    println!("{source_report}");

    Ok((list, state, source_report))
}

/// DISTINCT-entry report after first-wins, printed before our rows are folded
/// in, and returned so the durable `summary.txt` carries the health of the
/// SOURCE beside the health of the comparison — the raw record count cannot be
/// compared with our
/// tables, because a key appears in as many buckets as it has versions.
pub fn report_state(state: &NetworkState, checkpoint_ledger: u32, secs: f64) -> String {
    use std::fmt::Write as _;
    let dead_accounts = state.accounts.len() - state.live_accounts();
    let dead_trustlines = state.trustlines.len() - state.live_trustlines();
    let mut out = format!(
        "\n  DISTINCT entries at checkpoint {checkpoint_ledger} (first-wins applied)\n  \
         entity            live         deleted\n"
    );
    let _ = writeln!(
        out,
        "  accounts     {:>10} {:>15}",
        state.live_accounts(),
        dead_accounts
    );
    let _ = writeln!(
        out,
        "  trustlines   {:>10} {:>15}",
        state.live_trustlines(),
        dead_trustlines
    );
    let _ = writeln!(
        out,
        "  pool shares  {:>10} {:>15}",
        state.live_pool_shares(),
        state.pool_shares.len() - state.live_pool_shares()
    );
    let _ = writeln!(
        out,
        "\n  {} records superseded by a newer one for the same key",
        state.superseded
    );
    let _ = writeln!(
        out,
        "  {} records of entry types this comparison does not model          (offers, contract data, TTL, claimable balances, …)",
        state.unmodelled
    );
    // Printed only when non-zero, and loudly: the protocol forbids a native
    // trustline, so any count here is a protocol change or a decode defect,
    // and the surrounding numbers should not be trusted until it is explained.
    if state.native_trustlines > 0 {
        let _ = writeln!(
            out,
            "  !! {} TrustLineEntry records carry the NATIVE asset — the protocol \
             forbids these; the comparison skipped them",
            state.native_trustlines
        );
    }
    let _ = writeln!(out, "  {secs:.1}s");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(balance: i64, ledger: u32) -> NetHolding {
        NetHolding {
            live: true,
            ledger,
            balance,
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
        let mut st = NetworkState::default();
        let key = HoldingKey {
            holder_id: 7,
            asset_id: 9,
        };
        st.absorb(NetFact::Trustline {
            key,
            entry: NetHolding::dead(),
        });
        st.absorb(NetFact::Trustline {
            key,
            entry: live(500, 60_000_000),
        });
        st.absorb(NetFact::Trustline {
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

    /// A real `LiquidityPoolEntry` decodes into the classic pair vocabulary
    /// the `liquidity_pools` columns store, and first-wins applies to pools
    /// exactly like every other key (task 0374 K4-6 seed).
    #[test]
    fn a_liquidity_pool_entry_classifies_into_the_pair_vocabulary() {
        use stellar_xdr::{
            AlphaNum4, Asset, AssetCode4, LedgerEntry, LedgerEntryData, LedgerEntryExt,
            LiquidityPoolConstantProductParameters, LiquidityPoolEntry, LiquidityPoolEntryBody,
            LiquidityPoolEntryConstantProduct, PoolId,
        };
        let issuer: stellar_xdr::AccountId =
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
                .parse()
                .expect("issuer strkey");
        let entry = LedgerEntry {
            last_modified_ledger_seq: 50_000_000,
            data: LedgerEntryData::LiquidityPool(LiquidityPoolEntry {
                liquidity_pool_id: PoolId(stellar_xdr::Hash([7u8; 32])),
                body: LiquidityPoolEntryBody::LiquidityPoolConstantProduct(
                    LiquidityPoolEntryConstantProduct {
                        params: LiquidityPoolConstantProductParameters {
                            asset_a: Asset::Native,
                            asset_b: Asset::CreditAlphanum4(AlphaNum4 {
                                asset_code: AssetCode4(*b"USDC"),
                                issuer: issuer.clone(),
                            }),
                            fee: 30,
                        },
                        reserve_a: 1_000,
                        reserve_b: 2_000,
                        total_pool_shares: 1_414,
                        pool_shares_trust_line_count: 3,
                    },
                ),
            }),
            ext: LedgerEntryExt::V0,
        };
        let mut st = NetworkState::default();
        st.absorb_record(&SnapshotRecord::Live(Box::new(entry)));

        let p = &st.pools[&[7u8; 32]];
        assert!(p.live);
        assert_eq!(p.ledger, 50_000_000);
        assert_eq!(p.asset_a, (0, String::new(), String::new()), "native leg");
        assert_eq!(p.asset_b.0, 1, "alphanum4 → XDR type 1");
        assert_eq!(p.asset_b.1, "USDC");
        assert_eq!(p.asset_b.2, issuer.to_string());
        assert_eq!((p.fee_bps, p.reserve_a, p.reserve_b), (30, 1_000, 2_000));
        assert_eq!((p.total_shares, p.trustline_count), (1_414, 3));
        assert_eq!(st.live_pools(), 1);
    }
}
