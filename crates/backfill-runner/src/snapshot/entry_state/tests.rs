use super::*;
use crate::snapshot::network_state::{AccountDetail, NetHolding};
use std::collections::HashMap;

const CHECKPOINT_ERA: u32 = 64_131_263;

fn account(state: &mut NetworkState, id: i64, live: bool, ledger: u32) {
    state.accounts.insert(
        id,
        NetHolding {
            live,
            ledger,
            balance: 0,
            matched: false,
        },
    );
    state.account_details.insert(
        id,
        AccountDetail {
            strkey: format!("G{id}"),
            seq_num: 1,
            home_domain: String::new(),
            thresholds: [1, 0, 0, 0],
            flags: 0,
            // Non-zero, so a seed row that dropped them would read as 0.
            num_sponsoring: 4,
            num_sponsored: 2,
            signers: vec![],
        },
    );
}

/// Every live account gets a row, whatever we already hold of it — the
/// version rule, not this pass, decides which row survives. A merged-away
/// account is not ours to write. lore-0629.
#[test]
fn every_live_account_is_written() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, CHECKPOINT_ERA);
    account(&mut state, 2, true, CHECKPOINT_ERA - 1);
    account(&mut state, 3, false, 0); // merged away

    let mut written: Vec<i64> = classify_and_write(&state, &HashMap::new())
        .rows
        .iter()
        .map(|r| r.account_id)
        .collect();
    written.sort_unstable();
    assert_eq!(written, [1, 2]);
}

/// Versioned on the entry's own ledger, never the checkpoint's: a newer live
/// write outranks the seed's row, an equal one is replaced by it.
#[test]
fn rows_take_the_entry_ledger_as_their_version() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, 64_800_534);

    assert_eq!(
        classify_and_write(&state, &HashMap::new()).rows[0].last_updated_ledger,
        64_800_534
    );
}

/// The row carries the CAP-33 counters — what fills an account whose row was
/// written before the columns existed. lore-0629.
#[test]
fn rows_carry_the_sponsorship_counters() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, CHECKPOINT_ERA);

    let row = &classify_and_write(&state, &HashMap::new()).rows[0];
    assert_eq!((row.num_sponsoring, row.num_sponsored), (4, 2));
}

fn ours(account_id: i64, ledger: i64, num_sponsoring: u32, num_sponsored: u32) -> (i64, OurRow) {
    (
        account_id,
        OurRow {
            account_id,
            ledger,
            num_sponsoring,
            num_sponsored,
        },
    )
}

/// Every live account is still written; the counts say what each row does to
/// ours. `same ledger, counters differ` is the repair: a row from before the
/// sponsorship columns, 0 / 0 at the entry's own ledger. The five counts sum
/// to the rows written. lore-0629.
#[test]
fn counts_say_what_each_row_does_to_ours() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, 64_800_534); // ours 0/0 at the same ledger: repaired
    account(&mut state, 2, true, 64_800_534); // ours 4/2 at the same ledger: identical
    account(&mut state, 3, true, 64_816_100); // ours older: stale
    account(&mut state, 4, true, 64_816_100); // ours newer: the live writer wins
    account(&mut state, 5, true, 64_816_100); // never seen
    let ours = HashMap::from([
        ours(1, 64_800_534, 0, 0),
        ours(2, 64_800_534, 4, 2),
        ours(3, 64_800_000, 0, 0),
        ours(4, 64_818_384, 4, 2),
    ]);

    let out = classify_and_write(&state, &ours);

    assert_eq!(out.rows.len(), 5);
    assert_eq!(
        (
            out.missing,
            out.stale,
            out.same_ledger_counters_differ,
            out.same_ledger_counters_equal,
            out.ours_newer
        ),
        (1, 1, 1, 1, 1)
    );
}
