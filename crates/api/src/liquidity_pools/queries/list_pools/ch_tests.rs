//! ClickHouse-backed check of the soroban pool reads (task 0374): the list's
//! activity order, reserves and total shares, and the same values on the
//! single-pool detail.
//!
//! The order key comes from `pool_activity`, which a refreshable MV fills from
//! `pool_state_changes`; the reserves and shares are joined per page. This runs
//! the real `init.sql` and the real list query in a throwaway database, so it
//! can never touch shared data. Gated on `CH_URL` like the other DB-backed
//! tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api list_reads_soroban_order_reserves_and_shares

use super::*;

const DB: &str = "api_test_0374_pool_activity";

// Pool ids as 32-byte hex: `c1` classic, `51` to `53` soroban.
const CLASSIC: &str = "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1";
const SOROBAN_ROUTER: &str = "5151515151515151515151515151515151515151515151515151515151515151";
const SOROBAN_ACTIVE: &str = "5252525252525252525252525252525252525252525252525252525252525252";
const SOROBAN_REPOINTED: &str = "5353535353535353535353535353535353535353535353535353535353535353";

async fn seed(ch: &clickhouse::Client) {
    for sql in [
        // A classic pool's own row moves with every trade: last activity 200.
        // The soroban rows sit at their registration ledgers.
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs, pool_type_raw) VALUES \
             (unhex('{CLASSIC}'), 30, 200, 0, [], ''), \
             (unhex('{SOROBAN_ROUTER}'), 30, 100, 1, [1001, 1002], 'constant'), \
             (unhex('{SOROBAN_ACTIVE}'), 30, 110, 1, [], ''), \
             (unhex('{SOROBAN_REPOINTED}'), 30, 90, 1, [1001, 1002], '')"
        ),
        // Two classic legs, so both scale by the protocol's 7 decimals.
        "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, id) VALUES \
         (0, '', 0, 0, 1001), (1, 'USDC', 42, 0, 1002)"
            .to_string(),
        // The router pool's share token (501) publishes 7 decimals; the
        // pair-factory pool stores 0, which for that family is a measurement.
        // REPOINTED declares plane 9, then re-points to plane 90 at 140
        // without moving its reserves.
        format!(
            "INSERT INTO pool_instance_state (pool_id, plane_id, share_token_id, total_shares, derived_at_ledger) VALUES \
             (unhex('{SOROBAN_ROUTER}'), 7, 501, 252647541418, 100), \
             (unhex('{SOROBAN_ACTIVE}'), 8, 0, 0, 110), \
             (unhex('{SOROBAN_REPOINTED}'), 9, 0, 0, 90), \
             (unhex('{SOROBAN_REPOINTED}'), 90, 0, 0, 140)"
        ),
        "INSERT INTO soroban_contracts (id, contract_id, is_sac) VALUES (501, 'CSHARETOKEN', false)"
            .to_string(),
        "INSERT INTO soroban_contract_metadata (contract_id, decimals, version) VALUES ('CSHARETOKEN', 7, 1)"
            .to_string(),
        // Rows are staged from each pool's own instance, under the plane it
        // declared at that ledger.
        format!(
            "INSERT INTO pool_state_changes (pool_id, ledger_sequence, reserves, plane_id) VALUES \
             (unhex('{SOROBAN_ROUTER}'), 120, [50000000, 60000000], 7), \
             (unhex('{SOROBAN_ROUTER}'), 150, [10000000, 20000000], 7), \
             (unhex('{SOROBAN_ACTIVE}'), 300, [1, 2], 8), \
             (unhex('{SOROBAN_REPOINTED}'), 120, [30000000, 40000000], 9)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }
}

#[tokio::test]
async fn list_reads_soroban_order_reserves_and_shares() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban pool read check");
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
    seed(&ch).await;

    // Run the MV now instead of waiting for its schedule.
    for sql in [
        format!("SYSTEM REFRESH VIEW {DB}.pool_activity_mv"),
        format!("SYSTEM WAIT VIEW {DB}.pool_activity_mv"),
    ] {
        ch.query(&sql)
            .execute()
            .await
            .expect("refresh pool_activity_mv");
    }

    let params = ResolvedPoolListParams {
        limit: 10,
        cursor: None,
        pool_kind: None,
        asset_codes: vec![],
        pool_id_hex: None,
    };
    let rows = fetch_pool_list(&ch, &params, Direction::Next)
        .await
        .expect("list query runs");
    let order: Vec<(&str, i64)> = rows
        .iter()
        .map(|r| (r.pool_id_hex.as_str(), r.cursor_ledger))
        .collect();

    // Soroban pools rank by their last reserve change, the classic one by its
    // live row. REPOINTED falls back to its registration (90): the MV still
    // counts only rows of the currently declared plane (task 0581 rebuilds it).
    assert_eq!(
        order,
        vec![
            (SOROBAN_ACTIVE, 300),
            (CLASSIC, 200),
            (SOROBAN_ROUTER, 150),
            (SOROBAN_REPOINTED, 90)
        ],
        "list must order by last activity"
    );

    let listed = |pool: &str| {
        rows.iter()
            .find(|r| r.pool_id_hex == pool)
            .expect("pool listed")
    };
    let reserves = |row: &PoolRow| -> Vec<Option<String>> {
        row.legs.iter().map(|l| l.reserve.clone()).collect()
    };
    let some = |v: [&str; 2]| v.map(|s| Some(s.to_string())).to_vec();

    // The newest row wins (1, 2 at 7 decimals), not the older 5, 6.
    let router = listed(SOROBAN_ROUTER);
    assert_eq!(reserves(router), some(["1", "2"]));
    // Shares scale by the share token's own decimals; the pair-factory 0 is real.
    assert_eq!(router.total_shares.as_deref(), Some("25264.7541418"));
    assert_eq!(listed(SOROBAN_ACTIVE).total_shares.as_deref(), Some("0"));
    // A pool that re-pointed its plane without moving keeps its reserves: its
    // newest row sits on the old plane, and a filter on the declared plane
    // would read nothing.
    assert_eq!(reserves(listed(SOROBAN_REPOINTED)), some(["3", "4"]));

    // The detail reads the same values through its own statement.
    let detail = crate::liquidity_pools::queries::fetch_pool_by_id(&ch, SOROBAN_ROUTER)
        .await
        .expect("detail query runs")
        .expect("pool found");
    assert_eq!(reserves(&detail), some(["1", "2"]));
    assert_eq!(detail.total_shares.as_deref(), Some("25264.7541418"));

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
