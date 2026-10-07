//! Signers, thresholds, flags and sponsorship counters in the checkpoint seed
//! (task 0521, task 0629).
//!
//! One row for every live account in the snapshot, versioned on the entry's
//! own `lastModifiedLedgerSeq` — the version rule decides the rest. Where our
//! row is at the same ledger, the seed's row is the later insert and replaces
//! it; where the live writer has stamped a newer change, ours outranks the
//! seed's and stays.
//!
//! Every account, not only the ones newer than ours: a column added to the
//! table reads its default on every row written before its writer went live
//! (the sponsorship counters read 0 for an account whose entry has not changed
//! since), and the version rule cannot see that — the row is current, only
//! incomplete. ~10.9M rows per pass, identical where nothing changed, which
//! the ReplacingMergeTree collapses.
//!
//! Before writing, the pass reads our newest row of each account and counts
//! what the write will do. `same ledger, counters differ` is the number of
//! accounts it repairs: high on the first pass after a new column, ~0 on the
//! next — a jump later means the live writer stopped stamping the counters.

use std::collections::HashMap;

use db_clickhouse::persist::rows::AccountEntryStateRow;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::snapshot::network_state::NetworkState;
use crate::snapshot::slices::key_slices;

/// Our newest row of one account: its version and the two counters a
/// pre-column row carries as defaults.
#[derive(clickhouse::Row, serde::Deserialize)]
struct OurRow {
    account_id: i64,
    ledger: i64,
    num_sponsoring: u32,
    num_sponsored: u32,
}

#[derive(Default)]
pub(crate) struct EntryState {
    /// One row per live account in the snapshot.
    pub(crate) rows: Vec<AccountEntryStateRow>,
    /// Live accounts with no row of ours.
    pub(crate) missing: u64,
    /// Live accounts whose entry changed after our newest row.
    pub(crate) stale: u64,
    /// Our newest row is at the entry's ledger and its counters differ — the
    /// accounts this pass repairs.
    pub(crate) same_ledger_counters_differ: u64,
    /// Our newest row is at the entry's ledger with the same counters.
    pub(crate) same_ledger_counters_equal: u64,
    /// Our newest row is newer than the snapshot — the seed's row loses.
    pub(crate) ours_newer: u64,
}

/// Our newest row per account, sliced on `account_id` like every other read
/// of ours here: the server's `max_execution_time` counts the time spent
/// sending rows. `argMax` picks the counters of the newest version; a version
/// map plus two counters for 11.2M accounts costs ~350 MB of the run's peak.
async fn our_rows(sink: &Sink) -> Result<HashMap<i64, OurRow>, BackfillError> {
    let mut out = HashMap::new();
    for (from, to) in key_slices() {
        let sql = format!(
            "SELECT account_id, \
                    max(last_updated_ledger) AS ledger, \
                    argMax(num_sponsoring, last_updated_ledger) AS num_sponsoring, \
                    argMax(num_sponsored, last_updated_ledger) AS num_sponsored \
             FROM account_entry_state \
             WHERE account_id BETWEEN {from} AND {to} \
             GROUP BY account_id"
        );
        let mut cursor = sink.client().query(&sql).fetch::<OurRow>()?;
        while let Some(r) = cursor.next().await? {
            out.insert(r.account_id, r);
        }
    }
    Ok(out)
}

pub(crate) async fn build(sink: &Sink, state: &NetworkState) -> Result<EntryState, BackfillError> {
    let ours = our_rows(sink).await?;
    println!("  read our newest entry state of {} accounts", ours.len());
    Ok(classify_and_write(state, &ours))
}

/// A row for every live account the snapshot holds, at the entry's own
/// ledger, and the count of what each row will do against ours.
fn classify_and_write(state: &NetworkState, ours: &HashMap<i64, OurRow>) -> EntryState {
    let mut out = EntryState::default();
    for (id, e) in &state.accounts {
        if !e.live {
            continue;
        }
        let Some(d) = state.account_details.get(id) else {
            continue;
        };
        let entry_ledger = i64::from(e.ledger);
        match ours.get(id) {
            None => out.missing += 1,
            Some(o) if o.ledger < entry_ledger => out.stale += 1,
            Some(o) if o.ledger > entry_ledger => out.ours_newer += 1,
            Some(o)
                if o.num_sponsoring == d.num_sponsoring && o.num_sponsored == d.num_sponsored =>
            {
                out.same_ledger_counters_equal += 1
            }
            Some(_) => out.same_ledger_counters_differ += 1,
        }
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
            last_updated_ledger: entry_ledger,
        });
    }
    out
}

#[cfg(test)]
mod tests;
