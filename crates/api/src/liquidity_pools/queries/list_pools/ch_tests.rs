//! ClickHouse-backed check of the soroban pool reads (task 0374): the list's
//! activity order, reserves and total shares, and the same values on the
//! single-pool detail.
//!
//! The order key comes from `pool_activity`, which a refreshable MV fills from
//! `pool_state_changes`. That MV keeps only rows of the plane the pool itself
//! declares in `pool_instance_state`: a plane entry names its pool in a key any
//! contract can write, so without the filter a foreign contract could push any
//! pool to the top of the list. The filter lives in `init.sql`, where nothing
//! else would notice it going missing — this runs the real `init.sql` and the
//! real list query in a throwaway database, so it can never touch shared data.
//! Gated on `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api list_orders_by_activity_from_the_declared_plane_only

use super::*;

const DB: &str = "api_test_0374_pool_activity";

// Pool ids as 32-byte hex: `c1` classic, `51` to `54` soroban.
const CLASSIC: &str = "c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1";
const SOROBAN_SPOOFED: &str = "5151515151515151515151515151515151515151515151515151515151515151";
const SOROBAN_ACTIVE: &str = "5252525252525252525252525252525252525252525252525252525252525252";
const SOROBAN_LATE: &str = "5353535353535353535353535353535353535353535353535353535353535353";
const SOROBAN_UNREFRESHED: &str =
    "5454545454545454545454545454545454545454545454545454545454545454";

async fn seed(ch: &clickhouse::Client) {
    for sql in [
        // A classic pool's own row moves with every trade: last activity 200.
        // The soroban rows sit at their registration ledgers. LATE and
        // UNREFRESHED register (500, 400) AFTER their last change (120, 50),
        // so the page's activity key is no lower bound on their reserve rows.
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs, pool_type_raw) VALUES \
             (unhex('{CLASSIC}'), 30, 200, 0, [], ''), \
             (unhex('{SOROBAN_SPOOFED}'), 30, 100, 1, [1001, 1002], 'constant'), \
             (unhex('{SOROBAN_ACTIVE}'), 30, 110, 1, [], ''), \
             (unhex('{SOROBAN_LATE}'), 30, 500, 1, [1001, 1002], ''), \
             (unhex('{SOROBAN_UNREFRESHED}'), 30, 400, 1, [1001, 1002], '')"
        ),
        // Two classic legs, so both scale by the protocol's 7 decimals.
        "INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, id) VALUES \
         (0, '', 0, 0, 1001), (1, 'USDC', 42, 0, 1002)"
            .to_string(),
        // Each soroban pool declares its plane (7 and 8) and its shares. The
        // router pool's share token (501) publishes 7 decimals; the
        // pair-factory pool stores 0, which for that family is a measurement.
        format!(
            "INSERT INTO pool_instance_state (pool_id, plane_id, share_token_id, total_shares, derived_at_ledger) VALUES \
             (unhex('{SOROBAN_SPOOFED}'), 7, 501, 252647541418, 100), \
             (unhex('{SOROBAN_ACTIVE}'), 8, 0, 0, 110), \
             (unhex('{SOROBAN_LATE}'), 9, 0, 0, 500), \
             (unhex('{SOROBAN_UNREFRESHED}'), 10, 0, 0, 400)"
        ),
        "INSERT INTO soroban_contracts (id, contract_id, is_sac) VALUES (501, 'CSHARETOKEN', false)"
            .to_string(),
        "INSERT INTO soroban_contract_metadata (contract_id, decimals, version) VALUES ('CSHARETOKEN', 7, 1)"
            .to_string(),
        // The spoofed pool's own plane last moved at 150; a FOREIGN plane (666)
        // publishes rows under its id at 900. The active pool moved at 300.
        format!(
            "INSERT INTO pool_state_changes (pool_id, ledger_sequence, reserves, plane_id) VALUES \
             (unhex('{SOROBAN_SPOOFED}'), 150, [10000000, 20000000], 7), \
             (unhex('{SOROBAN_SPOOFED}'), 900, [999990000000, 1], 666), \
             (unhex('{SOROBAN_ACTIVE}'), 300, [1, 2], 8), \
             (unhex('{SOROBAN_LATE}'), 120, [30000000, 40000000], 9)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }
}

#[tokio::test]
async fn list_orders_by_activity_from_the_declared_plane_only() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping pool activity order check");
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
    // A change the refresh has not reached yet: UNREFRESHED has no
    // `pool_activity` row, so its reserves must still read.
    ch.query(&format!(
        "INSERT INTO pool_state_changes (pool_id, ledger_sequence, reserves, plane_id) VALUES \
         (unhex('{SOROBAN_UNREFRESHED}'), 50, [50000000, 60000000], 10)"
    ))
    .execute()
    .await
    .expect("seed unrefreshed change");

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

    // The foreign plane's 900 must not count: the spoofed pool's activity is its
    // own 150, so it ranks LAST — below the classic pool's live 200.
    assert_eq!(
        order,
        vec![
            (SOROBAN_LATE, 500),
            (SOROBAN_UNREFRESHED, 400),
            (SOROBAN_ACTIVE, 300),
            (CLASSIC, 200),
            (SOROBAN_SPOOFED, 150)
        ],
        "list must order by last activity, counting only the declared plane"
    );

    // The reserves come from the declared plane too: its latest row (1, 2 at
    // 7 decimals), never the foreign plane's newer one.
    let spoofed = rows
        .iter()
        .find(|r| r.pool_id_hex == SOROBAN_SPOOFED)
        .expect("spoofed pool listed");
    let reserves: Vec<Option<&str>> = spoofed.legs.iter().map(|l| l.reserve.as_deref()).collect();
    assert_eq!(reserves, vec![Some("1"), Some("2")]);
    // Shares scale by the share token's own decimals; the pair-factory 0 is real.
    assert_eq!(spoofed.total_shares.as_deref(), Some("25264.7541418"));
    let active = rows
        .iter()
        .find(|r| r.pool_id_hex == SOROBAN_ACTIVE)
        .expect("active pool listed");
    assert_eq!(active.total_shares.as_deref(), Some("0"));

    // The reserve read's lower bound. LATE's latest row (120) is older than
    // every activity key on the page (lowest 150), so a bound on that key hides
    // it; UNREFRESHED has no `pool_activity` entry, which must lift the bound.
    for (pool, want) in [
        (SOROBAN_LATE, [Some("3"), Some("4")]),
        (SOROBAN_UNREFRESHED, [Some("5"), Some("6")]),
    ] {
        let row = rows
            .iter()
            .find(|r| r.pool_id_hex == pool)
            .expect("pool listed");
        let reserves: Vec<Option<&str>> = row.legs.iter().map(|l| l.reserve.as_deref()).collect();
        assert_eq!(reserves, want, "reserves of {pool}");
    }

    // The detail reads the same values through its own statement (8 binds).
    let detail = crate::liquidity_pools::queries::fetch_pool_by_id(&ch, SOROBAN_SPOOFED)
        .await
        .expect("detail query runs")
        .expect("pool found");
    let reserves: Vec<Option<&str>> = detail.legs.iter().map(|l| l.reserve.as_deref()).collect();
    assert_eq!(reserves, vec![Some("1"), Some("2")]);
    assert_eq!(detail.total_shares.as_deref(), Some("25264.7541418"));

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
