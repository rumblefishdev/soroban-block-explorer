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

use std::path::Path;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::{self, PUBNET_ARCHIVE, SnapshotState, fetch_bucket_list, report_state};

/// Our indexer's ledger floor. The single most important discriminator for the
/// `missing` bucket: an entry whose own `lastModifiedLedgerSeq` is below this
/// CANNOT have a row on our side (we never saw a change for it), while one
/// above it means either our surrogate derivation or the parser dropped a
/// change — a defect, not dormancy. The histogram over this line is what
/// separates "expected blind spot" from "bug".
const LEDGER_FLOOR: u32 = 50_457_424;

/// Rows per sample file. Decoder defects are SYSTEMATIC — a wrong surrogate
/// derivation or a wrong first-wins order hits every row of a class, not a
/// random one-in-a-million — so 1,000 samples per bucket detect anything a
/// larger sample would, and a per-row error bound would be theatre. (A
/// `--sample-cap` flag with population estimates and rule-of-three arithmetic
/// existed briefly and was removed as over-engineering, 2026-08-20 review.)
const SAMPLE_CAP: usize = 1_000;

/// First-N sample collector.
struct Sample {
    /// (surrogate line, verify-format key). The key is real StrKeys from the
    /// snapshot itself, so checking a sample never routes through our own
    /// tables; `None` when detail mode is off or the snapshot carries no
    /// identity for the key. Both land in ONE dump file per bucket, key first,
    /// which `snapshot-verify` consumes directly.
    rows: Vec<(String, Option<String>)>,
}

impl Sample {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }
    /// Take the row when `selected` — for populations whose iteration order is
    /// not stable, where first-N would be irreproducible across runs.
    fn offer_if(
        &mut self,
        selected: bool,
        make_line: impl FnOnce() -> String,
        make_key: impl FnOnce() -> Option<String>,
    ) {
        if selected && self.rows.len() < SAMPLE_CAP {
            self.rows.push((make_line(), make_key()));
        }
    }

    fn offer(
        &mut self,
        make_line: impl FnOnce() -> String,
        make_key: impl FnOnce() -> Option<String>,
    ) {
        self.offer_if(true, make_line, make_key);
    }
    fn write(&self, dir: &Path, name: &str) -> Result<(), BackfillError> {
        let path = dir.join(name);
        // One file per bucket: verify key first (what `snapshot-verify` parses;
        // it ignores the trailing surrogate columns), surrogates after. Rows
        // whose identity the snapshot does not carry (an asset with no live
        // trustline anywhere) are prefixed `unresolved` and counted rather than
        // silently dropped.
        let mut unresolved = 0usize;
        let mut lines = Vec::with_capacity(self.rows.len());
        for (line, key) in &self.rows {
            match key {
                Some(k) => lines.push(format!("{k}\t{line}")),
                None => {
                    unresolved += 1;
                    lines.push(format!("unresolved\t{line}"));
                }
            }
        }
        std::fs::write(&path, lines.join("\n") + "\n")
            .map_err(|e| BackfillError::Incomplete(format!("write {}: {e}", path.display())))?;
        println!(
            "    wrote {} rows -> {} ({} unresolved identities)",
            self.rows.len(),
            path.display(),
            unresolved
        );
        Ok(())
    }
}

/// Sample collectors for every bucket of the comparison, so each verdict can
/// be spot-checked against RPC instead of trusted.
struct Samples {
    closure_classic: Sample,
    ghost_classic: Sample,
    ghost_native: Sample,
    divergent_classic: Sample,
    divergent_native: Sample,
    agree_classic: Sample,
    missing_classic: Sample,
    closed_but_live: Sample,
    divergent_same_ledger: Sample,
    /// `missing` split by the entry's own last-modified ledger vs our floor.
    missing_below_floor: u64,
    missing_above_floor: u64,
    /// Finer histogram of the ABOVE-floor missing — these are the suspicious
    /// ones, and their shape says whether they cluster somewhere.
    missing_above_by_2m: std::collections::BTreeMap<u32, u64>,
    /// The checkpoint the verdicts are judged against.
    checkpoint: u32,
}

impl Samples {
    fn new(checkpoint: u32) -> Self {
        Self {
            closure_classic: Sample::new(),
            ghost_classic: Sample::new(),
            ghost_native: Sample::new(),
            divergent_classic: Sample::new(),
            divergent_native: Sample::new(),
            agree_classic: Sample::new(),
            missing_classic: Sample::new(),
            closed_but_live: Sample::new(),
            divergent_same_ledger: Sample::new(),
            missing_below_floor: 0,
            missing_above_floor: 0,
            missing_above_by_2m: std::collections::BTreeMap::new(),
            checkpoint,
        }
    }
}

