//! Live-CH **decode** smoke for the account-transactions keyset read.
//!
//! The driver seek, the page fetch by position and the cursor round-trip only
//! parse against a real server; the unit tests cannot see a column that does
//! not exist. Walks two pages in both directions for an account that has
//! transactions, feeding the first page's boundary row back as the cursor.
//!
//! **Skips cleanly when `CH_URL` is unset**, so CI (no CH access) is green:
//!
//! ```text
//! CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!   cargo test -p api --lib accounts::queries::decode_smoke -- --nocapture
//! ```

use super::*;

#[tokio::test]
async fn account_tx_keyset_decodes_and_pages() {
    let Some(ch) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping account-tx keyset decode smoke");
        return;
    };
    let account: Option<i64> = ch
        .query("SELECT account_id FROM transaction_participants LIMIT 1")
        .fetch_optional()
        .await
        .expect("bootstrap account query must run");
    let Some(account) = account else {
        eprintln!("no participant in this CH — account-tx smoke not exercised");
        return;
    };

    for direction in [Direction::Next, Direction::Prev] {
        let first = fetch_transactions(&ch, account, 2, None, SortOrder::Desc, direction)
            .await
            .unwrap_or_else(|e| panic!("first page failed ({direction:?}): {e}"));
        let Some(last) = first.last() else {
            continue;
        };
        let cursor = TxListCursor::ChPosition {
            ledger_sequence: last.ledger_sequence,
            application_order: last.application_order,
        };
        let second = fetch_transactions(&ch, account, 2, Some(&cursor), SortOrder::Desc, direction)
            .await
            .unwrap_or_else(|e| panic!("cursor page failed ({direction:?}): {e}"));
        assert!(
            second
                .iter()
                .all(|r| (r.ledger_sequence, r.application_order)
                    != (last.ledger_sequence, last.application_order)),
            "the cursor row must not repeat on the next page"
        );
    }
}
