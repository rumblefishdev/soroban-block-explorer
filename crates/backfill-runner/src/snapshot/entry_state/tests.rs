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

    let out = corrections(&state, &ours, None);

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

/// The seed writes the same CAP-33 counters the live writer does, copied from
/// the snapshot's entry — an account last touched before our floor must not
/// read 0 for a sponsor that pays for others. lore-0629.
#[test]
fn seed_row_carries_the_sponsorship_counters() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, CHECKPOINT_ERA);

    let out = corrections(&state, &HashMap::new(), None);

    assert_eq!(out.rows.len(), 1);
    assert_eq!(
        (out.rows[0].num_sponsoring, out.rows[0].num_sponsored),
        (4, 2)
    );
}

/// The ledger the writer of the sponsorship counters went live on mainnet
/// (task 0629): rows older than it carry the columns' default, 0.
const REFILL: u32 = 64_816_029;

/// The refill rewrites exactly the rows older than the refill ledger, each at
/// its own version so this insert replaces it — and leaves every row the new
/// writer has already stamped. The four counts still add up to the live
/// accounts.
#[test]
fn refill_rewrites_only_rows_older_than_the_refill_ledger() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, 64_800_534); // unchanged since before the writer
    account(&mut state, 2, true, REFILL + 10); // the new writer already wrote it
    account(&mut state, 3, true, REFILL + 20); // changed after our row: stale anyway
    account(&mut state, 4, true, REFILL + 30); // never seen
    let ours = HashMap::from([
        (1, 64_800_534),
        (2, i64::from(REFILL) + 10),
        (3, 64_700_000),
    ]);

    let out = corrections(&state, &ours, Some(REFILL));

    let mut written: Vec<(i64, i64)> = out
        .rows
        .iter()
        .map(|r| (r.account_id, r.last_updated_ledger))
        .collect();
    written.sort_unstable();
    assert_eq!(
        written,
        [
            (1, 64_800_534),
            (3, i64::from(REFILL) + 20),
            (4, i64::from(REFILL) + 30)
        ]
    );
    assert_eq!(
        (out.missing, out.stale, out.refilled, out.current),
        (1, 1, 1, 1)
    );
}

/// The refill row carries the counters — the whole point of the pass.
#[test]
fn refill_row_carries_the_sponsorship_counters() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, 64_800_534);
    let ours = HashMap::from([(1, 64_800_534)]);

    let out = corrections(&state, &ours, Some(REFILL));

    assert_eq!(out.refilled, 1);
    assert_eq!(
        (out.rows[0].num_sponsoring, out.rows[0].num_sponsored),
        (4, 2)
    );
}

/// Our row newer than the snapshot's entry (the entry did not change since):
/// the refill row takes OUR version, not the entry's, or the merge would keep
/// the old row with the default.
#[test]
fn refill_row_takes_our_version_when_it_is_newer_than_the_entry() {
    let mut state = NetworkState::default();
    account(&mut state, 1, true, 64_000_000);
    let ours = HashMap::from([(1, 64_500_000)]);

    let out = corrections(&state, &ours, Some(REFILL));

    assert_eq!(out.rows[0].last_updated_ledger, 64_500_000);
}
