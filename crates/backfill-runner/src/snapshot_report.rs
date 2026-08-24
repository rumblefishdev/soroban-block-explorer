//! Reporting and sampling for the snapshot comparison — the counters, the
//! per-bucket sample dumps and the ledger-floor histogram.
//!
//! One home for the analysis, separate from the command that renders it: the
//! numbers are computed from the shared
//! [`snapshot_verdict::verdict`] rule, and any consumer that folds our rows through
//! that rule can produce the identical report.

use std::path::Path;

use std::fmt::Write as _;

use crate::error::BackfillError;
use crate::network_state::{self, NetworkState};
use crate::snapshot_verdict;

/// Our indexer's ledger floor. The single most important discriminator for the
/// `missing` bucket: an entry whose own `lastModifiedLedgerSeq` is below this
/// CANNOT have a row on our side (we never saw a change for it), while one
/// above it means either our surrogate derivation or the parser dropped a
/// change — a defect, not dormancy. The histogram over this line is what
/// separates "expected blind spot" from "bug".
pub(crate) const LEDGER_FLOOR: u32 = 50_457_424;

/// Rows per sample file. Decoder defects are SYSTEMATIC — a wrong surrogate
/// derivation or a wrong first-wins order hits every row of a class, not a
/// random one-in-a-million — so 1,000 samples per bucket detect anything a
/// larger sample would, and a per-row error bound would be theatre. (A
/// `--sample-cap` flag with population estimates and rule-of-three arithmetic
/// existed briefly and was removed as over-engineering, 2026-08-20 review.)
const SAMPLE_CAP: usize = 1_000;

/// First-N sample collector.
pub(crate) struct Sample {
    /// (surrogate line, real-identity key). The key is StrKeys from the
    /// snapshot itself, so reading a sample never routes through our own
    /// tables; `None` when the snapshot carries no identity for the key.
    /// Both land in ONE dump file per bucket, key first.
    rows: Vec<(String, Option<String>)>,
}

impl Sample {
    pub(crate) fn new() -> Self {
        Self { rows: Vec::new() }
    }
    /// Take the row when `selected` — for populations whose iteration order is
    /// not stable, where first-N would be irreproducible across runs.
    pub(crate) fn offer_if(
        &mut self,
        selected: bool,
        make_line: impl FnOnce() -> String,
        make_key: impl FnOnce() -> Option<String>,
    ) {
        if selected && self.rows.len() < SAMPLE_CAP {
            self.rows.push((make_line(), make_key()));
        }
    }

