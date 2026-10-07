use super::*;
use crate::snapshot::network_state::{AccountDetail, NetHolding};

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

    let mut written: Vec<i64> = rows(&state).iter().map(|r| r.account_id).collect();
    written.sort_unstable();
    assert_eq!(written, [1, 2]);
}

/// Versioned on the entry's own ledger, never the checkpoint's: a newer live
/// write outranks the seed's row, an equal one is replaced by it.
#[test]
fn rows_take_the_entry_ledger_as_their_version() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, 64_800_534);

    assert_eq!(rows(&state)[0].last_updated_ledger, 64_800_534);
}

/// The row carries the CAP-33 counters — what fills an account whose row was
/// written before the columns existed. lore-0629.
#[test]
fn rows_carry_the_sponsorship_counters() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, CHECKPOINT_ERA);

    let row = &rows(&state)[0];
    assert_eq!((row.num_sponsoring, row.num_sponsored), (4, 2));
}
