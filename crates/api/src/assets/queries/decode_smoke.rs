//! Live-CH **decode** smoke for the asset-transactions keyset read.
//!
//! The two-arm `UNION ALL` (task 0446) is the class of SQL an offline build
//! cannot validate: the outer `ORDER BY … LIMIT 1 BY … LIMIT` over a
//! parenthesised union parses only against a real server, and the unit tests
//! above assert its shape as a string, not that ClickHouse accepts it. The
//! other read modules guard their generated SQL the same way.
//!
//! Exercises BOTH shapes — the lone arm (no associated contract) and the union
//! (an asset with one) — in both page directions.
//!
//! **Skips cleanly when `CH_URL` is unset**, so CI (no CH access) is green.
//! Run against a reachable CH (local replica or SSH tunnel):
//!
//! ```text
//! CH_URL=http://127.0.0.1:8123 CH_DATABASE=default \
//!   cargo test -p api --lib assets::queries::decode_smoke -- --nocapture
//! ```

use super::*;

/// Bootstrap row: any asset, plus its associated contract surrogate if it
/// has one (ADR 0051 — a type-3 token's own contract, else a SAC wrapper).
#[derive(Debug, Row, Deserialize)]
struct BootAssetRow {
    id: i64,
    contract_surrogate: i64,
}

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
async fn asset_tx_keyset_union_decodes() {
    let Some(ch) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping asset-tx keyset decode smoke");
        return;
    };

    // An asset that HAS an associated contract, so the union arm is real.
    let boot = ch
        .query(
            "SELECT a.id AS id, max(sc.sac_contract_id) AS contract_surrogate \
             FROM assets a INNER JOIN asset_sac sc \
               ON sc.asset_type = a.asset_type AND sc.asset_code = a.asset_code \
              AND sc.issuer_id = a.issuer_id AND sc.contract_id = a.contract_id \
             WHERE sc.sac_contract_id != 0 \
             GROUP BY a.id LIMIT 1",
        )
        .fetch_optional::<BootAssetRow>()
        .await
        .expect("bootstrap asset query must run");

    for direction in [Direction::Next, Direction::Prev] {
        // Lone arm: `None` skips the wrapper entirely.
        fetch_transactions(&ch, 1, None, 21, None, direction)
            .await
            .unwrap_or_else(|e| panic!("lone-arm keyset failed ({direction:?}): {e}"));

        if let Some(b) = &boot {
            fetch_transactions(&ch, b.id, Some(b.contract_surrogate), 21, None, direction)
                .await
                .unwrap_or_else(|e| panic!("union keyset failed ({direction:?}): {e}"));
        }
    }
    if boot.is_none() {
        eprintln!("no SAC-backed asset in this CH — union arm not exercised");
    }
}
