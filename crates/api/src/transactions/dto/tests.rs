use super::*;
use crate::common::cursor::{self, CursorError, Direction};
use chrono::TimeZone;

#[test]
fn surrogate_cursor_round_trips() {
    // The transactions.id hash surrogate may be negative (cityhash64 as i64).
    let c = TxListCursor::ChSurrogate {
        ledger_sequence: 50_000,
        transaction_id: -123,
    };
    let encoded = cursor::encode(&c, Direction::Prev);
    let (dir, decoded): (Direction, TxListCursor) = cursor::decode(&encoded).unwrap();
    assert_eq!(dir, Direction::Prev);
    assert!(matches!(
        decoded,
        TxListCursor::ChSurrogate {
            ledger_sequence: 50_000,
            transaction_id: -123
        }
    ));
}

#[test]
fn position_cursor_round_trips() {
    let c = TxListCursor::ChPosition {
        ledger_sequence: 64_000_000,
        application_order: 7,
    };
    let encoded = cursor::encode(&c, Direction::Next);
    let (_, decoded): (Direction, TxListCursor) = cursor::decode(&encoded).unwrap();
    assert!(matches!(
        decoded,
        TxListCursor::ChPosition {
            ledger_sequence: 64_000_000,
            application_order: 7
        }
    ));
}

#[test]
fn each_statement_takes_only_its_own_keyset() {
    let position = TxListCursor::ChPosition {
        ledger_sequence: 64_000_000,
        application_order: 7,
    };
    let surrogate = TxListCursor::ChSurrogate {
        ledger_sequence: 64_000_000,
        transaction_id: -123,
    };
    // (contract filter, operation-type filter) → the statement's keyset.
    for (contract, op_type, keyed_by_position) in [
        (false, false, true), // A: no filter
        (true, false, true),  // B: contract
        (true, true, true),   // B: contract + operation type
        (false, true, false), // C: operation type only
    ] {
        assert_eq!(
            position.fits_transaction_list(contract, op_type),
            keyed_by_position,
            "position cursor, contract {contract}, op_type {op_type}"
        );
        assert_eq!(
            surrogate.fits_transaction_list(contract, op_type),
            !keyed_by_position,
            "surrogate cursor, contract {contract}, op_type {op_type}"
        );
    }
}

#[test]
fn variant_carries_the_src_tag_on_the_wire() {
    let tag = |c: TxListCursor| serde_json::to_value(c).unwrap()["src"].clone();
    assert_eq!(
        tag(TxListCursor::ChSurrogate {
            ledger_sequence: 1,
            transaction_id: 2
        }),
        "ch_surrogate"
    );
    assert_eq!(
        tag(TxListCursor::ChPosition {
            ledger_sequence: 1,
            application_order: 2
        }),
        "ch_position"
    );
}

#[test]
fn cursor_minted_before_the_split_is_rejected() {
    // The `ch` variant carried the position or the surrogate in one
    // `tiebreak` field. It no longer decodes: a 400 once at the deploy
    // instead of a page read with the wrong key (ADR 0008 clean break).
    #[derive(serde::Serialize)]
    struct Old {
        src: &'static str,
        ledger_sequence: i64,
        tiebreak: i64,
    }
    let old = Old {
        src: "ch",
        ledger_sequence: 64_000_000,
        tiebreak: 7,
    };
    let encoded = cursor::encode(&old, Direction::Next);
    let err = cursor::decode::<TxListCursor>(&encoded).unwrap_err();
    assert!(matches!(err, CursorError::InvalidPayload));
}

#[test]
fn legacy_untagged_cursor_is_rejected() {
    // A pre-0243 `{ts, id}` cursor carries no `src` tag, so it MUST fail
    // to decode as the tagged enum — surfacing as `invalid_cursor` (400)
    // rather than silently promoting to a PG (or CH) walk. This is the
    // ADR 0008 "clean break / no silent-promotion" contract: a cursor
    // that lacks the current backend's intent fails, it does not
    // mis-paginate.
    #[derive(serde::Serialize)]
    struct Legacy {
        ts: DateTime<Utc>,
        id: i64,
    }
    let ts = Utc.with_ymd_and_hms(2026, 5, 29, 12, 0, 0).unwrap();
    let encoded = cursor::encode(&Legacy { ts, id: 7 }, Direction::Prev);
    let err = cursor::decode::<TxListCursor>(&encoded).unwrap_err();
    assert!(matches!(err, CursorError::InvalidPayload));
}
