//! Four-way comparison of our `balances` against the checkpoint snapshot
//! (task 0463 step 3c, option A — in process, no staging table, no writes).
//!
//! ## Why four numbers and never one
//!
//! A single "match %" hides the direction of every error. The four are:
//!
//! | bucket        | meaning                                          |
//! |---------------|--------------------------------------------------|
//! | **missing**   | the network holds it, we have no row at all      |
//! | **closure**   | we hold it at zero, the network says it is gone  |
//! | **anomaly**   | we hold it at a POSITIVE amount, network says gone |
//! | **divergent** | both hold it, the amounts disagree               |
//! | **stale**     | both hold it, our `last_updated_ledger` is behind |
//!
//! **`anomaly` is the one that must never be silently folded into `closure`.**
//! A positive-but-absent row means an ingestion gap, and writing it off as a
//! closure would let the read flip render it as a live holding carrying a
//! number that is simply false — worse than the bug being fixed.
//!
//! ## Why the read is chunked by key range
//!
//! One `GROUP BY holder_id, asset_id` over the whole table dies:
//! `MEMORY_LIMIT_EXCEEDED … maximum: 3.73 GiB` at ~48.6M groups (measured
//! 2026-08-18). `holder_id` is the leading `ORDER BY` column, so slicing on it
//! turns each chunk into a primary-key range scan with a few hundred thousand
//! groups — bounded server memory, no reliance on global result ordering.
//!
//! ## What is deliberately out of scope, and reported as such
//!
//! - **Contract-held classic balances** (`holder_id` in `soroban_contracts`) —
//!   a contract holds a classic asset through the SAC's `ContractData`, not a
//!   trustline, so the snapshot's trustline set would call every one of them a
//!   phantom. Excluded and COUNTED.
//! - **Type-3 Soroban holdings** — same reason, different entry type.
//! - **Pool shares** — same ledger entry type as a trustline, but our side
//!   keeps them in `lp_positions`. Tallied from the snapshot, not compared.

use std::io::BufRead;
use std::path::Path;

use clickhouse::Row;
use serde::Deserialize;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::{
    self, HoldingKey, PUBNET_ARCHIVE, SnapshotState, fetch_bucket_list, report_state,
};

/// Our indexer's ledger floor. The single most important discriminator for the
/// `missing` bucket: an entry whose own `lastModifiedLedgerSeq` is below this
/// CANNOT have a row on our side (we never saw a change for it), while one
/// above it means either our surrogate derivation or the parser dropped a
/// change — a defect, not dormancy. The histogram over this line is what
/// separates "expected blind spot" from "bug".
const LEDGER_FLOOR: u32 = 50_457_424;

/// Cap per sample file. Stride sampling (every Nth hit) — deterministic,
/// re-runnable, no RNG (which the workflow environment forbids anyway).
const SAMPLE_CAP: usize = 2_000;

/// Deterministic every-Nth sampler.
struct Stride {
    stride: u64,
    seen: u64,
    rows: Vec<String>,
}

impl Stride {
    fn new(stride: u64) -> Self {
        Self {
            stride,
            seen: 0,
            rows: Vec::new(),
        }
    }
    fn offer(&mut self, make_line: impl FnOnce() -> String) {
        self.seen += 1;
        // (seen-1) % stride — `seen % stride == 1` never fires for stride 1,
        // which silently produced EMPTY sample files on the first run.
        if (self.seen - 1).is_multiple_of(self.stride) && self.rows.len() < SAMPLE_CAP {
            self.rows.push(make_line());
        }
    }
    fn write(&self, dir: &Path, name: &str) -> Result<(), BackfillError> {
        let path = dir.join(name);
        std::fs::write(&path, self.rows.join("\n") + "\n")
            .map_err(|e| BackfillError::Incomplete(format!("write {}: {e}", path.display())))?;
        println!("    wrote {} rows -> {}", self.rows.len(), path.display());
        Ok(())
    }
}

/// Sample collectors for every bucket of the comparison, so each verdict can
/// be spot-checked against RPC instead of trusted.
struct Samples {
    closure_classic: Stride,
    anomaly_classic: Stride,
    anomaly_native: Stride,
    divergent_classic: Stride,
    divergent_native: Stride,
    agree_classic: Stride,
    missing_classic: Stride,
    /// `missing` split by the entry's own last-modified ledger vs our floor.
    missing_below_floor: u64,
    missing_above_floor: u64,
    /// Finer histogram of the ABOVE-floor missing — these are the suspicious
    /// ones, and their shape says whether they cluster somewhere.
    missing_above_by_2m: std::collections::BTreeMap<u32, u64>,
}

