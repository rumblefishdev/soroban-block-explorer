//! ClickHouse-backed check of a soroban pool's 24h volume in leg-A units
//! (task 0374): the real `init.sql` and the real query in a throwaway
//! database. Needs no prices. Gated on `CH_URL` like the other DB-backed
//! tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_volume_24h

use super::fetch_pool_volume_24h;

const DB: &str = "api_test_0374_soroban_volume";
const POOL: &str = "5858585858585858585858585858585858585858585858585858585858585858";

/// Leg A is asset 101 (7 decimals), leg B 102. In the last 24 h:
/// - a trade selling 2.5 A, written twice (live writer and backfill);
/// - a trade buying 1.5 A;
/// - a deposit of 100 A, which is not volume.
/// A trade two days back is outside the window.
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
             (unhex('{POOL}'), 30, 1, 1, [101, 102])"
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

    let units = fetch_pool_volume_24h(&ch, POOL, domain::PoolKind::Soroban, Some(7))
        .await
        .expect("volume query runs");
    assert_eq!(units, Some(4.0), "2.5 sold + 1.5 bought, once each");

    // A leg A that publishes no decimals has no honest volume.
    let unknown = fetch_pool_volume_24h(&ch, POOL, domain::PoolKind::Soroban, None)
        .await
        .expect("volume query runs");
    assert_eq!(unknown, None);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
