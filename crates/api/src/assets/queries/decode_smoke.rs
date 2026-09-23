//! Live-CH **decode** smoke for the asset-transactions keyset read.
//!
//! The keyset seek, the page fetch by position and the cursor round-trip only
//! parse against a real server; the unit tests cannot see a column that does
//! not exist. Walks two pages in both directions, feeding the first page's
//! boundary row back as the cursor.
//!
//! **Skips cleanly when `CH_URL` is unset**, so CI (no CH access) is green.
//! Run against a reachable CH (local replica or SSH tunnel):
//!
//! ```text
//! CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!   cargo test -p api --lib assets::queries::decode_smoke -- --nocapture
//! ```

use super::*;

/// Task 0485. The ranking that puts native XLM first is a SORT DIRECTION,
/// and a direction is invisible to the SQL-shape tests — they pin the
/// string, not what comes back. This runs the real read and looks at row 1.
#[tokio::test]
async fn code_search_returns_native_first() {
    let Some(ch) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping assets native-first smoke");
        return;
    };
    let has_native: u64 = ch
        .query("SELECT count() FROM assets WHERE asset_type = 0")
        .fetch_one()
        .await
        .expect("native probe must run");
    if has_native == 0 {
        eprintln!("no native row in this CH — native-first smoke not exercised");
        return;
    }

    let mut params = ResolvedListParams {
        limit: 10,
        cursor: None,
        asset_type: None,
        asset_code: Some("xlm".to_string()),
        sac_only: false,
    };
    let page = fetch_list(&ch, &params, Direction::Next)
        .await
        .expect("code-search page decodes");

    let first = page
        .first()
        .expect("a corpus with native XLM cannot be empty");
    assert_eq!(
        first.row.asset_type, 0,
        "`xlm` answered with {:?} first — native XLM is the MINIMUM of the \
         identity 4-tuple, so a DESC walk buries it on the last page",
        first.row.asset_code
    );

    // And with NO filter at all: the asset list of a Stellar explorer
    // opening on anything other than XLM was the same defect wearing a
    // different hat (it opened on codeless Soroban contracts).
    params.asset_code = None;
    let browse = fetch_list(&ch, &params, Direction::Next)
        .await
        .expect("unfiltered page decodes");
    assert_eq!(
        browse.first().map(|r| r.row.asset_type),
        Some(0),
        "the unfiltered list must open on native XLM; got {:?}",
        browse.first().map(|r| r.row.asset_code.clone())
    );
}

#[tokio::test]
async fn asset_tx_keyset_decodes_and_pages() {
    let Some(ch) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping asset-tx keyset decode smoke");
        return;
    };
    let asset: Option<i64> = ch
        .query("SELECT asset_id FROM operation_asset_appearances LIMIT 1")
        .fetch_optional()
        .await
        .expect("bootstrap asset query must run");
    let Some(asset) = asset else {
        eprintln!("no asset appearance in this CH — asset-tx smoke not exercised");
        return;
    };

    for direction in [Direction::Next, Direction::Prev] {
        let first = fetch_transactions(&ch, asset, 2, None, direction)
            .await
            .unwrap_or_else(|e| panic!("first page failed ({direction:?}): {e}"));
        let Some(last) = first.last() else {
            continue;
        };
        let cursor = TxListCursor::ChPosition {
            ledger_sequence: last.ledger_sequence,
            application_order: last.application_order,
        };
        let second = fetch_transactions(&ch, asset, 2, Some(&cursor), direction)
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