impl Samples {
    fn new() -> Self {
        Self {
            // Strides sized so the caps are reached across the measured
            // populations (22M closures / 1M anomalies / 19M missing).
            closure_classic: Stride::new(11_000),
            anomaly_classic: Stride::new(1),
            anomaly_native: Stride::new(500),
            divergent_classic: Stride::new(1),
            divergent_native: Stride::new(10),
            agree_classic: Stride::new(6_500),
            missing_classic: Stride::new(9_600),
            missing_below_floor: 0,
            missing_above_floor: 0,
            missing_above_by_2m: std::collections::BTreeMap::new(),
        }
    }
}

/// Number of `holder_id` slices. 64 keeps each chunk near the ~760k groups
/// measured for 1/64 of the key space — two orders under the server's limit.
const KEY_SLICES: i128 = 64;

#[derive(Row, Deserialize)]
struct OurBalance {
    holder_id: i64,
    asset_id: i64,
    amount: i128,
    last_updated_ledger: i64,
    closed_at_ledger: i64,
}

/// Counts for one entity. Every field is a direction, never a ratio.
#[derive(Debug, Default)]
pub struct FourWay {
    /// In the snapshot as LIVE, no row on our side.
    pub missing: u64,
    /// We hold zero, the snapshot says the entry is gone. The intended closures.
    pub closure: u64,
    /// We hold a POSITIVE amount, the snapshot says the entry is gone.
    /// An ingestion gap, never a silent closure.
    pub anomaly: u64,
    /// Both sides hold it, amounts disagree.
    pub divergent: u64,
    /// Both sides hold it, our row is older than the snapshot entry.
    pub stale: u64,
    /// Both sides agree.
    pub agree: u64,
    /// Rows we already marked closed (nothing to do).
    pub already_closed: u64,
    /// Sum of the positive amounts behind `anomaly`, in stroops — the value at
    /// stake, because a count alone does not say whether a gap matters.
    pub anomaly_stroops: i128,
}

impl FourWay {
    fn print(&self, label: &str) {
        println!("\n  {label}");
        println!("    missing (network has, we do not)  {:>12}", self.missing);
        println!("    closure (we hold 0, network gone) {:>12}", self.closure);
        println!("    ANOMALY (we hold >0, network gone){:>12}", self.anomaly);
        println!(
            "    divergent amount                  {:>12}",
            self.divergent
        );
        println!("    stale (our ledger behind)         {:>12}", self.stale);
        println!("    agree                             {:>12}", self.agree);
        println!(
            "    already marked closed             {:>12}",
            self.already_closed
        );
        if self.anomaly > 0 {
            println!(
                "    value behind the anomalies        {:>12} stroops ({:.7} units)",
                self.anomaly_stroops,
                self.anomaly_stroops as f64 / 1e7
            );
        }
    }
}

/// Which bucket a row landed in — returned so the caller can sample it.
#[derive(Clone, Copy, PartialEq)]
enum Verdict {
    AlreadyClosed,
    Divergent,
    Stale,
    Agree,
    Closure,
    Anomaly,
}

/// Classify one of our rows against the snapshot entry for the same key.
fn classify_row(
    row: &OurBalance,
    snap: Option<&mut snapshot::SnapEntry>,
    out: &mut FourWay,
) -> Verdict {
    if row.closed_at_ledger != 0 {
        out.already_closed += 1;
        return Verdict::AlreadyClosed;
    }
    match snap {
        // The network still has it: compare value, then freshness.
        Some(e) if e.live => {
            e.matched = true;
            if i128::from(e.amount) != row.amount {
                out.divergent += 1;
                Verdict::Divergent
            } else if row.last_updated_ledger < i64::from(e.ledger) {
                out.stale += 1;
                Verdict::Stale
            } else {
                out.agree += 1;
                Verdict::Agree
            }
        }
        // Present in the snapshot but DEAD, or absent from it entirely. Both
        // mean the same thing for us: the network does not have this holding.
        other => {
            if let Some(e) = other {
                e.matched = true;
            }
            if row.amount == 0 {
                out.closure += 1;
                Verdict::Closure
            } else {
                out.anomaly += 1;
                out.anomaly_stroops += row.amount;
                Verdict::Anomaly
            }
        }
    }
}

