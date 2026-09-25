//! ClickHouse-backed check that a soroban pool's list and detail rows carry
//! its reserves (task 0374).
//!
//! A soroban pool writes no `liquidity_pool_snapshots` row, so both reads used
//! to leave every leg's reserve `null`. They now take the newest
//! `pool_state_changes` row and scale the legs whose scale is a fact. This runs
//! the real `init.sql` and the real list and detail queries in a throwaway
//! database. Gated on `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_pool_reads_serve_leg_reserves

use crate::common::cursor::Direction;
use crate::liquidity_pools::queries::{ResolvedPoolListParams, fetch_pool_by_id, fetch_pool_list};

const DB: &str = "api_test_0374_soroban_reserves";

const POOL: &str = "5454545454545454545454545454545454545454545454545454545454545454";
const ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

#[tokio::test]
async fn soroban_pool_reads_serve_leg_reserves() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban reserves check");
        return;
    };
    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop leftover throwaway db");
    base.query(&format!("CREATE DATABASE {DB}"))
        .execute()
        .await
        .expect("create throwaway db");
    let ch = base.clone().with_database(DB);
    db_clickhouse::apply_init_sql(&ch)
        .await
        .expect("apply init.sql");

    // Three legs, in the pool's token order: native XLM (101), a classic USDC
    // (102) and a soroban token (103). Two state rows; the newer one wins.
    for sql in [
        "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, id) VALUES \
         (0, '', 0, 0, 101), (1, 'USDC', 7, 0, 102), (3, '', 0, 103, 103)"
            .to_string(),
        format!(
            "INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number) VALUES \
             (7, '{ISSUER}', 1, 1, 1)"
        ),
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs) VALUES \
             (unhex('{POOL}'), 10, 100, 1, [101, 102, 103])"
        ),
        format!(
            "INSERT INTO pool_state_changes (pool_id, ledger_sequence, reserves, plane_id) VALUES \
             (unhex('{POOL}'), 150, [1, 2, 3], 9), \
             (unhex('{POOL}'), 200, [31072879007206, 125000000, 999], 9)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let expected = vec![
        Some("3107287.9007206".to_string()), // XLM, newest row, scaled by 7
        Some("12.5".to_string()),            // classic USDC, scaled by 7
        None,                                // soroban token: scale not read here
    ];

    let detail = fetch_pool_by_id(&ch, POOL)
        .await
        .expect("detail query runs")
        .expect("pool exists");
    let detail_reserves: Vec<Option<String>> =
        detail.legs.iter().map(|l| l.reserve.clone()).collect();
    assert_eq!(detail_reserves, expected, "detail legs");

    let params = ResolvedPoolListParams {
        limit: 10,
        cursor: None,
        pool_kind: None,
        asset_codes: vec![],
        pool_id_hex: None,
    };
    let list = fetch_pool_list(&ch, &params, Direction::Next)
        .await
        .expect("list query runs");
    let row = list
        .iter()
        .find(|r| r.pool_id_hex == POOL)
        .expect("pool is listed");
    let list_reserves: Vec<Option<String>> = row.legs.iter().map(|l| l.reserve.clone()).collect();
    assert_eq!(list_reserves, expected, "list legs");

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
