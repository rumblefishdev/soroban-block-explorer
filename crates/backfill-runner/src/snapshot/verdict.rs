//! The comparison rule — one of our rows against what the network holds.
//!
//! Eleven verdicts, one function. The report COUNTS them and the seed ACTS on
//! them, and both go through [`verdict`], so they cannot describe different
//! populations. Every arm is a production decision: mis-mapping one either
//! hides a live holding or writes a false number.

use db_clickhouse::persist::ids;

use crate::snapshot::network_state::{HoldingKey, NetHolding, NetworkState};

// ---------------------------------------------------------------------------
// The verdict — ONE rule, counted by the report and acted on by the seed
// ---------------------------------------------------------------------------

/// One of our deduplicated `balances` rows, exactly the SELECT list of
/// `seed::slice_sql`. Shared by the report and the write so the
/// two consumers cannot drift into different column meanings.
#[derive(Debug, Clone, Copy, clickhouse::Row, serde::Deserialize)]
pub struct OurRow {
    pub holder_id: i64,
    pub asset_id: i64,
    pub amount: i128,
    pub last_updated_ledger: i64,
    pub closed_at_ledger: i64,
}

/// What the snapshot says about one of our rows. The report COUNTS these;
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
    /// We stamped it CLOSED, and the network holds it LIVE — but at a ledger
    /// NOT NEWER than our closure. The snapshot still contradicts us: an entry
    /// present in the checkpoint bucket list is live AT the checkpoint whatever
    /// its own last-modified ledger says. What is missing is a way to fix it
    /// honestly — re-opening at the entry's own (older-or-equal) ledger cannot
    /// outversion our closure row, and inventing a newer version would be a
    /// synthetic stamp, the very defect task 0492 documents.
    ///
    /// So this is a DEFECT SIGNAL, not a correction: something closed a
    /// holding the network still has. Unreachable on a first seed run (nothing
    /// is stamped closed yet); it becomes the alarm on the reconciliation runs
    /// ADR 0057 makes mandatory, where the closures under test are our own
    /// previous output or the live writer's.
    ///
    /// It was previously folded into [`Verdict::AlreadyClosed`] — reported as
    /// "we closed it and nothing contradicts us", which was exactly wrong.
    ClosedButLiveConflict,
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
    /// so this is a defect signal, not routine drift. Reported apart and
    /// QUARANTINED, never auto-healed: guessing a winner would bury the
    /// evidence.
    ///
    /// Characterised 2026-08-24 on the full population (17,798 rows): OUR
    /// amount is lower than the chain's in every single one, the defect is
    /// LIVE (ledgers up to the checkpoint itself, ~96 distinct ledgers in the
    /// newest band alone), and 106 of 107 recent (account, ledger) pairs carry
    /// a SOROBAN transaction — zero classic. Repairing these belongs to the
    /// Soroban-writer defect task together with its root cause; a heal here
    /// was built, dry-run-verified (200/200 healed values equal to chain), and
    /// REMOVED — correcting the symptom from the seed while the writer keeps
    /// producing new ties at ~1,900/week would silently decay.
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