/// Stream our `balances` in `holder_id` slices and fold each row into the
/// comparison. Read-only.
async fn compare_our_rows(
    sink: &Sink,
    state: &mut SnapshotState,
    native_asset_id: i64,
    samples: &mut Samples,
) -> Result<(FourWay, FourWay), BackfillError> {
    let mut classic = FourWay::default();
    let mut native = FourWay::default();
    let mut seen_rows = 0u64;

    for (i, (from, to)) in key_slices().enumerate() {
        let slice = i as i128;
        // `argMax` collapses the ReplacingMergeTree duplicates the way a read
        // must: prod tables carry unmerged parts, so a plain SELECT double-counts.
        let mut cursor = sink
            .client()
            .query(&slice_sql(from, to))
            .fetch::<OurBalance>()?;
        while let Some(row) = cursor.next().await? {
            seen_rows += 1;
            fold_row(
                &row,
                state,
                native_asset_id,
                &mut classic,
                &mut native,
                samples,
            );
        }
        println!(
            "    slice {:>2}/{KEY_SLICES} — {seen_rows} rows so far",
            slice + 1
        );
    }

    fill_missing(state, &mut classic, &mut native, samples);
    Ok((classic, native))
}

/// The per-slice read. Spelled once so the live cursor and any exported TSV
/// carry the SAME columns in the SAME order — a mismatch would silently
/// misclassify every row rather than fail.
pub fn slice_sql(from: i128, to: i128) -> String {
    // The aggregates are aliased INSIDE a subquery and renamed outside. Aliasing
    // `max(last_updated_ledger) AS last_updated_ledger` directly shadows the
    // column, so the next `argMax(..., last_updated_ledger)` binds the alias and
    // ClickHouse rejects it: "Aggregate function max(...) is found inside
    // another aggregate function" (ILLEGAL_AGGREGATION).
    format!(
        "SELECT holder_id, asset_id, amt AS amount, led AS last_updated_ledger, \
                cls AS closed_at_ledger \
         FROM ( \
             SELECT holder_id, \
                    asset_id, \
                    argMax(amount, last_updated_ledger) AS amt, \
                    max(last_updated_ledger) AS led, \
                    argMax(closed_at_ledger, last_updated_ledger) AS cls \
             FROM balances \
             WHERE holder_id BETWEEN {from} AND {to} \
               AND asset_id IN (SELECT id FROM assets WHERE asset_type IN (0, 1)) \
               AND holder_id NOT IN (SELECT id FROM soroban_contracts) \
             GROUP BY holder_id, asset_id \
         )"
    )
}

/// The slice boundaries the export must use, so a TSV produced outside this
/// binary covers the key space exactly once.
pub fn key_slices() -> impl Iterator<Item = (i128, i128)> {
    let lo = i128::from(i64::MIN);
    let hi = i128::from(i64::MAX);
    let step = (hi - lo + 1) / KEY_SLICES;
    (0..KEY_SLICES).map(move |s| {
        let from = lo + s * step;
        let to = if s == KEY_SLICES - 1 {
            hi
        } else {
            lo + (s + 1) * step - 1
        };
        (from, to)
    })
}

