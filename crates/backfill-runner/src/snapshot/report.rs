//! Reporting and sampling for the snapshot comparison — the counters, the
//! per-bucket sample dumps and the ledger-floor histogram.
//!
//! One home for the analysis, separate from the command that renders it: the
//! numbers are computed from the shared
//! [`verdict::verdict`] rule, and any consumer that folds our rows through
//! that rule can produce the identical report.

use std::path::Path;

use std::fmt::Write as _;

use crate::error::BackfillError;
use crate::snapshot::network_state::{self, NetworkState};
use crate::snapshot::verdict;

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

/// Bounded sample collector: keeps the [`SAMPLE_CAP`] rows whose KEY hashes
/// smallest, out of however many are offered.
///
/// The property that matters is not the size — it is that the kept set is a
/// function of WHICH KEYS exist, and of nothing else. Two consequences the
/// obvious alternative does not have:
///
/// - **Spread.** Our rows arrive in ascending key-slice order, so first-N
///   would fill every bucket from the first ~1.6% of the key space, and a
///   defect confined to some other part of it would be counted but never
///   appear in any dump. Hashes are uniform, so these rows are drawn from the
///   whole range.
/// - **Reproducibility.** The result does not depend on arrival order, which
///   matters because the missing-entry buckets are fed from a `HashMap` whose
///   iteration order is seeded per process. The same population always yields
///   the same sample, so a dump taken before the seed can be compared with one
///   taken after.
///
/// A bucket smaller than the cap keeps EVERYTHING — the defect buckets
/// (`divergent SAME ledger`, `closed but live`) are expected to hold a handful
/// of rows, and those are the rows an operator reaches for first.
pub(crate) struct Sample {
    /// Max-heap on the key hash: the largest hash sits on top and is the next
    /// to be evicted. Holds `(hash, surrogate line, real-identity key)`; the
    /// key is StrKeys from the snapshot itself, so reading a sample never
    /// routes through our own tables. `None` when the snapshot carries no
    /// identity for the key. Both land in ONE dump file per bucket, key first.
    heap: std::collections::BinaryHeap<(u64, String, Option<String>)>,
}

