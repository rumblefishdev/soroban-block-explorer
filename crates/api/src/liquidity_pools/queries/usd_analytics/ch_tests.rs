//! ClickHouse-backed check of a soroban pool's 24h volume per traded leg
//! (task 0374): the real `init.sql` and the real query in a throwaway
//! database. Needs no prices. Gated on `CH_URL` like the other DB-backed
//! tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_volume_24h

use super::fetch_pool_volume_24h;

const DB: &str = "api_test_0374_soroban_volume";
const POOL: &str = "5858585858585858585858585858585858585858585858585858585858585858";
/// A three-leg pool: legs 101 (A), 103 (B), 104 (C).
const POOL3: &str = "5757575757575757575757575757575757575757575757575757575757575757";
/// A registered pool with no trades.
const IDLE: &str = "5656565656565656565656565656565656565656565656565656565656565656";

/// Leg A is asset 101 (7 decimals), leg B 102. In the last 24 h:
/// - a trade selling 2.5 A, written twice (live writer and backfill);
/// - a trade buying 1.5 A;
/// - a deposit of 100 A, which is not volume.
/// A trade two days back is outside the window.
///
/// The three-leg pool trades 3 B for C (never touching leg A) and 1 A for C.
#[tokio::test]
async fn soroban_volume_24h() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban volume check");
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
    ch.query("SYSTEM STOP MERGES pool_movements")
        .execute()
        .await
        .expect("stop merges");

    let sell = format!(
        "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
         (unhex('{POOL}'), 1000, 1, 0, 0, 0, 101, -25000000), \
         (unhex('{POOL}'), 1000, 1, 0, 0, 0, 102, 7)"
    );
    for sql in [
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs) VALUES \
             (unhex('{POOL}'), 30, 1, 1, [101, 102]), \
             (unhex('{POOL3}'), 30, 1, 1, [101, 103, 104]), \
             (unhex('{IDLE}'), 30, 1, 1, [101, 102])"
        ),
        format!(
            "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
             (unhex('{POOL3}'), 1000, 3, 0, 0, 0, 103, 30000000), (unhex('{POOL3}'), 1000, 3, 0, 0, 0, 104, -29000000), \
             (unhex('{POOL3}'), 1001, 3, 0, 0, 0, 101, -10000000), (unhex('{POOL3}'), 1001, 3, 0, 0, 0, 104, 11000000)"
        ),
        "INSERT INTO ledgers (sequence, closed_at) VALUES \
         (900, now() - INTERVAL 2 DAY), (1000, now() - INTERVAL 2 HOUR), (1001, now() - INTERVAL 1 HOUR)"
            .to_string(),
        sell.clone(),
        sell,
        format!(
            "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
             (unhex('{POOL}'), 1001, 1, 0, 0, 0, 101, 15000000), \
             (unhex('{POOL}'), 1001, 1, 0, 0, 0, 102, -5), \
             (unhex('{POOL}'), 1001, 2, 0, 0, 1, 101, 1000000000), \
             (unhex('{POOL}'), 900, 1, 0, 0, 0, 101, 990000000)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let soroban = domain::PoolKind::Soroban;
    let units = fetch_pool_volume_24h(&ch, POOL, soroban, &[Some(7), Some(7)])
        .await
        .expect("volume query runs");
    assert_eq!(
        units,
        Some(vec![(0, 4.0)]),
        "2.5 sold + 1.5 bought, once each, on leg A"
    );

    // A leg A that publishes no decimals has no honest volume.
    let unknown = fetch_pool_volume_24h(&ch, POOL, soroban, &[None, Some(7)])
        .await
        .expect("volume query runs");
    assert_eq!(unknown, None);

    // Each trade of the three-leg pool on its lowest leg: 1 A, and 3 B for
    // the trade that never touches A.
    let three = fetch_pool_volume_24h(&ch, POOL3, soroban, &[Some(7); 3])
        .await
        .expect("volume query runs");
    assert_eq!(three, Some(vec![(0, 1.0), (1, 3.0)]));

    // A pool with no trades in the window is a genuine zero, still in leg A.
    let idle = fetch_pool_volume_24h(&ch, IDLE, soroban, &[Some(7); 2])
        .await
        .expect("volume query runs");
    assert_eq!(idle, Some(vec![(0, 0.0)]));

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