/// Fold rows from a TSV export instead of a live ClickHouse cursor.
///
/// The columns are exactly the SELECT list in [`slice_sql`]. This exists
/// because the operator transport for prod ClickHouse is a sanctioned
/// read-only wrapper holding the mTLS material; a comparison run must not
/// require handing those credentials to another binary. Same classification
/// either way — only the transport differs.
fn compare_from_tsv(
    path: &Path,
    state: &mut SnapshotState,
    native_asset_id: i64,
    samples: &mut Samples,
) -> Result<(FourWay, FourWay), BackfillError> {
    let file = std::fs::File::open(path)
        .map_err(|e| BackfillError::Incomplete(format!("open {}: {e}", path.display())))?;
    let mut classic = FourWay::default();
    let mut native = FourWay::default();
    let mut seen = 0u64;

    for (lineno, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line =
            line.map_err(|e| BackfillError::Incomplete(format!("read line {lineno}: {e}")))?;
        if line.is_empty() {
            continue;
        }
        let mut it = line.split('\t');
        let mut next = |what: &str| -> Result<i128, BackfillError> {
            it.next()
                .ok_or_else(|| {
                    BackfillError::Incomplete(format!("line {}: missing {what}", lineno + 1))
                })?
                .parse::<i128>()
                .map_err(|e| BackfillError::Incomplete(format!("line {} {what}: {e}", lineno + 1)))
        };
        // `as i64` would TRUNCATE a malformed export silently and misclassify
        // the row under a wrong surrogate; reject instead. `amount` stays i128
        // — the column is Int128 and large-supply tokens use the range.
        let as_i64 = |v: i128, what: &str| -> Result<i64, BackfillError> {
            i64::try_from(v).map_err(|_| {
                BackfillError::Incomplete(format!(
                    "line {}: {what} out of i64 range: {v}",
                    lineno + 1
                ))
            })
        };
        let row = OurBalance {
            holder_id: as_i64(next("holder_id")?, "holder_id")?,
            asset_id: as_i64(next("asset_id")?, "asset_id")?,
            amount: next("amount")?,
            last_updated_ledger: as_i64(next("last_updated_ledger")?, "last_updated_ledger")?,
            closed_at_ledger: as_i64(next("closed_at_ledger")?, "closed_at_ledger")?,
        };
        seen += 1;
        fold_row(
            &row,
            state,
            native_asset_id,
            &mut classic,
            &mut native,
            samples,
        );
    }

    println!("    {seen} rows read from {}", path.display());
    fill_missing(state, &mut classic, &mut native, samples);
    Ok((classic, native))
}

/// Route one row to the right entity and classify it. Shared by both
/// transports so they cannot drift apart.
fn fold_row(
    row: &OurBalance,
    state: &mut SnapshotState,
    native_asset_id: i64,
    classic: &mut FourWay,
    native: &mut FourWay,
    samples: &mut Samples,
) {
    let line = || {
        format!(
            "{}\t{}\t{}\t{}",
            row.holder_id, row.asset_id, row.amount, row.last_updated_ledger
        )
    };
    if row.asset_id == native_asset_id {
        match classify_row(row, state.accounts.get_mut(&row.holder_id), native) {
            Verdict::Anomaly => samples.anomaly_native.offer(line),
            Verdict::Divergent => samples.divergent_native.offer(line),
            _ => {}
        }
    } else {
        let key = HoldingKey {
            holder_id: row.holder_id,
            asset_id: row.asset_id,
        };
        match classify_row(row, state.trustlines.get_mut(&key), classic) {
            Verdict::Closure => samples.closure_classic.offer(line),
            Verdict::Anomaly => samples.anomaly_classic.offer(line),
            Verdict::Divergent => samples.divergent_classic.offer(line),
            Verdict::Agree => samples.agree_classic.offer(line),
            _ => {}
        }
    }
}

/// Whatever the snapshot holds LIVE and nothing on our side ever touched is the
/// blind spot: entries we never ingested, invisible to any query over our own
/// data.
fn fill_missing(
    state: &SnapshotState,
    classic: &mut FourWay,
    native: &mut FourWay,
    samples: &mut Samples,
) {
    native.missing = state
        .accounts
        .values()
        .filter(|e| e.live && !e.matched)
        .count() as u64;
    let mut missing = 0u64;
    for (key, e) in &state.trustlines {
        if !e.live || e.matched {
            continue;
        }
        missing += 1;
        // The discriminator: below the floor we cannot have a row (dormant
        // since before our history starts — expected). Above it, someone
        // dropped a change we should have seen — a defect to chase.
        if e.ledger < LEDGER_FLOOR {
            samples.missing_below_floor += 1;
        } else {
            samples.missing_above_floor += 1;
            *samples
                .missing_above_by_2m
                .entry(e.ledger / 2_000_000 * 2_000_000)
                .or_default() += 1;
        }
        samples.missing_classic.offer(|| {
            format!(
                "{}\t{}\t{}\t{}",
                key.holder_id, key.asset_id, e.amount, e.ledger
            )
        });
    }
    classic.missing = missing;
}