    pub(crate) fn offer(
        &mut self,
        make_line: impl FnOnce() -> String,
        make_key: impl FnOnce() -> Option<String>,
    ) {
        self.offer_if(true, make_line, make_key);
    }
    pub(crate) fn write(&self, dir: &Path, name: &str) -> Result<(), BackfillError> {
        let path = dir.join(name);
        // One file per bucket: real identity first, surrogates after. Rows
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
/// be eyeballed against real chain identities instead of trusted.
pub(crate) struct Samples {
    pub(crate) closure_classic: Sample,
    pub(crate) ghost_classic: Sample,
    pub(crate) ghost_native: Sample,
    pub(crate) divergent_classic: Sample,
    pub(crate) divergent_native: Sample,
    pub(crate) agree_classic: Sample,
    pub(crate) missing_classic: Sample,
    pub(crate) closed_but_live: Sample,
    pub(crate) divergent_same_ledger: Sample,
    /// `missing` split by the entry's own last-modified ledger vs our floor.
    pub(crate) missing_below_floor: u64,
    pub(crate) missing_above_floor: u64,
    /// Finer histogram of the ABOVE-floor missing — these are the suspicious
    /// ones, and their shape says whether they cluster somewhere.
    pub(crate) missing_above_by_2m: std::collections::BTreeMap<u32, u64>,
    /// The checkpoint the verdicts are judged against.
    pub(crate) checkpoint: u32,
}

impl Samples {
    pub(crate) fn new(checkpoint: u32) -> Self {
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

/// Counts for one entity. Every field is a direction, never a ratio.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    /// In the snapshot as LIVE, no row on our side.
    pub(crate) missing: u64,
    /// We hold zero, the snapshot says the entry is gone. The intended closures.
    pub(crate) closure: u64,
    /// We hold a POSITIVE amount, the snapshot says the entry is gone. Called
    /// GHOST throughout — the docs, the seed and the artifact file all use that
    /// word. Never folded into `closure`: it marks an ingestion gap.
    pub(crate) ghost: u64,
    /// Both hold it, amounts disagree, snapshot strictly newer — the seed heals.
    pub(crate) heal: u64,
    /// Both hold it, amounts disagree, our row at least as new — report only.
    pub(crate) divergent_ours_newer: u64,
    /// Both sides hold it, our row is older than the snapshot entry.
    pub(crate) stale: u64,
    /// Both sides agree.
    pub(crate) agree: u64,
    /// Rows we already marked closed and the snapshot does not contradict.
    pub(crate) already_closed: u64,
    /// We say CLOSED, the network says LIVE at a newer ledger. Our closure is
    /// wrong and the holding is hidden — the costliest bucket here.
    pub(crate) closed_but_live: u64,
    /// Same ledger, different amount. Not freshness: a parsing defect on one
    /// side or the other, and worth a human look.
    pub(crate) divergent_same_ledger: u64,
    /// Our row is newer than the checkpoint, so the SNAPSHOT is the stale side.
    pub(crate) newer_than_checkpoint: u64,
    /// Sum of the positive amounts behind `ghost`, in stroops — the value at
    /// stake, because a count alone does not say whether a gap matters.
    pub(crate) ghost_stroops: i128,
}

impl Tally {
    /// Count one verdict. The seed acts on the same enum, so the report and the
    /// write cannot describe different populations.
    pub(crate) fn observe(&mut self, v: snapshot_verdict::Verdict, amount: i128) {
        use snapshot_verdict::Verdict as V;
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

    /// The eleven buckets as text. The TEN verdict buckets sum to the rows
    /// read — a completeness invariant, so one of our rows cannot vanish from
    /// the report unnoticed. `missing` is the eleventh and does not belong to
    /// that sum: it is counted from the snapshot side, over entries we hold no
    /// row for at all.
    pub(crate) fn render(&self, label: &str, native: bool) -> String {
        let mut out = format!("\n  {label}\n");
        let rows = [
            ("missing (network has, we do not)  ", self.missing),
            ("closure (we hold 0, network gone) ", self.closure),
            ("GHOST   (we hold >0, network gone)", self.ghost),
            ("heal    (snapshot newer)          ", self.heal),
            (
                "divergent (ours newer, kept)      ",
                self.divergent_ours_newer,
            ),
            ("stale   (our ledger behind)       ", self.stale),
            ("agree                             ", self.agree),
            ("already marked closed             ", self.already_closed),
            ("CLOSED BUT LIVE (re-opened)       ", self.closed_but_live),
            (
                "divergent SAME ledger (defect?)   ",
                self.divergent_same_ledger,
            ),
            (
                "newer than checkpoint (left alone)",
                self.newer_than_checkpoint,
            ),
        ];
        for (label, n) in rows {
            let _ = writeln!(out, "    {label} {n:>12}");
        }
        if self.ghost > 0 {
            // Only native sums to a meaningful unit. Summing classic amounts
            // across different assets is a unit-less number — an early run
            // printed 796 billion "XLM" that way.
            if native {
                let _ = writeln!(
                    out,
                    "    value behind the ghosts            {:>12} stroops ({:.7} XLM)",
                    self.ghost_stroops,
                    self.ghost_stroops as f64 / 1e7
                );
            } else {
                out.push_str("    (classic ghost amounts are per-asset units — see ghosts.tsv)\n");
            }
        }
        out
    }
}

/// The real-identity line for one row, from snapshot-carried
/// identities. `None` when the snapshot holds no identity for the key
/// (an asset with no live trustline anywhere — those are
/// counted as unresolved by the dump, never silently dropped).
pub(crate) fn key_line(
    state: &NetworkState,
    row: &snapshot_verdict::OurRow,
    is_native: bool,
) -> Option<String> {
    let holder = &state.account_details.get(&row.holder_id)?.strkey;
    if is_native {
        Some(format!("account\t{holder}"))
    } else {
        let (code, issuer) = state.asset_registry.get(&row.asset_id)?;
        Some(format!("trustline\t{holder}\t{code}\t{issuer}"))
    }
}

/// The whole analysis of one snapshot pass: per-population counters, the
/// sample dumps and the ledger-floor histogram.
///
/// Owned by whoever renders it. [`Report::observe`] RETURNS the verdict it
/// counted, so a consumer that also acts on the verdict (the seed, building
/// corrections) computes it exactly once — the report and the write can never
/// describe different populations.
pub(crate) struct Report {
    pub(crate) classic: Tally,
    pub(crate) native: Tally,
    samples: Samples,
    native_asset_id: i64,
}

impl Report {
    pub(crate) fn new(checkpoint: u32) -> Self {
        Self {
            classic: Tally::default(),
            native: Tally::default(),
            samples: Samples::new(checkpoint),
            native_asset_id: db_clickhouse::persist::ids::NATIVE_ASSET_ID,
        }
    }

    /// Classify one of our rows, count it, sample it — and hand the verdict
    /// back to the caller.
    pub(crate) fn observe(
        &mut self,
        row: &snapshot_verdict::OurRow,
        state: &mut NetworkState,
    ) -> snapshot_verdict::Verdict {
        let line = || {
            format!(
                "{}\t{}\t{}\t{}",
                row.holder_id, row.asset_id, row.amount, row.last_updated_ledger
            )
        };
        let is_native = row.asset_id == self.native_asset_id;
        let v = snapshot_verdict::verdict(
            row,
            snapshot_verdict::holding_for(state, row),
            self.samples.checkpoint,
        );
        let out = if is_native {
            &mut self.native
        } else {
            &mut self.classic
        };
        out.observe(v, row.amount);
        // Sample the buckets a human has to adjudicate. `Agree` is sampled too:
        // it is the positive control — if the surrogate derivation were wrong
        // that bucket would be empty, so a non-empty sample of it is evidence
        // the whole comparison is keyed correctly.
        use snapshot_verdict::Verdict as V;
        let s = &mut self.samples;
        let bucket = match (v, is_native) {
            (V::Ghost, true) => Some(&mut s.ghost_native),
            (V::Ghost, false) => Some(&mut s.ghost_classic),
            (V::HealFromSnapshot | V::DivergentOursNewer, true) => Some(&mut s.divergent_native),
            (V::HealFromSnapshot | V::DivergentOursNewer, false) => Some(&mut s.divergent_classic),
            (V::ClosedButLive, _) => Some(&mut s.closed_but_live),
            (V::DivergentSameLedger, _) => Some(&mut s.divergent_same_ledger),
            (V::Closure, false) => Some(&mut s.closure_classic),
            (V::Agree, false) => Some(&mut s.agree_classic),
            _ => None,
        };
        // The identity is built ONLY for a row that is actually kept — at most
        // SAMPLE_CAP per bucket, against tens of millions of rows read. It comes
        // straight from the snapshot, which is what breaks the circularity the
        // audit called out: reversing our surrogates through our own tables
        // meant auditing the tables with themselves.
        if let Some(bucket) = bucket {
            bucket.offer(line, || key_line(state, row, is_native));
        }
        v
    }

    /// One live snapshot trustline nothing on our side ever touched — the blind
    /// spot: an entry we never ingested, invisible to any query over our own
    /// data. Counted, bucketed by the ledger floor, and sampled.
    pub(crate) fn observe_missing_trustline(
        &mut self,
        key: &network_state::HoldingKey,
        entry: &network_state::NetHolding,
        state: &NetworkState,
    ) {
        self.classic.missing += 1;
        // The discriminator: below the floor we cannot have a row (dormant
        // since before our history starts — expected). Above it, someone
        // dropped a change we should have seen — a defect to chase.
        if entry.ledger < LEDGER_FLOOR {
            self.samples.missing_below_floor += 1;
        } else {
            self.samples.missing_above_floor += 1;
            *self
                .samples
                .missing_above_by_2m
                .entry(entry.ledger / 2_000_000 * 2_000_000)
                .or_default() += 1;
        }
        // First-N is NOT deterministic here: the caller iterates a HashMap whose
        // hasher is seeded per process, so the sample differed on every run and
        // could not be re-verified or compared before/after. Select on a
        // function of the KEY instead — reproducible, and independent of any
        // structure in the data.
        const MISSING_SAMPLE_MODULUS: u64 = 16_384;
        let row_for_key = snapshot_verdict::OurRow {
            holder_id: key.holder_id,
            asset_id: key.asset_id,
            amount: 0,
            last_updated_ledger: 0,
            closed_at_ledger: 0,
        };
        self.samples.missing_classic.offer_if(
            (key.holder_id ^ key.asset_id)
                .unsigned_abs()
                .is_multiple_of(MISSING_SAMPLE_MODULUS),
            || {
                format!(
                    "{}\t{}\t{}\t{}",
                    key.holder_id, key.asset_id, entry.balance, entry.ledger
                )
            },
            || key_line(state, &row_for_key, false),
        );
    }

    /// One live snapshot ACCOUNT nothing on our side ever touched — native's
    /// half of the blind spot.
    pub(crate) fn observe_missing_account(&mut self) {
        self.native.missing += 1;
    }

    /// The missing-bucket discriminator, rendered: below our floor is the
    /// expected blind spot, above it is a defect signal to chase.
    pub(crate) fn render_missing_histogram(&self) -> String {
        use std::fmt::Write as _;
        let s = &self.samples;
        let mut out = format!(
            "\n  missing trustlines by the entry's own lastModifiedLedgerSeq:\n    \
             below our floor {LEDGER_FLOOR}: {}\n    at/above the floor:      {}\n",
            s.missing_below_floor, s.missing_above_floor
        );
        if !s.missing_above_by_2m.is_empty() {
            out.push_str("    above-floor by 2M-ledger band:\n");
            for (band, n) in &s.missing_above_by_2m {
                let _ = writeln!(out, "      {band:>10}+  {n:>10}");
            }
        }
        out
    }

    /// Write every per-bucket sample dump into `dir`.
    pub(crate) fn write_dumps(&self, dir: &Path) -> Result<(), BackfillError> {
        std::fs::create_dir_all(dir)
            .map_err(|e| BackfillError::Incomplete(format!("mkdir {}: {e}", dir.display())))?;
        println!("\n  sample dumps (real identity, then holder_id\tasset_id\tamount\tledger):");
        let s = &self.samples;
        s.closure_classic.write(dir, "closure_classic.tsv")?;
        s.ghost_classic.write(dir, "ghosts_classic.tsv")?;
        s.ghost_native.write(dir, "ghosts_native.tsv")?;
        s.divergent_classic.write(dir, "divergent_classic.tsv")?;
        s.divergent_native.write(dir, "divergent_native.tsv")?;
        s.agree_classic.write(dir, "agree_classic.tsv")?;
        s.missing_classic.write(dir, "missing_classic.tsv")?;
        s.closed_but_live.write(dir, "closed_but_live.tsv")?;
        s.divergent_same_ledger
            .write(dir, "divergent_same_ledger.tsv")?;
        Ok(())
    }
}