/// The single classification rule — a pure function of our row, what the
/// network holds for that key (`None` = nothing at all), and the checkpoint.
///
/// Claiming the entry (`matched`) is the CALLER's job, done once at the single
/// call site. It used to happen here, as a side effect in three separate arms;
/// proving it was unconditional then meant walking all three, and the `&mut`
/// it forced propagated into the seed, which mutates nothing and paid a second
/// lookup of the key the report had just found.
pub fn verdict(row: &OurRow, snap: Option<&NetHolding>, checkpoint: u32) -> Verdict {
    // A row the live writer touched after the snapshot was taken is one the
    // snapshot CANNOT know about, so no comparison against it means anything.
    // This has to come first, before both the closure branch and the live-entry
    // match, or ordinary post-checkpoint churn is filed as disagreement:
    //
    // - measured on the 2026-08-24 dry-run, 1000 of 1000 sampled rows in BOTH
    //   `divergent ours-newer` buckets were simply newer than the checkpoint —
    //   the bucket an operator reads as "our parser and the network disagree"
    //   held no disagreement at all;
    // - worse, a trustline our writer CLOSES between the checkpoint and the run
    //   would meet a snapshot that still calls it live and be reported as
    //   `ClosedButLiveConflict`, a defect signal. That bucket reads 0 today
    //   only because the lifecycle writer is not deployed yet; the deployment
    //   order puts the writer FIRST, so the false positives would have started
    //   with the very run this exists to make trustworthy.
    //
    // `>=`, not `>`: a row last touched AT the checkpoint would be written back
    // at that same ledger, and ReplacingMergeTree resolves an equal version
    // arbitrarily — the closure might lose and the ghost survive,
    // nondeterministically and undetectably. Ceding a handful of legitimate
    // closures is the fail-safe direction.
    if row.last_updated_ledger >= i64::from(checkpoint) {
        return Verdict::NewerThanCheckpoint;
    }
    if row.closed_at_ledger != 0 {
        // "Accounted for" is not "correctly closed". If the network holds the
        // entry LIVE, our closure is contradicted — before this check the row
        // was simply reported as already-closed, silently and permanently.
        if let Some(e) = snap
            && e.live
        {
            // Whether we can FIX it depends on whose ledger is newer; whether
            // it is WRONG does not.
            return if i64::from(e.ledger) > row.last_updated_ledger {
                Verdict::ClosedButLive
            } else {
                Verdict::ClosedButLiveConflict
            };
        }
        return Verdict::AlreadyClosed;
    }
    match snap {
        Some(e) if e.live => {
            if i128::from(e.balance) != row.amount {
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
        // Rows newer than the checkpoint already returned above.
        _ if row.amount == 0 => Verdict::Closure,
        _ => Verdict::Ghost,
    }
}

/// The balance-row change a verdict implies, without the row's identity.
///
/// Split out of the seed's fold so the WRITE policy is testable the way the
/// classification already is: `verdict()` says what we are looking at, this
/// says what we do about it, and both are pure. A wrong arm here — a closure
/// written open, a heal versioned on the wrong ledger — would otherwise be
/// invisible until production.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Correction {
    pub amount: i128,
    /// Becomes `last_updated_ledger`, the ReplacingMergeTree version.
    pub last_updated_ledger: i64,
    pub closed_at_ledger: i64,
}

/// What a verdict writes, or `None` for the report-only ones.
///
/// `net` is the network's holding for this key; the live-adopting verdicts
/// cannot be produced without one, so its absence yields `None` rather than a
/// fabricated row.
///
/// Version discipline, in one place: a live fact versions on the ENTRY's own
/// ledger, an absence fact (closure, ghost) on the checkpoint — "true at or
/// before". Never a synthetic stamp (task 0492).
pub fn correction(v: Verdict, net: Option<&NetHolding>, checkpoint: u32) -> Option<Correction> {
    match v {
        // Report-only. The two defect signals (same-ledger divergence,
        // closed-but-live conflict) are here deliberately: picking a winner, or
        // inventing a version, would bury the only evidence.
        Verdict::AlreadyClosed
        | Verdict::Agree
        | Verdict::DivergentOursNewer
        | Verdict::Stale
        | Verdict::DivergentSameLedger
        | Verdict::ClosedButLiveConflict
        | Verdict::NewerThanCheckpoint => None,
        // We hid a holding the network says is live at a NEWER ledger: re-open
        // at the entry's own ledger, which outversions our wrong closure. Heal:
        // the snapshot is strictly newer AND the amounts differ, so adopt its
        // amount at ITS ledger.
        Verdict::ClosedButLive | Verdict::HealFromSnapshot => net.map(|e| Correction {
            amount: i128::from(e.balance),
            last_updated_ledger: i64::from(e.ledger),
            closed_at_ledger: 0,
        }),
        Verdict::Closure | Verdict::Ghost => Some(Correction {
            amount: 0,
            last_updated_ledger: i64::from(checkpoint),
            closed_at_ledger: i64::from(checkpoint),
        }),
    }
}

/// Look up the snapshot entry for one of our rows. Native lives on the
/// `AccountEntry`, everything else is a trustline — the split both consumers
/// need, spelled once.
pub fn holding_for<'a>(state: &'a mut NetworkState, row: &OurRow) -> Option<&'a mut NetHolding> {
    if row.asset_id == ids::NATIVE_ASSET_ID {
        state.accounts.get_mut(&row.holder_id)
    } else {
        state.trustlines.get_mut(&HoldingKey {
            holder_id: row.holder_id,
            asset_id: row.asset_id,
        })
    }
}