/// Number of `holder_id` slices. 64 keeps each chunk near the ~760k groups
/// measured for 1/64 of the key space — two orders under the server's limit.
const KEY_SLICES: i128 = 64;

/// Stream our deduplicated `balances` in `holder_id` slices, invoking `f` per
/// row. The ONE read path both the comparison and the seed use — like every
/// other corrective command in this crate, the tool reads its own inputs
/// through `sink.client()`; there is no manual export step. (A hand-exported
/// TSV transport existed during the research phase and was removed in the
/// 2026-08-20 review: the binary holds the same connection for `--execute`
/// inserts anyway, and a cursor error propagates loudly where the operator
/// CLI's exit-0-on-server-error trap did not.)
///
/// Errors below [`snapshot::MIN_OUR_ROWS`] rows: a short read (wrong database,
/// dropped slice) would silently report our own holdings as a phantom network
/// gap.
pub(crate) async fn stream_our_rows(
    sink: &Sink,
    mut f: impl FnMut(&snapshot::OurRow),
) -> Result<u64, BackfillError> {
    let mut seen = 0u64;
    for (i, (from, to)) in key_slices().enumerate() {
        // `argMax` collapses the ReplacingMergeTree duplicates the way a read
        // must: prod tables carry unmerged parts, so a plain SELECT double-counts.
        let mut cursor = sink
            .client()
            .query(&slice_sql(from, to))
            .fetch::<snapshot::OurRow>()?;
        while let Some(row) = cursor.next().await? {
            seen += 1;
            f(&row);
        }
        println!("    slice {:>2}/{KEY_SLICES} — {seen} rows so far", i + 1);
    }
    if seen < snapshot::MIN_OUR_ROWS {
        return Err(BackfillError::Incomplete(format!(
            "our balances read returned {seen} rows, expected at least {} — a short \
             read reports our own holdings as a phantom network gap (wrong database?)",
            snapshot::MIN_OUR_ROWS
        )));
    }
    Ok(seen)
}

/// Counts for one entity. Every field is a direction, never a ratio.
#[derive(Debug, Default)]
pub struct Tally {
    /// In the snapshot as LIVE, no row on our side.
    pub missing: u64,
    /// We hold zero, the snapshot says the entry is gone. The intended closures.
    pub closure: u64,
    /// We hold a POSITIVE amount, the snapshot says the entry is gone. Called
    /// GHOST throughout — the docs, the seed and the artifact file all use that
    /// word. Never folded into `closure`: it marks an ingestion gap.
    pub ghost: u64,
    /// Both hold it, amounts disagree, snapshot strictly newer — the seed heals.
    pub heal: u64,
    /// Both hold it, amounts disagree, our row at least as new — report only.
    pub divergent_ours_newer: u64,
    /// Both sides hold it, our row is older than the snapshot entry.
    pub stale: u64,
    /// Both sides agree.
    pub agree: u64,
    /// Rows we already marked closed and the snapshot does not contradict.
    pub already_closed: u64,
    /// We say CLOSED, the network says LIVE at a newer ledger. Our closure is
    /// wrong and the holding is hidden — the costliest bucket here.
    pub closed_but_live: u64,
    /// Same ledger, different amount. Not freshness: a parsing defect on one
    /// side or the other, and worth a human look.
    pub divergent_same_ledger: u64,
    /// Our row is newer than the checkpoint, so the SNAPSHOT is the stale side.
    pub newer_than_checkpoint: u64,
    /// Sum of the positive amounts behind `ghost`, in stroops — the value at
    /// stake, because a count alone does not say whether a gap matters.
    pub ghost_stroops: i128,
}

impl Tally {
    /// Count one verdict. The seed acts on the same enum, so the report and the
    /// write cannot describe different populations.
    fn observe(&mut self, v: snapshot::Verdict, amount: i128) {
        use snapshot::Verdict as V;
        match v {
            V::AlreadyClosed => self.already_closed += 1,
            V::ClosedButLive => self.closed_but_live += 1,
            V::DivergentSameLedger => self.divergent_same_ledger += 1,
            V::Agree => self.agree += 1,
            V::HealFromSnapshot => self.heal += 1,
            V::DivergentOursNewer => self.divergent_ours_newer += 1,
            V::Stale => self.stale += 1,
            V::Closure => self.closure += 1,
            V::Ghost => {
                self.ghost += 1;
                self.ghost_stroops += amount;
            }
            V::NewerThanCheckpoint => self.newer_than_checkpoint += 1,
        }
    }

