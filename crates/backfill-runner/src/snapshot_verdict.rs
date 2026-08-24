//! The comparison rule — one of our rows against what the network holds.
//!
//! Ten verdicts, one function. The report COUNTS them and the seed ACTS on
//! them, and both go through [`verdict`], so they cannot describe different
//! populations. Every arm is a production decision: mis-mapping one either
//! hides a live holding or writes a false number.

use db_clickhouse::persist::ids;

use crate::network_state::{HoldingKey, NetHolding, NetworkState};

// ---------------------------------------------------------------------------
// The verdict — ONE rule, counted by the report and acted on by the seed
// ---------------------------------------------------------------------------

/// Floor on the our-rows read. A short
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
pub fn verdict(row: &OurRow, snap: Option<&mut NetHolding>, checkpoint: u32) -> Verdict {
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
            if e.live {
                // The network holds it. Whether we can FIX that depends on
                // whose ledger is newer; whether it is WRONG does not.
                return if i64::from(e.ledger) > row.last_updated_ledger {
                    Verdict::ClosedButLive
                } else {
                    Verdict::ClosedButLiveConflict
                };
            }
        }
        return Verdict::AlreadyClosed;
    }
    match snap {
        Some(e) if e.live => {
            e.matched = true;
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

    /// FIRST-WINS is the whole anti-resurrection guarantee: the bucket list is
    /// newest-first, so a DeadEntry seen first must not be overwritten by an
    /// older LiveEntry below it. Inverting this comparison would resurrect
    /// every closed trustline on the network, and nothing else in the suite
    /// would notice.
    /// The verdict is the one rule the report counts and the seed acts on.
    /// Each arm below is a production decision: mis-mapping any one of them
    /// either hides a live holding or writes a false number.
    #[test]
    fn verdict_covers_every_bucket() {
        const CP: u32 = 100;

        // Our writer already closed it and the network agrees it is gone — no
        // correction, but the snapshot entry is still MATCHED, or our own
        // closure would be counted as a network gap and re-inserted as live.
        let mut e = NetHolding::dead();
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
        let mut dead = NetHolding::dead();
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

        // Our closure at 90, network's evidence of life is OLDER (or equal).
        // The snapshot still contradicts us — an entry in the checkpoint bucket
        // list is live AT the checkpoint — but no honest version can supersede
        // our closure row, so it is reported as a defect, never healed. This
        // arm used to return `AlreadyClosed`: "we closed it and nothing
        // contradicts us", while the network was contradicting us.
        for entry_ledger in [85, 90] {
            let mut older = live(5, entry_ledger);
            assert_eq!(
                verdict(&row(0, 90, 90), Some(&mut older), CP),
                Verdict::ClosedButLiveConflict,
                "network live at {entry_ledger} vs our closure at 90"
            );
            assert!(older.matched, "still matched: pass 2 must not re-insert it");
        }

        // Network agrees the entry is gone.
        let mut dead = NetHolding::dead();
        assert_eq!(
            verdict(&row(0, 90, 90), Some(&mut dead), CP),
            Verdict::AlreadyClosed
        );
        assert!(dead.matched, "our own closure is not a network gap");
    }
}
