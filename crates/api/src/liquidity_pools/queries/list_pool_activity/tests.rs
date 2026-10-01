use super::*;

/// Amounts land in the slot of the leg they belong to, for any number of legs,
/// and a leg that did not move stays `None` — a three-leg pool is the case the
/// old `a` / `b` pair could not express.
#[test]
fn pair_legs_slots_each_amount_by_its_leg() {
    let row = |oi: i16, asset_id: i64, amount: i64| PoolLegChRow {
        ls: 10,
        ao: 1,
        oi,
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

/// A cursor minted before task 0372 carried `transaction_id` and the
/// operation's 1-based `application_order`. It must not decode: its
/// `application_order` would be read as the transaction's position.
#[test]
fn cursor_keyed_on_transaction_id_is_rejected() {
    use crate::common::cursor::{self, CursorError, Direction};
    use crate::liquidity_pools::dto::PoolActivityCursor;

    #[derive(serde::Serialize)]
    struct Old {
        ledger_sequence: i64,
        transaction_id: i64,
        application_order: i16,
    }
    let old = Old {
        ledger_sequence: 64_000_000,
        transaction_id: -123,
        application_order: 2,
    };
    let encoded = cursor::encode(&old, Direction::Next);
    let err = cursor::decode::<PoolActivityCursor>(&encoded).unwrap_err();
    assert!(matches!(err, CursorError::InvalidPayload));
}