    fn print(&self, label: &str, native: bool) {
        println!("\n  {label}");
        println!(
            "    missing (network has, we do not)   {:>12}",
            self.missing
        );
        println!(
            "    closure (we hold 0, network gone)  {:>12}",
            self.closure
        );
        println!("    GHOST   (we hold >0, network gone) {:>12}", self.ghost);
        println!("    heal    (snapshot newer)           {:>12}", self.heal);
        println!(
            "    divergent (ours newer, kept)       {:>12}",
            self.divergent_ours_newer
        );
        println!("    stale   (our ledger behind)        {:>12}", self.stale);
        println!("    agree                              {:>12}", self.agree);
        println!(
            "    already marked closed              {:>12}",
            self.already_closed
        );
        println!(
            "    CLOSED BUT LIVE (we hide it)       {:>12}",
            self.closed_but_live
        );
        println!(
            "    divergent SAME ledger (defect?)    {:>12}",
            self.divergent_same_ledger
        );
        println!(
            "    newer than checkpoint (left alone) {:>12}",
            self.newer_than_checkpoint
        );
        if self.ghost > 0 {
            // Only native sums to a meaningful unit. Summing classic amounts
            // across different assets is a unit-less number — an early run
            // printed 796 billion "XLM" that way.
            if native {
                println!(
                    "    value behind the ghosts            {:>12} stroops ({:.7} XLM)",
                    self.ghost_stroops,
                    self.ghost_stroops as f64 / 1e7
                );
            } else {
                println!("    (classic ghost amounts are per-asset units — see the dump)");
            }
        }
    }
}