/// Spread a key over `u64`.
///
/// The XOR alone would very nearly do — `holder_id` and `asset_id` are already
/// CityHash outputs, so it is uniform for the keys this actually sees. It is
/// finalized anyway, because without it this function is the IDENTITY on the
/// key, and taking the smallest identities means taking a contiguous run of
/// the key space. That is invisible while the input happens to be
/// hash-distributed and silent the moment it is not — a future bucket keyed by
/// a ledger, a pool id, or anything else with structure. The mix is
/// SplitMix64's finalizer: three shifts and two multiplies, once per kept row.
fn sample_hash(holder_id: i64, asset_id: i64) -> u64 {
    let mut x = (holder_id ^ asset_id) as u64;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

impl Sample {
    pub(crate) fn new() -> Self {
        Self {
            heap: std::collections::BinaryHeap::new(),
        }
    }

    /// Offer one row. Kept if the bucket is not full, or if this key hashes
    /// below the largest hash currently held — in which case that one goes.
    ///
    /// `make_line` / `make_key` stay lazy: they run only for a row that is
    /// actually taken. A row can still be built and later evicted, but that
    /// happens on the order of `cap * ln(population)` times — thousands, not
    /// the tens of millions a non-lazy version would cost.
    pub(crate) fn offer(
        &mut self,
        holder_id: i64,
        asset_id: i64,
        make_line: impl FnOnce() -> String,
        make_key: impl FnOnce() -> Option<String>,
    ) {
        let h = sample_hash(holder_id, asset_id);
        if self.heap.len() < SAMPLE_CAP {
            self.heap.push((h, make_line(), make_key()));
        } else if let Some((worst, ..)) = self.heap.peek()
            && h < *worst
        {
            self.heap.pop();
            self.heap.push((h, make_line(), make_key()));
        }
    }

    /// The kept rows, ordered by key hash so a dump is byte-identical across
    /// runs (the heap's own order is an implementation detail).
    fn sorted(&self) -> Vec<&(u64, String, Option<String>)> {
        let mut out: Vec<_> = self.heap.iter().collect();
        out.sort_by_key(|(h, ..)| *h);
        out
    }

    pub(crate) fn write(&self, dir: &Path, name: &str) -> Result<(), BackfillError> {
        let path = dir.join(name);
        // One file per bucket: real identity first, surrogates after. Rows
        // whose identity the snapshot does not carry (an asset with no live
        // trustline anywhere) are prefixed `unresolved` and counted rather than
        // silently dropped.
        let mut unresolved = 0usize;
        let kept = self.sorted();
        let mut lines = Vec::with_capacity(kept.len());
        for (_, line, key) in kept {
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
            self.heap.len(),
            path.display(),
            unresolved
        );
        Ok(())
    }
}

#[cfg(test)]
mod sample_tests {
    use super::*;

    /// Keys spanning the WHOLE i64 range, offered in ascending order — the
    /// order our key-slice read actually produces, and the one first-N used to
    /// bias on.
    fn key_at(i: u64, n: u64) -> i64 {
        (i.wrapping_mul(u64::MAX / n.max(1))) as i64
    }

    fn fill(n: u64) -> Sample {
        let mut s = Sample::new();
        for i in 0..n {
            let holder = key_at(i, n);
            s.offer(holder, 0, || format!("{holder}"), || None);
        }
        s
    }

    /// A bucket smaller than the cap keeps EVERYTHING. The defect buckets hold
    /// a handful of rows and those are the rows an operator reaches for first;
    /// any selection that thins them is worse than useless.
    #[test]
    fn a_bucket_below_the_cap_keeps_every_row() {
        assert_eq!(fill(17).heap.len(), 17);
    }

    /// A large bucket is capped, and — the point of the whole exercise — the
    /// kept rows are drawn from the WHOLE key range, not from the front of the
    /// arrival order. First-N on this input would keep only the lowest keys.
    #[test]
    fn a_large_bucket_is_capped_and_spread_over_the_key_range() {
        let s = fill(100_000);
        assert_eq!(s.heap.len(), SAMPLE_CAP);

        let kept: Vec<i64> = s
            .sorted()
            .iter()
            .map(|(_, line, _)| line.parse().expect("line is the holder id"))
            .collect();
        let below_zero = kept.iter().filter(|h| **h < 0).count();
        assert!(
            below_zero > SAMPLE_CAP / 4 && below_zero < SAMPLE_CAP * 3 / 4,
            "both halves of the key range must be represented, got {below_zero} of {SAMPLE_CAP} below zero"
        );
    }

    /// Order-independence is what lets a dump taken before the seed be compared
    /// with one taken after. The missing-entry buckets are fed from a `HashMap`
    /// whose iteration order is seeded per process, so this is not academic.
    #[test]
    fn the_kept_set_does_not_depend_on_arrival_order() {
        let forward = fill(5_000);
        let mut backward = Sample::new();
        for i in (0..5_000u64).rev() {
            let holder = key_at(i, 5_000);
            backward.offer(holder, 0, || format!("{holder}"), || None);
        }
        let a: Vec<_> = forward.sorted().iter().map(|(h, ..)| *h).collect();
        let b: Vec<_> = backward.sorted().iter().map(|(h, ..)| *h).collect();
        assert_eq!(a, b, "the same population must yield the same sample");
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
    /// The OTHER defect signal. It writes nothing, so this dump is the only
    /// way to look at one — the same reason `divergent_same_ledger` has one.
    pub(crate) closed_but_live_conflict: Sample,
    pub(crate) divergent_same_ledger: Sample,
    /// `missing` split by the entry's own last-modified ledger vs our floor.
    pub(crate) missing_below_floor: u64,
    pub(crate) missing_above_floor: u64,
    /// Finer histogram of the ABOVE-floor missing — these are the suspicious
    /// ones, and their shape says whether they cluster somewhere.
    pub(crate) missing_above_by_2m: std::collections::BTreeMap<u32, u64>,
}

impl Samples {
    pub(crate) fn new() -> Self {
        Self {
            closure_classic: Sample::new(),
            ghost_classic: Sample::new(),
            ghost_native: Sample::new(),
            divergent_classic: Sample::new(),
            divergent_native: Sample::new(),
            agree_classic: Sample::new(),
            missing_classic: Sample::new(),
            closed_but_live: Sample::new(),
            closed_but_live_conflict: Sample::new(),
            divergent_same_ledger: Sample::new(),
            missing_below_floor: 0,
            missing_above_floor: 0,
            missing_above_by_2m: std::collections::BTreeMap::new(),
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
    /// We say CLOSED, the network says LIVE — but not at a newer ledger, so no
    /// honest version can supersede our closure. Reported, never auto-healed.
    pub(crate) closed_but_live_conflict: u64,
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
    pub(crate) fn observe(&mut self, v: verdict::Verdict, amount: i128) {
        use verdict::Verdict as V;
        match v {
            V::AlreadyClosed => self.already_closed += 1,
            V::ClosedButLive => self.closed_but_live += 1,
            V::ClosedButLiveConflict => self.closed_but_live_conflict += 1,
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

    /// The twelve buckets as text. The ELEVEN verdict buckets sum to the rows
    /// read — a completeness invariant, so one of our rows cannot vanish from
    /// the report unnoticed. `missing` is the twelfth and does NOT belong to
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
                "CLOSED vs LIVE conflict (defect?) ",
                self.closed_but_live_conflict,
            ),
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
    holder_id: i64,
    asset_id: i64,
    is_native: bool,
) -> Option<String> {
    let holder = &state.account_details.get(&holder_id)?.strkey;
    if is_native {
        Some(format!("account\t{holder}"))
    } else {
        let (code, issuer) = state.asset_registry.get(&asset_id)?;
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
    /// The checkpoint every verdict is judged against. It has nothing to do
    /// with sampling, which is where it used to live.
    checkpoint: u32,
    samples: Samples,
}

impl Report {
    pub(crate) fn new(checkpoint: u32) -> Self {
        Self {
            classic: Tally::default(),
            native: Tally::default(),
            samples: Samples::new(),
            checkpoint,
        }
    }

    /// Classify one of our rows, count it, sample it — and hand the verdict
    /// back with the network holding it was judged against, so the caller
    /// builds its correction from the same two facts and looks nothing up
    /// twice.
    pub(crate) fn observe(
        &mut self,
        row: &verdict::OurRow,
        state: &mut NetworkState,
    ) -> (verdict::Verdict, Option<network_state::NetHolding>) {
        let line = || {
            format!(
                "{}\t{}\t{}\t{}",
                row.holder_id, row.asset_id, row.amount, row.last_updated_ledger
            )
        };
        let is_native = row.asset_id == db_clickhouse::persist::ids::NATIVE_ASSET_ID;
        let net = verdict::claim(state, row);
        let v = verdict::verdict(row, net.as_ref(), self.checkpoint);
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
        use verdict::Verdict as V;
        let s = &mut self.samples;
        let bucket = match (v, is_native) {
            (V::Ghost, true) => Some(&mut s.ghost_native),
            (V::Ghost, false) => Some(&mut s.ghost_classic),
            (V::HealFromSnapshot | V::DivergentOursNewer, true) => Some(&mut s.divergent_native),
            (V::HealFromSnapshot | V::DivergentOursNewer, false) => Some(&mut s.divergent_classic),
            (V::ClosedButLive, _) => Some(&mut s.closed_but_live),
            (V::ClosedButLiveConflict, _) => Some(&mut s.closed_but_live_conflict),
            (V::DivergentSameLedger, _) => Some(&mut s.divergent_same_ledger),
            (V::Closure, false) => Some(&mut s.closure_classic),
            (V::Agree, false) => Some(&mut s.agree_classic),
            _ => None,
        };
        // The identity is built only for a row the bucket actually takes. It
        // comes straight from the snapshot, which is what breaks the
        // circularity the audit called out: reversing our surrogates through
        // our own tables meant auditing the tables with themselves.
        if let Some(bucket) = bucket {
            bucket.offer(row.holder_id, row.asset_id, line, || {
                key_line(state, row.holder_id, row.asset_id, is_native)
            });
        }
        (v, net)
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
        // This bucket is fed from a `HashMap` whose hasher is seeded per
        // process, so arrival order differs on every run. `Sample` selects on
        // the key hash, which is order-independent — the same population always
        // yields the same dump, and this bucket needs no rule of its own.
        self.samples.missing_classic.offer(
            key.holder_id,
            key.asset_id,
            || {
                format!(
                    "{}\t{}\t{}\t{}",
                    key.holder_id, key.asset_id, entry.balance, entry.ledger
                )
            },
            || key_line(state, key.holder_id, key.asset_id, false),
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
        s.closed_but_live_conflict
            .write(dir, "closed_but_live_conflict.tsv")?;
        s.divergent_same_ledger
            .write(dir, "divergent_same_ledger.tsv")?;
        Ok(())
    }
}
