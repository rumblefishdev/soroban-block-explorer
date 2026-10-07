//! Signers, thresholds, flags and sponsorship counters in the checkpoint seed
//! (task 0521, task 0629).
//!
//! `account_entry_state` versions an account on its entry's own
//! `lastModifiedLedgerSeq`, and the live writer (deployed 2026-08-24) stamps
//! every change since. So the snapshot adds something only for a live account
//! whose entry is newer than our newest row of it — the rule the pool pass
//! applies to its snapshots ([`entry_freshness::need`]), and the one used here.
//!
//! Until task 0521 this pass emitted every live account on every run. Measured
//! on the second production pass (checkpoint 64,132,415): every other
//! correction fell to the week's churn, while this one re-emitted 10,872,072
//! rows identical to what we held. The data was never at risk — the rows
//! collapse at the same version — but a number that cannot move could not show
//! the live writer stopping, which is what it is for.
//!
//! **Refill (task 0629).** A column added to the table reads its default on
//! every row written before it existed — the sponsorship counters read 0 for
//! an account whose entry has not changed since. The version rule cannot see
//! that: the row is current, only incomplete. `--refill-entry-state-older-than
//! <ledger>` rewrites exactly those rows, the ones whose newest version
//! predates the ledger the new writer went live, and no others — so a normal
//! pass afterwards still writes ~0.

use std::collections::HashMap;

use db_clickhouse::persist::rows::AccountEntryStateRow;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::entry_freshness::{self, Need};
use crate::snapshot::network_state::NetworkState;
use crate::snapshot::slices::key_slices;

#[derive(Default)]
pub(crate) struct EntryStateCorrections {
    pub(crate) rows: Vec<AccountEntryStateRow>,
    /// Live accounts with no row of ours.
    pub(crate) missing: u64,
    /// Live accounts whose entry changed after our newest row.
    pub(crate) stale: u64,
    /// Live accounts we hold at the entry's ledger or later, but whose newest
    /// row predates the refill ledger — rewritten whole from the snapshot.
    pub(crate) refilled: u64,
    /// Live accounts we already hold at the entry's ledger or later. With
    /// `missing`, `stale` and `refilled` they sum to the snapshot's
    /// live-account count.
    pub(crate) current: u64,
}

/// Our newest `last_updated_ledger` per account.
///
/// Only the version is read, not the signers: a row with wrong signers at the
/// right ledger is a real defect class, but finding it is an audit (task
/// 0503), not a gate on a load path, and it would carry three arrays per
/// account over the wire. A version map for 10.9M accounts costs ~260 MB of the
/// run's ~4.5 GB peak.
///
/// Sliced on `account_id` like every other read of ours here: the server's
/// `max_execution_time` counts the time spent sending rows. `GROUP BY`
/// collapses the unmerged ReplacingMergeTree parts production carries.
///
/// A short read would only make the pass emit more rows, identical at the
/// same version, so it cannot damage data; the run refuses a profile that can
/// truncate a read anyway (`refuse_if_reads_can_truncate`).
async fn our_newest(sink: &Sink) -> Result<HashMap<i64, i64>, BackfillError> {
    #[derive(clickhouse::Row, serde::Deserialize)]
    struct Newest {
        account_id: i64,
        ledger: i64,
    }
    let mut out = HashMap::new();
    for (from, to) in key_slices() {
        let sql = format!(
            "SELECT account_id, max(last_updated_ledger) AS ledger \
             FROM account_entry_state \
             WHERE account_id BETWEEN {from} AND {to} \
             GROUP BY account_id"
        );
        let mut cursor = sink.client().query(&sql).fetch::<Newest>()?;
        while let Some(r) = cursor.next().await? {
            out.insert(r.account_id, r.ledger);
        }
    }
    Ok(out)
}

pub(crate) async fn build_corrections(
    sink: &Sink,
    state: &NetworkState,
    refill_older_than: Option<u32>,
) -> Result<EntryStateCorrections, BackfillError> {
    let ours = our_newest(sink).await?;
    println!("  read our newest entry state of {} accounts", ours.len());
    Ok(corrections(state, &ours, refill_older_than))
}

/// A row for every live account the snapshot knows newer than `ours` does,
/// and — with a refill ledger — for every one whose newest row of ours is
/// older than that ledger.
fn corrections(
    state: &NetworkState,
    ours: &HashMap<i64, i64>,
    refill_older_than: Option<u32>,
) -> EntryStateCorrections {
    let mut out = EntryStateCorrections::default();
    for (id, e) in &state.accounts {
        if !e.live {
            continue;
        }
        let Some(d) = state.account_details.get(id) else {
            continue;
        };
        let our_newest = ours.get(id).copied();
        let version = match entry_freshness::need(our_newest, e.ledger) {
            Need::Missing => {
                out.missing += 1;
                i64::from(e.ledger)
            }
            Need::Stale => {
                out.stale += 1;
                i64::from(e.ledger)
            }
            Need::Current => {
                // Our row is current but older than the columns it lacks.
                // Rewritten at its own version: the merge then keeps this,
                // the later insert, and any live write after it outranks both.
                if let (Some(ours), Some(refill)) = (our_newest, refill_older_than)
                    && ours < i64::from(refill)
                {
                    out.refilled += 1;
                    ours
                } else {
                    out.current += 1;
                    continue;
                }
            }
        };
        out.rows.push(AccountEntryStateRow {
            account_id: *id,
            signer_keys: d.signers.iter().map(|(k, _, _)| k.clone()).collect(),
            signer_weights: d.signers.iter().map(|(_, w, _)| *w).collect(),
            signer_types: d.signers.iter().map(|(_, _, t)| t.to_string()).collect(),
            master_weight: d.thresholds[0],
            threshold_low: d.thresholds[1],
            threshold_med: d.thresholds[2],
            threshold_high: d.thresholds[3],
            flags: d.flags,
            num_sponsoring: d.num_sponsoring,
            num_sponsored: d.num_sponsored,
            last_updated_ledger: version,
        });
    }
    out
}

#[cfg(test)]
mod tests;
