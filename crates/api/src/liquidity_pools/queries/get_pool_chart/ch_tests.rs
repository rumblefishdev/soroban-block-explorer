//! ClickHouse-backed check of a soroban pool's chart volume (task 0374): the
//! real `init.sql`, the real query, a throwaway database. The price series
//! is a `prices` view on production; a test ClickHouse without one gets a
//! plain table of the same columns, created here and dropped after. Gated on
//! `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api soroban_volume_series

use chrono::{Duration, Utc};

use super::fetch_soroban_volume_series;
use crate::liquidity_pools::queries::usd_analytics::{
    PoolChartContext, PoolPriceContext, PriceLeg,
};

const DB: &str = "api_test_0374_soroban_volume_series";
const POOL: &str = "5959595959595959595959595959595959595959595959595959595959595959";
/// A three-leg pool: legs 101 (A), 103 (B) and 104 (C, no price).
const POOL3: &str = "5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";
/// Leg A's price identity: a code no real feed uses, so the rows this test
/// adds to a shared `prices` table cannot be confused with anything.
const CODE: &str = "TST0374";
/// The three-leg pool's leg B, priced at 2.0 on day A.
const CODE_B: &str = "TST0374B";
const ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

/// Day A (six days back) has a leg-A close of 0.5; day B (two days back) has
/// none within the 48 h carry. On day A: a trade selling 2 A, written twice;
/// a trade buying 1 A; a deposit of 100 A. On day B: a trade of 4 A.
///
/// The three-leg pool trades on day A: 3 B for C (a trade that never touches
/// leg A), 1 A for C, and 2 A for B — each counted once, on its lowest leg.
#[tokio::test]
async fn soroban_volume_series() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping soroban volume series check");
        return;
    };
    let price_engine: Vec<String> = base
        .query("SELECT engine FROM system.tables WHERE database = 'prices' AND name = 'price_usd_series'")
        .fetch_all()
        .await
        .expect("read system.tables");
    if price_engine.iter().any(|e| e != "MergeTree") {
        eprintln!("prices.price_usd_series is a real view here — skipping");
        return;
    }
    let created_prices = price_engine.is_empty();
    for sql in [
        "CREATE DATABASE IF NOT EXISTS prices",
        "CREATE TABLE IF NOT EXISTS prices.price_usd_series (asset_kind String, asset_code String, \
         issuer_address String, bucket DateTime, close_usd Decimal(38, 14)) \
         ENGINE = MergeTree ORDER BY (asset_kind, asset_code, issuer_address, bucket)",
    ] {
        base.query(sql).execute().await.expect("prices table");
    }
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

    let day_a = "toStartOfDay(now()) - INTERVAL 6 DAY";
    let day_b = "toStartOfDay(now()) - INTERVAL 2 DAY";
    let sell = format!(
        "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
         (unhex('{POOL}'), 100, 1, 0, 0, 0, 101, -20000000), (unhex('{POOL}'), 100, 1, 0, 0, 0, 102, 9)"
    );
    for sql in [
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind, legs) VALUES \
             (unhex('{POOL}'), 30, 1, 1, [101, 102]), \
             (unhex('{POOL3}'), 30, 1, 1, [101, 103, 104])"
        ),
        format!(
            "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
             (unhex('{POOL3}'), 100, 3, 0, 0, 0, 103, 30000000), (unhex('{POOL3}'), 100, 3, 0, 0, 0, 104, -29000000), \
             (unhex('{POOL3}'), 101, 3, 0, 0, 0, 101, -10000000), (unhex('{POOL3}'), 101, 3, 0, 0, 0, 104, 11000000), \
             (unhex('{POOL3}'), 101, 4, 0, 0, 0, 101, 20000000), (unhex('{POOL3}'), 101, 4, 0, 0, 0, 103, -900000)"
        ),
        format!(
            "INSERT INTO ledgers (sequence, closed_at) VALUES \
             (100, {day_a} + INTERVAL 12 HOUR), (101, {day_a} + INTERVAL 13 HOUR), \
             (200, {day_b} + INTERVAL 12 HOUR)"
        ),
        sell.clone(),
        sell,
        format!(
            "INSERT INTO pool_movements (pool_id, ledger_sequence, application_order, operation_index, event_index, event_kind, asset_id, amount) VALUES \
             (unhex('{POOL}'), 101, 1, 0, 0, 0, 101, 10000000), (unhex('{POOL}'), 101, 1, 0, 0, 0, 102, -4), \
             (unhex('{POOL}'), 101, 2, 0, 0, 1, 101, 1000000000), \
             (unhex('{POOL}'), 200, 1, 0, 0, 0, 101, 40000000)"
        ),
        format!(
            "INSERT INTO prices.price_usd_series VALUES ('credit', '{CODE}', '{ISSUER}', {day_a}, 0.5), \
             ('credit', '{CODE_B}', '{ISSUER}', {day_a}, 2.0)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let leg_a = PriceLeg {
        kind: "credit",
        code: CODE.to_string(),
        issuer: ISSUER.to_string(),
    };
    let ctx = |legs: Vec<PriceLeg>, decimals: Vec<Option<u32>>| PoolChartContext {
        price: PoolPriceContext { legs, fee_bps: 30 },
        pool_kind: domain::PoolKind::Soroban,
        leg_decimals: decimals,
    };
    let xlm = PriceLeg {
        kind: "native",
        code: "XLM".to_string(),
        issuer: String::new(),
    };
    let (from, to) = (Utc::now() - Duration::days(10), Utc::now());

    let mut buckets = fetch_soroban_volume_series(
        &ch,
        POOL,
        &ctx(vec![leg_a.clone(), xlm.clone()], vec![Some(7), Some(7)]),
        "1d",
        from,
        to,
    )
    .await
    .expect("volume series runs");
    buckets.sort_by_key(|b| b.bucket_ms);
    let volumes: Vec<Option<f64>> = buckets.iter().map(|b| b.volume).collect();
    // Day A: (2 + 1) × 0.5, the duplicate once and the deposit not at all.
    // Day B: a trade with no price in reach — a hole, never a partial sum.
    assert_eq!(volumes, vec![Some(1.5), None]);

    // A three-leg pool: 3 B × 2.0 for the trade that never touches leg A, then
    // (1 + 2) A × 0.5 — the A-for-B trade counted on A only, once.
    let leg_b = PriceLeg {
        kind: "credit",
        code: CODE_B.to_string(),
        issuer: ISSUER.to_string(),
    };
    let three = ctx(
        vec![leg_a.clone(), leg_b.clone(), xlm.clone()],
        vec![Some(7); 3],
    );
    let buckets = fetch_soroban_volume_series(&ch, POOL3, &three, "1d", from, to)
        .await
        .expect("volume series runs");
    let volumes: Vec<Option<f64>> = buckets.iter().map(|b| b.volume).collect();
    assert_eq!(volumes, vec![Some(7.5)]);

    // A traded leg without published decimals has no honest volume: the
    // bucket is a hole, never a partial sum.
    let b_unscaled = ctx(
        vec![leg_a.clone(), leg_b, xlm],
        vec![Some(7), None, Some(7)],
    );
    let buckets = fetch_soroban_volume_series(&ch, POOL3, &b_unscaled, "1d", from, to)
        .await
        .expect("volume series runs");
    let volumes: Vec<Option<f64>> = buckets.iter().map(|b| b.volume).collect();
    assert_eq!(volumes, vec![None]);
    let no_scale = ctx(
        vec![
            leg_a,
            PriceLeg {
                kind: "",
                code: String::new(),
                issuer: String::new(),
            },
        ],
        vec![None, Some(7)],
    );
    let mut buckets = fetch_soroban_volume_series(&ch, POOL, &no_scale, "1d", from, to)
        .await
        .expect("volume series runs");
    buckets.sort_by_key(|b| b.bucket_ms);
    let volumes: Vec<Option<f64>> = buckets.iter().map(|b| b.volume).collect();
    assert_eq!(volumes, vec![None, None]);

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
    if created_prices {
        base.query("DROP TABLE IF EXISTS prices.price_usd_series")
            .execute()
            .await
            .expect("drop test prices table");
    } else {
        base.query(&format!(
            "ALTER TABLE prices.price_usd_series DELETE WHERE asset_code IN ('{CODE}', '{CODE_B}')"
        ))
        .execute()
        .await
        .expect("remove test prices rows");
    }
}
