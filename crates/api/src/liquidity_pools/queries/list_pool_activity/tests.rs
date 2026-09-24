use super::*;

/// Amounts land in the slot of the leg they belong to, for any number of legs,
/// and a leg that did not move stays `None` — a three-leg pool is the case the
/// old `a` / `b` pair could not express.
#[test]
fn pair_legs_slots_each_amount_by_its_leg() {
    let row = |ao: i16, asset_id: i64, amount: i64| PoolLegChRow {
        ls: 10,
        tid: 1,
        ao,
        asset_id,
        amount,
    };
    let legs = [7, 8, 9];
    // Op 1 moved legs 9 and 7 (out of order in the stream); op 2 moved all three.
    let rows = vec![
        row(1, 9, -5),
        row(1, 7, 4),
        row(2, 7, 1),
        row(2, 8, 2),
        row(2, 9, 3),
    ];
    let ops = pair_legs(rows, &legs, false);
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].amounts, vec![Some(4), None, Some(-5)]);
    // Not every leg landed, so no event is claimed for it.
    assert_eq!(ops[0].event(), None);
    assert_eq!(ops[1].amounts, vec![Some(1), Some(2), Some(3)]);
    assert_eq!(ops[1].event(), Some(PoolEvent::Deposit));
}
