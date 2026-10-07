//! Signers, thresholds, flags and sponsorship counters in the checkpoint seed
//! (task 0521, task 0629).
//!
//! One row for every live account in the snapshot, versioned on the entry's
//! own `lastModifiedLedgerSeq` — the version rule decides the rest. Where our
//! row is at the same ledger, the seed's row is the later insert and replaces
//! it; where the live writer has stamped a newer change, ours outranks the
//! seed's and stays.
//!
//! Task 0521 had narrowed this pass to accounts newer than our newest row, so
//! a repeat pass wrote ~0 rows and a sudden million would show the live writer
//! stopping. Task 0629 went back to every account: a column added to the table
//! reads its default on every row written before it existed (the sponsorship
//! counters read 0 for an account whose entry has not changed since), and the
//! version rule cannot see that — the row is current, only incomplete. Writing
//! every account fills such a column on the next ordinary pass, with no extra
//! mode to remember. The cost is ~10.9M rows per pass, identical where nothing
//! changed, which the ReplacingMergeTree collapses.

use db_clickhouse::persist::rows::AccountEntryStateRow;

use crate::snapshot::network_state::NetworkState;

/// A row for every live account the snapshot holds, at the entry's own ledger.
pub(crate) fn rows(state: &NetworkState) -> Vec<AccountEntryStateRow> {
    let mut out = Vec::new();
    for (id, e) in &state.accounts {
        if !e.live {
            continue;
        }
        let Some(d) = state.account_details.get(id) else {
            continue;
        };
        out.push(AccountEntryStateRow {
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
            last_updated_ledger: i64::from(e.ledger),
        });
    }
    out
}

#[cfg(test)]
mod tests;