/// Read-only four-way comparison. Downloads and decodes the snapshot, streams
/// our rows, prints the counts. **Writes nothing, anywhere.**
pub async fn compare_command(
    sink: &Sink,
    our_rows: Option<&Path>,
    dump_dir: Option<&Path>,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    let http = reqwest::Client::new();
    let list = fetch_bucket_list(&http, PUBNET_ARCHIVE).await?;
    println!(
        "checkpoint ledger {} — {} buckets",
        list.checkpoint_ledger,
        list.hashes.len()
    );

    let take = list.hashes.len();
    let mut state =
        snapshot::build_state(&http, &list, SnapshotState::default(), |i, bytes, secs| {
            println!("  [{:>2}/{take}] {bytes:>10} B  {secs:>6.1}s", i + 1);
        })
        .await?;
    report_state(
        &state,
        list.checkpoint_ledger,
        started.elapsed().as_secs_f64(),
    );

    let native_id = db_clickhouse::persist::ids::NATIVE_ASSET_ID;
    let mut samples = Samples::new();
    let (classic, native) = match our_rows {
        Some(path) => {
            println!("\n  folding our balances from {}…", path.display());
            compare_from_tsv(path, &mut state, native_id, &mut samples)?
        }
        None => {
            println!("\n  streaming our balances in {KEY_SLICES} key slices…");
            compare_our_rows(sink, &mut state, native_id, &mut samples).await?
        }
    };

    // The missing-bucket discriminator: below our floor is the expected blind
    // spot; above it is a defect signal that must be chased, not explained.
    println!(
        "\n  missing trustlines by the entry's own lastModifiedLedgerSeq:\n    \
         below our floor {LEDGER_FLOOR}: {}\n    at/above the floor:      {}",
        samples.missing_below_floor, samples.missing_above_floor
    );
    if !samples.missing_above_by_2m.is_empty() {
        println!("    above-floor by 2M-ledger band:");
        for (band, n) in &samples.missing_above_by_2m {
            println!("      {band:>10}+  {n:>10}");
        }
    }

    if let Some(dir) = dump_dir {
        std::fs::create_dir_all(dir)
            .map_err(|e| BackfillError::Incomplete(format!("mkdir {}: {e}", dir.display())))?;
        println!("\n  sample dumps (holder_id\tasset_id\tamount\tledger):");
        samples.closure_classic.write(dir, "closure_classic.tsv")?;
        samples.anomaly_classic.write(dir, "anomaly_classic.tsv")?;
        samples.anomaly_native.write(dir, "anomaly_native.tsv")?;
        samples
            .divergent_classic
            .write(dir, "divergent_classic.tsv")?;
        samples
            .divergent_native
            .write(dir, "divergent_native.tsv")?;
        samples.agree_classic.write(dir, "agree_classic.tsv")?;
        samples.missing_classic.write(dir, "missing_classic.tsv")?;
    }

    classic.print("CLASSIC CREDIT trustlines");
    native.print("NATIVE XLM holdings (AccountEntry, not a trustline)");

    // Excluded on purpose — printed so the pass never reads as exhaustive when
    // it is not. Only queried in client mode; a TSV run reports the exclusions
    // from the export side rather than pretending to zero.
    if our_rows.is_none() {
        let excluded_contract: u64 = sink
            .client()
            .query(
                "SELECT count() FROM balances \
             WHERE asset_id IN (SELECT id FROM assets WHERE asset_type IN (0, 1)) \
               AND holder_id IN (SELECT id FROM soroban_contracts)",
            )
            .fetch_one()
            .await?;
        let excluded_type3: u64 = sink
        .client()
        .query("SELECT count() FROM balances WHERE asset_id IN (SELECT id FROM assets WHERE asset_type = 3)")
        .fetch_one()
        .await?;

        println!("\n  NOT COMPARED (deliberate, see module docs)");
        println!("    contract-held classic rows  {excluded_contract:>12}");
        println!("    type-3 Soroban rows         {excluded_type3:>12}");
    }
    println!(
        "    snapshot pool shares        {:>12}  (our side: lp_positions)",
        state.live_pool_shares()
    );
    println!(
        "\n  total {:.1}s — nothing was written",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// RPC spot-verification of the comparison's verdicts (task 0463 step 3e-early)
// ---------------------------------------------------------------------------

/// Build a `LedgerKey::Trustline` for a classic credit asset. The width
/// (alphanum4 vs 12) is the CODE length's business, exactly as the XDR defines
/// it — a 5-char code in an alphanum4 key would be a different ledger key and
/// silently verify the wrong entry.
fn trustline_ledger_key(holder: &str, code: &str, issuer: &str) -> Option<stellar_xdr::LedgerKey> {
    use stellar_xdr::{
        AccountId, AlphaNum4, AlphaNum12, AssetCode4, AssetCode12, LedgerKeyTrustLine, PublicKey,
        TrustLineAsset, Uint256,
    };
    let holder_pk = stellar_strkey::ed25519::PublicKey::from_string(holder).ok()?;
    let issuer_pk = stellar_strkey::ed25519::PublicKey::from_string(issuer).ok()?;
    let issuer_id = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(issuer_pk.0)));
    let bytes = code.as_bytes();
    let asset = if bytes.len() <= 4 {
        let mut c = [0u8; 4];
        c[..bytes.len()].copy_from_slice(bytes);
        TrustLineAsset::CreditAlphanum4(AlphaNum4 {
            asset_code: AssetCode4(c),
            issuer: issuer_id,
        })
    } else if bytes.len() <= 12 {
        let mut c = [0u8; 12];
        c[..bytes.len()].copy_from_slice(bytes);
        TrustLineAsset::CreditAlphanum12(AlphaNum12 {
            asset_code: AssetCode12(c),
            issuer: issuer_id,
        })
    } else {
        return None;
    };
    Some(stellar_xdr::LedgerKey::Trustline(LedgerKeyTrustLine {
        account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(holder_pk.0))),
        asset,
    }))
}

