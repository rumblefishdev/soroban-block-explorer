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
            signers: vec![],
        },
    );
}

/// The case the pass exists for is the second run: every account we already
/// hold at the entry's ledger is the same `AccountEntry`, and re-writing it
/// was the 10.9M-row no-op of task 0521. Only what is new or changed goes in,
/// and the three counts add up to the live accounts.
#[test]
fn only_accounts_newer_than_ours_are_written() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, CHECKPOINT_ERA); // never seen
    account(&mut state, 2, true, CHECKPOINT_ERA); // we are behind
    account(&mut state, 3, true, CHECKPOINT_ERA); // same entry
    account(&mut state, 4, true, CHECKPOINT_ERA); // live writer ahead
    account(&mut state, 5, false, 0); // merged away: not ours to write
    let ours = HashMap::from([
        (2, i64::from(CHECKPOINT_ERA) - 1),
        (3, i64::from(CHECKPOINT_ERA)),
        (4, i64::from(CHECKPOINT_ERA) + 50),
    ]);

    let out = corrections(&state, &ours);

    let mut written: Vec<i64> = out.rows.iter().map(|r| r.account_id).collect();
    written.sort_unstable();
    assert_eq!(written, [1, 2]);
    assert_eq!((out.missing, out.stale, out.current), (1, 1, 2));
    // Versioned on the entry's own ledger, never the checkpoint's.
    assert!(
        out.rows
            .iter()
            .all(|r| r.last_updated_ledger == i64::from(CHECKPOINT_ERA))
    );
}