/// The per-slice read. Its SELECT list IS the field order of
/// [`snapshot::OurRow`] — a mismatch would silently misclassify every row
/// rather than fail.
fn slice_sql(from: i128, to: i128) -> String {
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

/// The slice boundaries, covering the i64 key space exactly once.
fn key_slices() -> impl Iterator<Item = (i128, i128)> {
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

/// Route one row to the right entity and classify it.
fn fold_row(
    row: &snapshot::OurRow,
    state: &mut SnapshotState,
    native_asset_id: i64,
    classic: &mut Tally,
    native: &mut Tally,
    samples: &mut Samples,
) {
    let line = || {
        format!(
            "{}\t{}\t{}\t{}",
            row.holder_id, row.asset_id, row.amount, row.last_updated_ledger
        )
    };
    let is_native = row.asset_id == native_asset_id;
    let out = if is_native { native } else { classic };
    let v = snapshot::verdict(
        row,
        snapshot::snap_entry_for(state, row),
        samples.checkpoint,
    );
    out.observe(v, row.amount);
    // Real identities for the verify-format companion file, straight from the
    // snapshot (populated only in detail mode). This is what breaks the
    // circularity the audit called out: reversing our surrogates through our
    // own tables meant auditing the tables with themselves.
    let key = key_line(state, row, is_native);
    // Sample the buckets a human has to adjudicate. `Agree` is sampled too: it
    // is the positive control — if the surrogate derivation were wrong, this
    // bucket would be empty, so a non-empty sample of it is evidence the whole
    // comparison is keyed correctly.
    match (v, is_native) {
        (snapshot::Verdict::Ghost, true) => samples.ghost_native.offer(line, || key),
        (snapshot::Verdict::Ghost, false) => samples.ghost_classic.offer(line, || key),
        (snapshot::Verdict::HealFromSnapshot | snapshot::Verdict::DivergentOursNewer, true) => {
            samples.divergent_native.offer(line, || key)
        }
        (snapshot::Verdict::HealFromSnapshot | snapshot::Verdict::DivergentOursNewer, false) => {
            samples.divergent_classic.offer(line, || key)
        }
        (snapshot::Verdict::ClosedButLive, _) => samples.closed_but_live.offer(line, || key),
        (snapshot::Verdict::DivergentSameLedger, _) => {
            samples.divergent_same_ledger.offer(line, || key)
        }
        (snapshot::Verdict::Closure, false) => samples.closure_classic.offer(line, || key),
        (snapshot::Verdict::Agree, false) => samples.agree_classic.offer(line, || key),
        _ => {}
    }
}

/// The `snapshot-verify` input line for one row, from snapshot-carried
/// identities. `None` when detail mode is off, or when the snapshot holds no
/// identity for the key (an asset with no live trustline anywhere — those are
/// counted as unresolved by the dump, never silently dropped).
fn key_line(state: &SnapshotState, row: &snapshot::OurRow, is_native: bool) -> Option<String> {
    let holder = &state.account_details.get(&row.holder_id)?.strkey;
    if is_native {
        Some(format!("account\t{holder}"))
    } else {
        let (code, issuer) = state.asset_registry.get(&row.asset_id)?;
        Some(format!("trustline\t{holder}\t{code}\t{issuer}"))
    }
}

/// Whatever the snapshot holds LIVE and nothing on our side ever touched is the
/// blind spot: entries we never ingested, invisible to any query over our own
/// data.
fn fill_missing(
    state: &SnapshotState,
    classic: &mut Tally,
    native: &mut Tally,
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
        // First-N is NOT deterministic here: this iterates a HashMap whose
        // hasher is seeded per process, so the sample differed on every run and
        // could not be re-verified or compared before/after. Select on a
        // function of the KEY instead — reproducible, and independent of any
        // structure in the data. The modulus is sized so ~19M missing rows
        // yield on the order of the cap.
        const MISSING_SAMPLE_MODULUS: u64 = 16_384;
        let row_for_key = snapshot::OurRow {
            holder_id: key.holder_id,
            asset_id: key.asset_id,
            amount: 0,
            last_updated_ledger: 0,
            closed_at_ledger: 0,
        };
        samples.missing_classic.offer_if(
            (key.holder_id ^ key.asset_id)
                .unsigned_abs()
                .is_multiple_of(MISSING_SAMPLE_MODULUS),
            || {
                format!(
                    "{}\t{}\t{}\t{}",
                    key.holder_id, key.asset_id, e.amount, e.ledger
                )
            },
            || key_line(state, &row_for_key, false),
        );
    }
    classic.missing = missing;
}

/// Read-only four-way comparison. Downloads and decodes the snapshot, streams
/// our rows, prints the counts. **Writes nothing, anywhere.**
pub async fn compare_command(
    sink: &Sink,
    dump_dir: Option<&Path>,
    pinned_manifest: Option<&Path>,
) -> Result<(), BackfillError> {
    let started = std::time::Instant::now();
    let http = snapshot::archive_client()?;
    // Pin the same snapshot the seed will run, or the report describes a
    // different checkpoint than the write — and the shared verdict rule then
    // guarantees only that the two would agree on inputs they never share.
    let list = match pinned_manifest {
        Some(path) => snapshot::bucket_list_from_manifest(path)?,
        None => fetch_bucket_list(&http, PUBNET_ARCHIVE).await?,
    };
    println!(
        "checkpoint ledger {} — {} buckets",
        list.checkpoint_ledger,
        list.hashes.len()
    );

    let take = list.hashes.len();
    let mut state = snapshot::build_state(
        &http,
        &list,
        // Details (StrKeys, asset identities) cost ~2 GB extra and exist so the
        // sample dumps can carry REAL keys next to the surrogates. Without them
        // verifying a "missing" entry meant reversing a one-way hash through
        // our own incomplete tables — the very tables under audit. Only paid
        // when dumps were asked for.
        if dump_dir.is_some() {
            SnapshotState::with_details()
        } else {
            SnapshotState::default()
        },
        |i, bytes, secs| {
            println!("  [{:>2}/{take}] {bytes:>10} B  {secs:>6.1}s", i + 1);
        },
    )
    .await?;
    report_state(
        &state,
        list.checkpoint_ledger,
        started.elapsed().as_secs_f64(),
    );

    let native_id = db_clickhouse::persist::ids::NATIVE_ASSET_ID;
    let mut samples = Samples::new(list.checkpoint_ledger);
    let mut classic = Tally::default();
    let mut native = Tally::default();
    println!("\n  streaming our balances in {KEY_SLICES} key slices…");
    stream_our_rows(sink, |row| {
        fold_row(
            row,
            &mut state,
            native_id,
            &mut classic,
            &mut native,
            &mut samples,
        );
    })
    .await?;
    fill_missing(&state, &mut classic, &mut native, &mut samples);

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
        samples.ghost_classic.write(dir, "ghosts_classic.tsv")?;
        samples.ghost_native.write(dir, "ghosts_native.tsv")?;
        samples
            .divergent_classic
            .write(dir, "divergent_classic.tsv")?;
        samples
            .divergent_native
            .write(dir, "divergent_native.tsv")?;
        samples.agree_classic.write(dir, "agree_classic.tsv")?;
        samples.missing_classic.write(dir, "missing_classic.tsv")?;
        samples.closed_but_live.write(dir, "closed_but_live.tsv")?;
        samples
            .divergent_same_ledger
            .write(dir, "divergent_same_ledger.tsv")?;
    }

    classic.print("CLASSIC CREDIT trustlines", false);
    native.print("NATIVE XLM holdings (AccountEntry, not a trustline)", true);

    // Excluded on purpose — printed so the pass never reads as exhaustive when
    // it is not.
    {
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

/// Verify sampled verdicts against Soroban RPC — an independent ~100-line
/// oracle over the snapshot decoder, for the one pre-`--execute` check.
///
/// Input: the dump files `snapshot-compare --dump-dir` writes, as-is. Each
/// line starts `account<TAB>G…` or `trustline<TAB>G…<TAB>CODE<TAB>G-issuer`;
/// trailing surrogate columns are ignored, and `unresolved` lines (identity
/// not carried by the snapshot) are counted and skipped. Emits one line per
/// key: `FOUND balance ledger` or `ABSENT`. **Absence from the response means
/// the entry does not exist** — the deleted-account / removed-trustline test.
pub async fn verify_command(
    rpc_url: &str,
    samples: &Path,
    checkpoint: Option<u32>,
) -> Result<(), BackfillError> {
    use crate::rpc_snapshot::{RpcClient, account_ledger_key};

    let body = std::fs::read_to_string(samples)
        .map_err(|e| BackfillError::Incomplete(format!("read {}: {e}", samples.display())))?;
    let mut keys = Vec::new();
    let mut labels = Vec::new();
    let mut n_unresolved = 0u64;
    for (n, line) in body.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let key = match f.as_slice() {
            ["account", holder, ..] => account_ledger_key(holder),
            ["trustline", holder, code, issuer, ..] => trustline_ledger_key(holder, code, issuer),
            ["unresolved", ..] => {
                n_unresolved += 1;
                continue;
            }
            _ => {
                return Err(BackfillError::Incomplete(format!(
                    "line {n}: expected 'account<TAB>G…' or 'trustline<TAB>G…<TAB>CODE<TAB>G…' \
                     (a snapshot-compare dump file)"
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
    if n_unresolved > 0 {
        println!("  {n_unresolved} unresolved-identity lines skipped");
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
    // The point-in-time argument, which the earlier check discarded even though
    // the data was already in hand:
    //
    //   an entry present NOW whose `lastModifiedLedgerSeq` is <= the checkpoint
    //   existed CONTINUOUSLY and unchanged over that span — a delete-and-recreate
    //   would have bumped the ledger.
    //
    // So FOUND with `L <= checkpoint` is proof about the checkpoint, not merely
    // about now. FOUND with `L > checkpoint` proves nothing either way.
    let mut n_proven = 0u64;
    let mut n_changed_since = 0u64;
    for (k, label) in keys.iter().zip(&labels) {
        use stellar_xdr::{Limits, WriteXdr};
        let bytes = k
            .to_xdr(Limits::none())
            .map_err(|e| BackfillError::Incomplete(format!("key xdr: {e}")))?;
        match by_key.get(&bytes) {
            Some((bal, led)) => {
                n_found += 1;
                match checkpoint {
                    Some(cp) if *led <= cp => {
                        n_proven += 1;
                        println!(
                            "FOUND\tPROVEN-AT-CHECKPOINT\t{label}\tbalance={bal}\tledger={led}"
                        );
                    }
                    Some(_) => {
                        n_changed_since += 1;
                        println!("FOUND\tCHANGED-SINCE\t{label}\tbalance={bal}\tledger={led}");
                    }
                    None => println!("FOUND\t{label}\tbalance={bal}\tledger={led}"),
                }
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
    if let Some(cp) = checkpoint {
        println!(
            "  against checkpoint {cp}: {n_proven} proven unchanged since, \
             {n_changed_since} changed after it (those prove nothing either way)"
        );
    } else {
        println!(
            "  NOTE: no --checkpoint given, so every result is about state NOW, not about \
             the checkpoint the verdicts were judged at. 'ABSENT now' is equally consistent \
             with a correct verdict and with an entry closed after the checkpoint."
        );
    }
    Ok(())
}