/// Verify sampled verdicts against Soroban RPC, the raw-XDR arbiter.
///
/// Input TSV: `kind<TAB>holder_strkey[<TAB>code<TAB>issuer]` where kind is
/// `account` or `trustline`. Emits one line per key: `FOUND balance ledger` or
/// `ABSENT`. **Absence from the response means the entry does not exist** —
/// that is the deleted-account / removed-trustline test.
pub async fn verify_command(rpc_url: &str, samples: &Path) -> Result<(), BackfillError> {
    use crate::rpc_snapshot::{RpcClient, account_ledger_key};

    let body = std::fs::read_to_string(samples)
        .map_err(|e| BackfillError::Incomplete(format!("read {}: {e}", samples.display())))?;
    let mut keys = Vec::new();
    let mut labels = Vec::new();
    for (n, line) in body.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let key = match f.as_slice() {
            ["account", holder] => account_ledger_key(holder),
            ["trustline", holder, code, issuer] => trustline_ledger_key(holder, code, issuer),
            _ => {
                return Err(BackfillError::Incomplete(format!(
                    "line {n}: expected 'account<TAB>G…' or 'trustline<TAB>G…<TAB>CODE<TAB>G…'"
                )));
            }
        };
        match key {
            Some(k) => {
                keys.push(k);
                labels.push(line.to_string());
            }
            None => println!("SKIP\t{line}\t(malformed strkey)"),
        }
    }

    let client = RpcClient::new(rpc_url)
        .map_err(|e| BackfillError::Incomplete(format!("rpc client: {e}")))?;
    let found = client
        .get_ledger_entries(&keys)
        .await
        .map_err(|e| BackfillError::Incomplete(format!("getLedgerEntries: {e}")))?;

    // The RPC answers with the entries that EXIST; absence is the signal.
    // Match responses back to requests by re-deriving each entry's key.
    let mut by_key: std::collections::HashMap<Vec<u8>, (i64, u32)> =
        std::collections::HashMap::new();
    for rec in &found {
        use stellar_xdr::{LedgerEntryData as D, Limits, WriteXdr};
        let (bal, key) = match &rec.data {
            D::Account(a) => (
                a.balance,
                stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
                    account_id: a.account_id.clone(),
                }),
            ),
            D::Trustline(t) => (
                t.balance,
                stellar_xdr::LedgerKey::Trustline(stellar_xdr::LedgerKeyTrustLine {
                    account_id: t.account_id.clone(),
                    asset: t.asset.clone(),
                }),
            ),
            _ => continue,
        };
        if let Ok(bytes) = key.to_xdr(Limits::none()) {
            by_key.insert(bytes, (bal, rec.last_modified_ledger));
        }
    }

    let mut n_found = 0u64;
    let mut n_absent = 0u64;
    for (k, label) in keys.iter().zip(&labels) {
        use stellar_xdr::{Limits, WriteXdr};
        let bytes = k
            .to_xdr(Limits::none())
            .map_err(|e| BackfillError::Incomplete(format!("key xdr: {e}")))?;
        match by_key.get(&bytes) {
            Some((bal, led)) => {
                n_found += 1;
                println!("FOUND\t{label}\tbalance={bal}\tledger={led}");
            }
            None => {
                n_absent += 1;
                println!("ABSENT\t{label}");
            }
        }
    }
    println!(
        "\nverified {} keys: {n_found} FOUND, {n_absent} ABSENT",
        keys.len()
    );
    Ok(())
}
