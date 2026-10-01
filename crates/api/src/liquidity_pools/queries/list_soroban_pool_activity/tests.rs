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
fn one_trade_is_one_operation_with_its_legs() {
    let ops = fold_movements(
        vec![
            row(10, 0, 4, 0, 101, "500"),
            row(10, 0, 4, 0, 102, "-499"),
            row(10, 0, 4, 0, 103, "0"),
        ],
        &LEGS,
        false,
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].sums, vec![Some(500), Some(-499), Some(0)]);
    assert_eq!(ops[0].event(), Some(PoolEvent::Trade));
}

#[test]
fn an_unmerged_duplicate_is_counted_once() {
    let ops = fold_movements(
        vec![
            row(10, 0, 4, 1, 101, "7"),
            row(10, 0, 4, 1, 101, "7"),
            row(10, 0, 4, 1, 102, "9"),
            row(10, 0, 4, 1, 102, "9"),
        ],
        &LEGS[..2],
        false,
    );
    assert_eq!(ops[0].sums, vec![Some(7), Some(9)]);
}

#[test]
fn several_events_of_one_operation_are_summed_per_leg() {
    // A router splitting one swap into two trades against the same pool.
    let ops = fold_movements(
        vec![
            row(10, 2, 1, 0, 101, "100"),
            row(10, 2, 1, 0, 102, "-90"),
            row(10, 2, 3, 0, 101, "50"),
            row(10, 2, 3, 0, 102, "-44"),
        ],
        &LEGS[..2],
        false,
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].sums, vec![Some(150), Some(-134)]);
    assert_eq!(ops[0].event(), Some(PoolEvent::Trade));
}

#[test]
fn a_stored_kind_names_a_one_sided_deposit() {
    // Signs alone would call `+5 / 0` a trade; the stored kind says deposit.
    let ops = fold_movements(
        vec![row(10, 0, 0, 1, 101, "5"), row(10, 0, 0, 1, 102, "0")],
        &LEGS[..2],
        false,
    );
    assert_eq!(ops[0].event(), Some(PoolEvent::Deposit));
}

#[test]
fn mixed_kinds_are_named_by_the_summed_signs() {
    let ops = fold_movements(
        vec![
            row(10, 0, 0, 1, 101, "10"),
            row(10, 0, 0, 1, 102, "10"),
            row(10, 0, 1, 0, 101, "5"),
            row(10, 0, 1, 0, 102, "-4"),
        ],
        &LEGS[..2],
        false,
    );
    assert_eq!(ops[0].kind, None);
    assert_eq!(ops[0].sums, vec![Some(15), Some(6)]);
    assert_eq!(ops[0].event(), Some(PoolEvent::Deposit));
}

#[test]
fn a_leg_with_no_row_stays_none_and_the_event_holds() {
    // A 4-token pool's event names 3 tokens at most (soroban events carry 4
    // topics): the fourth leg has no row. Production has one such deposit.
    let ops = fold_movements(
        vec![row(10, 0, 0, 1, 101, "3"), row(10, 0, 0, 1, 102, "4")],
        &LEGS,
        false,
    );
    assert_eq!(ops[0].sums, vec![Some(3), Some(4), None]);
    assert_eq!(ops[0].event(), Some(PoolEvent::Deposit));
}

#[test]
fn mixed_kinds_with_a_missing_leg_have_no_event() {
    let ops = fold_movements(
        vec![row(10, 0, 0, 1, 101, "3"), row(10, 0, 1, 0, 102, "-4")],
        &LEGS,
        false,
    );
    assert_eq!(ops[0].event(), None);
}

#[test]
fn an_asset_outside_the_legs_is_ignored() {
    let ops = fold_movements(
        vec![row(10, 0, 0, 0, 999, "1"), row(10, 0, 0, 0, 101, "2")],
        &LEGS[..2],
        false,
    );
    assert_eq!(ops[0].sums, vec![Some(2), None]);
}

#[test]
fn an_amount_beyond_i64_stays_exact() {
    let big = "123456789012345678901234567890";
    let ops = fold_movements(vec![row(10, 0, 0, 1, 101, big)], &LEGS[..1], false);
    assert_eq!(ops[0].sums[0].map(|v| v.to_string()).as_deref(), Some(big));
}

#[test]
fn a_truncated_read_drops_its_last_operation() {
    let rows = vec![
        row(11, 0, 0, 0, 101, "1"),
        row(11, 0, 0, 0, 102, "-1"),
        row(10, 0, 0, 0, 101, "2"),
    ];
    let ops = fold_movements(rows, &LEGS[..2], true);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].ls, 11);
}
