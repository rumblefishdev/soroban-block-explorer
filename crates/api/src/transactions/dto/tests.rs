use super::*;
use crate::common::cursor::{self, CursorError, Direction};
use chrono::TimeZone;

#[test]
fn ch_cursor_round_trips() {
    // CH variant: ledger_sequence is the partition key + primary sort;
    // tiebreak is the transactions.id hash surrogate (the SQL `id`
    // column in the (ledger_sequence, id) keyset — may be negative,
    // cityhash64 lower bits as i64).
    let c = TxListCursor::Ch {
        ledger_sequence: 50_000,
        tiebreak: -123,
    };
    let encoded = cursor::encode(&c, Direction::Prev);
    let (dir, decoded): (Direction, TxListCursor) = cursor::decode(&encoded).unwrap();
    assert_eq!(dir, Direction::Prev);
    assert!(matches!(
        decoded,
        TxListCursor::Ch {
            ledger_sequence: 50_000,
            tiebreak: -123
        }
    ));
}

#[test]
fn position_cursor_round_trips_and_fits_only_the_contract_list() {
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
    assert!(decoded.fits_transaction_list(true));
    assert!(!decoded.fits_transaction_list(false));
    // A pre-0541 contract-list cursor (keyed by the id surrogate) is refused.
    let old = TxListCursor::Ch {
        ledger_sequence: 64_000_000,
        tiebreak: -123,
    };
    assert!(!old.fits_transaction_list(true));
    assert!(old.fits_transaction_list(false));
}

#[test]
fn variant_carries_the_src_tag_on_the_wire() {
    // The `src` discriminant is what lets `list_transactions` reject a
    // stale PG cursor (ADR 0008 fail-clean): a decoded cursor without the
    // current `ch` tag is refused.
    assert_eq!(
        serde_json::to_value(TxListCursor::Ch {
            ledger_sequence: 1,
            tiebreak: 2
        })
        .unwrap()["src"],
        "ch"
    );
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
