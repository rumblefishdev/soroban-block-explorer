//! Reporting and sampling for the snapshot comparison — the counters, the
//! per-bucket sample dumps and the ledger-floor histogram.
//!
//! Split out of `snapshot_compare` so the analysis has one home regardless of
//! which command renders it: the numbers are computed from the shared
//! [`snapshot::verdict`] rule, and any consumer that folds our rows through
//! that rule can produce the identical report.

use std::path::Path;

use crate::error::BackfillError;
use crate::snapshot::{self, SnapshotState};

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
    /// tables; `None` when detail mode is off or the snapshot carries no
    /// identity for the key. Both land in ONE dump file per bucket, key first.
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
    pub(crate) fn observe(&mut self, v: snapshot::Verdict, amount: i128) {
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

    pub(crate) fn print(&self, label: &str, native: bool) {
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

/// The real-identity line for one row, from snapshot-carried
/// identities. `None` when detail mode is off, or when the snapshot holds no
/// identity for the key (an asset with no live trustline anywhere — those are
/// counted as unresolved by the dump, never silently dropped).
pub(crate) fn key_line(
    state: &SnapshotState,
    row: &snapshot::OurRow,
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
