//! ClickHouse-backed check of a soroban pool's served total shares (task
//! 0374), through the real list and detail queries in a throwaway database.
//! Gated on `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_total_shares_follow_the_measured_rule

use crate::common::cursor::Direction;
use crate::liquidity_pools::queries::{ResolvedPoolListParams, fetch_pool_by_id, fetch_pool_list};

const DB: &str = "api_test_0374_soroban_total_shares";

// Four soroban pools, one per case.
const POSITIVE: &str = "6161616161616161616161616161616161616161616161616161616161616161";
const EMPTY: &str = "6262626262626262626262626262626262626262626262626262626262626262";
const NO_KEY: &str = "6363636363636363636363636363636363636363636363636363636363636363";
const NO_SCALE: &str = "6464646464646464646464646464646464646464646464646464646464646464";

#[tokio::test]
async fn soroban_total_shares_follow_the_measured_rule() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban total shares check");
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

    for sql in [
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs) VALUES \
             (unhex('{POSITIVE}'), 30, 100, 1, [101, 102]), \
             (unhex('{EMPTY}'), 30, 100, 1, [101, 102]), \
             (unhex('{NO_KEY}'), 30, 100, 1, [101, 102]), \
             (unhex('{NO_SCALE}'), 30, 100, 1, [101, 102])"
        ),
        format!(
            "INSERT INTO pool_state_changes (pool_id, ledger_sequence, reserves, plane_id) VALUES \
             (unhex('{POSITIVE}'), 200, [5, 9], 1), \
             (unhex('{EMPTY}'), 200, [0, 0], 1), \
             (unhex('{NO_KEY}'), 200, [7, 8], 1), \
             (unhex('{NO_SCALE}'), 200, [3, 4], 1)"
        ),
        // Share tokens 901 (published decimals) and 904 (no metadata row). The
        // older positive row must lose to the newer one.
        format!(
            "INSERT INTO pool_instance_state (pool_id, plane_id, share_token_id, total_shares, derived_at_ledger) VALUES \
             (unhex('{POSITIVE}'), 1, 901, 1, 150), \
             (unhex('{POSITIVE}'), 1, 901, 1215785565496, 200), \
             (unhex('{EMPTY}'), 1, 901, 0, 200), \
             (unhex('{NO_KEY}'), 1, 0, 0, 200), \
             (unhex('{NO_SCALE}'), 1, 904, 4622, 200)"
        ),
        "INSERT INTO soroban_contracts (id, contract_id, is_sac) VALUES \
         (901, 'CSHARETOKEN901', false), (904, 'CSHARETOKEN904', false)"
            .to_string(),
        "INSERT INTO soroban_contract_metadata (contract_id, decimals, version) VALUES \
         ('CSHARETOKEN901', 7, 1)"
            .to_string(),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let expected = [
        (POSITIVE, Some("121578.5565496")),
        (EMPTY, Some("0")),
        (NO_KEY, None),
        (NO_SCALE, None),
    ];

    for (pool, want) in expected {
        let detail = fetch_pool_by_id(&ch, pool)
            .await
            .expect("detail query runs")
            .expect("pool exists");
        assert_eq!(detail.total_shares.as_deref(), want, "detail {pool}");
    }

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
    for (pool, want) in expected {
        let row = list
            .iter()
            .find(|r| r.pool_id_hex == pool)
            .expect("pool is listed");
        assert_eq!(row.total_shares.as_deref(), want, "list {pool}");
    }

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
