//! ClickHouse-backed check of a soroban pool's 24h volume per traded leg
//! (task 0374): the real `init.sql` and the real query in a throwaway
//! database. Needs no prices. Gated on `CH_URL` like the other DB-backed
//! tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_volume_24h

use super::{LegVolume, fetch_last_closes, fetch_pool_volume_24h, price_leg};

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
        Some(vec![LegVolume { leg: 0, units: 4.0 }]),
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
    assert_eq!(
        three,
        Some(vec![
            LegVolume { leg: 0, units: 1.0 },
            LegVolume { leg: 1, units: 3.0 },
        ])
    );

    // A pool with no trades in the window is a genuine zero, still in leg A.
    let idle = fetch_pool_volume_24h(&ch, IDLE, soroban, &[Some(7); 2])
        .await
        .expect("volume query runs");
    assert_eq!(idle, Some(vec![LegVolume { leg: 0, units: 0.0 }]));

    // An idle pool whose leg A publishes no decimals: no honest zero either,
    // exactly as before multi-leg pools were priced.
    let idle_unscaled = fetch_pool_volume_24h(&ch, IDLE, soroban, &[None, Some(7)])
        .await
        .expect("volume query runs");
    assert_eq!(idle_unscaled, None);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}

/// A Soroban token's last hourly close is found by its contract address
/// (task 0615): the prices views key it as ('contract', '', '', C…), so two
/// tokens differ only in that column. The prices series is a view on
/// production; a test ClickHouse without one gets a plain table of the same
/// key columns, created here and dropped after.
///
///   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
///     cargo test -p api soroban_token_close_by_contract
#[tokio::test]
async fn soroban_token_close_by_contract() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban token price check");
        return;
    };
    let engine: Vec<String> = base
        .query("SELECT engine FROM system.tables WHERE database = 'prices' AND name = 'price_usd_series_1h'")
        .fetch_all()
        .await
        .expect("read system.tables");
    if engine.iter().any(|e| e != "MergeTree") {
        eprintln!("prices.price_usd_series_1h is a real view here — skipping");
        return;
    }
    let created = engine.is_empty();
    for sql in [
        "CREATE DATABASE IF NOT EXISTS prices",
        "CREATE TABLE IF NOT EXISTS prices.price_usd_series_1h (asset_kind String, asset_code String, \
         issuer_address String, contract_address String, bucket DateTime, close_usd Decimal(38, 14)) \
         ENGINE = MergeTree ORDER BY (asset_kind, contract_address, bucket)",
    ] {
        base.query(sql).execute().await.expect("prices table");
    }
    // Two Soroban tokens two hours back: only the contract tells them apart.
    const XRP: &str = "CB7OOP3VSAWBZOOTOG2YEFANVU45GVWYUUM5HI32DKLHVKUDOFVQ37XP";
    const OTHER: &str = "CBIJBDNZNF4X35BJ4FFZWCDBSCKOP5NB4PLG4SNENRMLAPYG4P5FM6VN";
    base.query(&format!(
        "INSERT INTO prices.price_usd_series_1h \
         (asset_kind, asset_code, issuer_address, contract_address, bucket, close_usd) VALUES \
         ('contract', '', '', '{XRP}', toStartOfHour(now()) - INTERVAL 2 HOUR, 1.25), \
         ('contract', '', '', '{OTHER}', toStartOfHour(now()) - INTERVAL 2 HOUR, 80000)"
    ))
    .execute()
    .await
    .expect("seed prices");

    let xrp = price_leg(3, Some("XRP"), None, Some(XRP));
    let closes = fetch_last_closes(&base, &[&xrp])
        .await
        .expect("last closes");

    assert_eq!(closes.get(&xrp), Some(&1.25));

    if created {
        base.query("DROP TABLE IF EXISTS prices.price_usd_series_1h")
            .execute()
            .await
            .expect("drop test prices table");
    } else {
        base.query(&format!(
            "ALTER TABLE prices.price_usd_series_1h DELETE WHERE contract_address IN ('{XRP}', '{OTHER}')"
        ))
        .execute()
        .await
        .expect("remove test prices rows");
    }
}