/// Claim the network's holding for one of our rows and hand back a copy.
///
/// EVERY row of ours claims its entry, whatever the verdict turns out to be —
/// including our own closures, which are accounted for even though they write
/// nothing. Leaving one unclaimed would count it as a network gap and
/// re-insert our own closure as a live holding.
///
/// Whatever stays unclaimed once every row has passed through is therefore
/// exactly what the network holds and we do not. The mark is made HERE, once,
/// rather than inside the classification rule: one place to read, one place to
/// get it wrong.
pub fn claim(state: &mut NetworkState, row: &OurRow) -> Option<NetHolding> {
    holding_for(state, row).map(|e| {
        e.matched = true;
        *e
    })
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

    fn live(balance: i64, ledger: u32) -> NetHolding {
        NetHolding {
            live: true,
            ledger,
            balance,
            matched: false,
        }
    }

    /// The verdict is the one rule the report counts and the seed acts on.
    /// Each arm below is a production decision: mis-mapping any one of them
    /// either hides a live holding or writes a false number.
    #[test]
    fn verdict_covers_every_bucket() {
        const CP: u32 = 100;

        // Our writer already closed it and the network agrees it is gone.
        let e = NetHolding::dead();
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&e), CP),
            Verdict::AlreadyClosed
        );

        let e = live(5, 90);
        assert_eq!(verdict(&row(5, 90, 0), Some(&e), CP), Verdict::Agree);

        // Amounts differ: who is newer decides whether we adopt or keep.
        let e = live(5, 95);
        assert_eq!(
            verdict(&row(9, 90, 0), Some(&e), CP),
            Verdict::HealFromSnapshot
        );
        let e = live(5, 90);
        assert_eq!(
            verdict(&row(9, 95, 0), Some(&e), CP),
            Verdict::DivergentOursNewer
        );

        let e = live(5, 95);
        assert_eq!(verdict(&row(5, 90, 0), Some(&e), CP), Verdict::Stale);

        // Absent from the snapshot: zero is a closure, positive is a ghost —
        // never folded together, because the second means our data lies.
        assert_eq!(verdict(&row(0, 90, 0), None, CP), Verdict::Closure);
        assert_eq!(verdict(&row(42, 90, 0), None, CP), Verdict::Ghost);

        // Present but dead reads the same as absent.
        let dead = NetHolding::dead();
        assert_eq!(verdict(&row(0, 90, 0), Some(&dead), CP), Verdict::Closure);

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
        let e = live(5, 90);
        assert_eq!(
            verdict(&row(9, 90, 0), Some(&e), CP),
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
        let newer = live(5, 95);
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&newer), CP),
            Verdict::ClosedButLive,
            "the network says live at a NEWER ledger — our closure is wrong"
        );

        // Our closure at 90, network's evidence of life is OLDER (or equal).
        // The snapshot still contradicts us — an entry in the checkpoint bucket
        // list is live AT the checkpoint — but no honest version can supersede
        // our closure row, so it is reported as a defect, never healed. This
        // arm used to return `AlreadyClosed`: "we closed it and nothing
        // contradicts us", while the network was contradicting us.
        for entry_ledger in [85, 90] {
            let older = live(5, entry_ledger);
            assert_eq!(
                verdict(&row(0, 90, 90), Some(&older), CP),
                Verdict::ClosedButLiveConflict,
                "network live at {entry_ledger} vs our closure at 90"
            );
        }

        // Network agrees the entry is gone.
        let dead = NetHolding::dead();
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&dead), CP),
            Verdict::AlreadyClosed
        );
    }

    /// The rule that used to live inside `verdict` as a side effect, now with
    /// a test of its own: EVERY row of ours claims its entry, whatever the
    /// verdict. An unclaimed entry is counted as a network gap and re-inserted
    /// as a live holding — so a row that claims nothing would resurrect our own
    /// closure. Native and classic take different maps; both must claim.
    /// Anything the live writer touched after the checkpoint is churn the
    /// snapshot cannot see, whatever the snapshot happens to say about it.
    ///
    /// The second case is the one that bites in production: the deployment
    /// order runs the lifecycle writer BEFORE this seed, so between the
    /// checkpoint and the run our writer really does close trustlines the
    /// snapshot still lists as live. Comparing them yields a `ClosedButLive*`
    /// verdict — one of the two defect signals — for a row that is simply
    /// newer. The whole point of that signal is that it should never fire on
    /// healthy data.
    #[test]
    fn post_checkpoint_rows_are_churn_not_disagreement() {
        const CP: u32 = 1_000;
        for (name, snap) in [
            ("network says live, different amount", Some(live(50, 900))),
            ("network says live, same amount", Some(live(10, 900))),
            ("network has nothing", None),
        ] {
            for closed_at in [0, 1_005] {
                let r = row(10, 1_005, closed_at);
                assert_eq!(
                    verdict(&r, snap.as_ref(), CP),
                    Verdict::NewerThanCheckpoint,
                    "{name}, closed_at {closed_at}"
                );
                assert_eq!(
                    correction(verdict(&r, snap.as_ref(), CP), snap.as_ref(), CP),
                    None,
                    "a post-checkpoint row must never be written back"
                );
            }
        }
        // The boundary itself is ceded on purpose: an equal RMT version is
        // resolved arbitrarily, so a row AT the checkpoint is left alone too.
        assert_eq!(
            verdict(&row(10, i64::from(CP), 0), None, CP),
            Verdict::NewerThanCheckpoint
        );
        assert_eq!(
            verdict(&row(0, i64::from(CP) - 1, 0), None, CP),
            Verdict::Closure,
            "one ledger below the checkpoint is still comparable"
        );
    }

    #[test]
    fn every_row_claims_its_entry_whatever_the_verdict() {
        use crate::snapshot::network_state::HoldingKey;

        let mut state = NetworkState::default();
        // Two rows that end in OPPOSITE verdicts: a healthy classic trustline
        // and a closure whose entry the network still lists as dead.
        state.trustlines.insert(
            HoldingKey {
                holder_id: 1,
                asset_id: 2,
            },
            live(5, 90),
        );
        state.accounts.insert(1, NetHolding::dead());

        let classic = row(5, 90, 0);
        let native = OurRow {
            asset_id: ids::NATIVE_ASSET_ID,
            ..row(0, 90, 90)
        };

        let a = claim(&mut state, &classic).expect("the trustline is in the state");
        assert_eq!((a.live, a.balance, a.ledger), (true, 5, 90));
        let b = claim(&mut state, &native).expect("the account is in the state");
        assert!(!b.live, "the network lists the account as gone");

        assert!(
            state.trustlines[&HoldingKey {
                holder_id: 1,
                asset_id: 2
            }]
                .matched,
            "an ordinary agreeing row must claim its entry"
        );
        assert!(
            state.accounts[&1].matched,
            "our own closure is not a network gap — it must claim its entry too"
        );

        // A key the network does not hold claims nothing and says so.
        let absent = OurRow {
            holder_id: 999,
            ..classic
        };
        assert!(claim(&mut state, &absent).is_none());
    }

    /// The WRITE half of the policy, held to the same standard as the
    /// classification: every verdict either writes nothing, or writes a row
    /// whose version is the ledger of the fact it records.
    #[test]
    fn every_verdict_writes_the_row_its_name_promises() {
        const CP: u32 = 100;
        use Verdict as V;
        let net = live(7, 95);

        for v in [
            V::AlreadyClosed,
            V::Agree,
            V::DivergentOursNewer,
            V::Stale,
            V::DivergentSameLedger,
            V::ClosedButLiveConflict,
            V::NewerThanCheckpoint,
        ] {
            assert_eq!(correction(v, Some(&net), CP), None, "{v:?} must not write");
        }

        // Live-adopting: the network's amount, versioned on the ENTRY's ledger,
        // and explicitly re-opened.
        for v in [V::ClosedButLive, V::HealFromSnapshot] {
            assert_eq!(
                correction(v, Some(&net), CP),
                Some(Correction {
                    amount: 7,
                    last_updated_ledger: 95,
                    closed_at_ledger: 0,
                }),
                "{v:?} adopts the network fact at the network's ledger"
            );
            assert_eq!(
                correction(v, None, CP),
                None,
                "{v:?} without a network fact must not fabricate one"
            );
        }

        // Absence facts: zeroed, stamped, versioned at the checkpoint.
        for v in [V::Closure, V::Ghost] {
            assert_eq!(
                correction(v, None, CP),
                Some(Correction {
                    amount: 0,
                    last_updated_ledger: 100,
                    closed_at_ledger: 100,
                }),
                "{v:?} closes at the checkpoint"
            );
        }
    }
}
