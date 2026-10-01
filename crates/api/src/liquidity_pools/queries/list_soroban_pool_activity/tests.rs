use super::*;

const LEGS: [i64; 3] = [101, 102, 103];

fn row(ls: i64, oi: u16, ei: u32, kind: u8, asset_id: i64, amount: &str) -> MovementChRow {
    MovementChRow {
        ls,
        ao: 1,
        oi,
        ei,
        kind,
        asset_id,
        amount: amount.to_string(),
    }
}

#[test]
fn one_trade_is_one_row_with_its_legs() {
    let rows = group_movements(
        vec![row(10, 0, 4, 0, 101, "500"), row(10, 0, 4, 0, 102, "-499")],
        &LEGS,
        false,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ei, 4);
    assert_eq!(rows[0].amounts, vec![Some(500), Some(-499), None]);
    assert_eq!(rows[0].event(), Some(PoolEvent::Trade));
}

#[test]
fn an_unmerged_duplicate_is_counted_once() {
    let rows = group_movements(
        vec![
            row(10, 0, 4, 1, 101, "7"),
            row(10, 0, 4, 1, 101, "7"),
            row(10, 0, 4, 1, 102, "9"),
            row(10, 0, 4, 1, 102, "9"),
        ],
        &LEGS[..2],
        false,
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].amounts, vec![Some(7), Some(9)]);
}

/// The shape that summing per operation got wrong: a withdrawal and a
/// re-deposit in one call netted to a one-unit "trade". Each event is its own
/// row, named by its own kind.
#[test]
fn each_event_of_one_operation_is_its_own_row() {
    let rows = group_movements(
        vec![
            row(10, 2, 0, 2, 101, "0"),
            row(10, 2, 0, 2, 102, "-4999999999"),
            row(10, 2, 1, 1, 101, "0"),
            row(10, 2, 1, 1, 102, "5000000000"),
        ],
        &LEGS[..2],
        false,
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].event(), Some(PoolEvent::Withdrawal));
    assert_eq!(rows[0].amounts, vec![Some(0), Some(-4_999_999_999)]);
    assert_eq!(rows[1].event(), Some(PoolEvent::Deposit));
    assert_eq!(rows[1].amounts, vec![Some(0), Some(5_000_000_000)]);
}

#[test]
fn a_stored_kind_names_a_one_sided_deposit() {
    // Signs alone would call `+5 / 0` a trade; the stored kind says deposit.
    let rows = group_movements(
        vec![row(10, 0, 0, 1, 101, "5"), row(10, 0, 0, 1, 102, "0")],
        &LEGS[..2],
        false,
    );
    assert_eq!(rows[0].event(), Some(PoolEvent::Deposit));
}

#[test]
fn an_unknown_stored_kind_has_no_event() {
    let rows = group_movements(vec![row(10, 0, 0, 9, 101, "5")], &LEGS[..1], false);
    assert_eq!(rows[0].event(), None);
}

#[test]
fn a_leg_with_no_row_stays_none_and_the_event_holds() {
    // A 4-token pool's event names 3 tokens at most (soroban events carry 4
    // topics): the fourth leg has no row. Production has one such deposit.
    let rows = group_movements(
        vec![row(10, 0, 0, 1, 101, "3"), row(10, 0, 0, 1, 102, "4")],
        &LEGS,
        false,
    );
    assert_eq!(rows[0].amounts, vec![Some(3), Some(4), None]);
    assert_eq!(rows[0].event(), Some(PoolEvent::Deposit));
}

#[test]
fn an_asset_outside_the_legs_is_ignored() {
    let rows = group_movements(
        vec![row(10, 0, 0, 0, 999, "1"), row(10, 0, 0, 0, 101, "2")],
        &LEGS[..2],
        false,
    );
    assert_eq!(rows[0].amounts, vec![Some(2), None]);
}

#[test]
fn an_unparseable_amount_leaves_its_leg_unknown() {
    let rows = group_movements(
        vec![row(10, 0, 0, 0, 101, "x"), row(10, 0, 0, 0, 102, "2")],
        &LEGS[..2],
        false,
    );
    assert_eq!(rows[0].amounts, vec![None, Some(2)]);
}

#[test]
fn an_amount_beyond_i64_stays_exact() {
    let big = "123456789012345678901234567890";
    let rows = group_movements(vec![row(10, 0, 0, 1, 101, big)], &LEGS[..1], false);
    assert_eq!(
        rows[0].amounts[0].map(|v| v.to_string()).as_deref(),
        Some(big)
    );
}

#[test]
fn a_truncated_read_drops_its_last_event() {
    let rows = vec![
        row(11, 0, 0, 0, 101, "1"),
        row(11, 0, 0, 0, 102, "-1"),
        row(10, 0, 0, 0, 101, "2"),
    ];
    let grouped = group_movements(rows, &LEGS[..2], true);
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].ls, 11);
}
