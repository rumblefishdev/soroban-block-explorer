//! ClickHouse-backed check of the detail's `created_at_ledger` and
//! `participant_count` (task 0374).
//!
//! The value is the pool's first snapshot ledger, falling back to its own row
//! when it has none. A soroban pool never has a snapshot, and plain `min` over
//! zero rows is `0` rather than NULL, so every soroban detail read ledger 0.
//! This runs the real `init.sql` and the real query in a throwaway database.
//! Gated on `CH_URL` like the other DB-backed tests in this crate.
//!
//!   CH_URL=http://localhost:8123 CH_USER=default CH_PASSWORD=… \
//!     cargo test -p api detail_created_at_falls_back_without_a_snapshot

use super::*;

const DB: &str = "api_test_0374_detail_created_at";

const CLASSIC: &str = "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3";
const SOROBAN: &str = "5353535353535353535353535353535353535353535353535353535353535353";

#[tokio::test]
async fn detail_created_at_falls_back_without_a_snapshot() {
    let Some(base) = crate::common::ch::test_client_from_env() else {
        eprintln!("CH_URL unset — skipping detail created_at check");
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

    // The classic pool's first snapshot is at 1,000 (its row moved on to 5,000);
    // the soroban pool registered at 60,059,011 and has no snapshot at all.
    for sql in [
        format!(
            "INSERT INTO liquidity_pools (pool_id, fee_bps, last_updated_ledger, pool_kind) VALUES \
             (unhex('{CLASSIC}'), 30, 5000, 0), \
             (unhex('{SOROBAN}'), 10, 60059011, 1)"
        ),
        format!(
            "INSERT INTO liquidity_pool_snapshots (pool_id, ledger_sequence, reserve_a, reserve_b, total_shares) VALUES \
             (unhex('{CLASSIC}'), 1000, 1, 1, 1), \
             (unhex('{CLASSIC}'), 5000, 2, 2, 2)"
        ),
        // One open and one closed classic position. A row under the soroban
        // pool must not count: its providers are share-token holders, which
        // the handler reads, never `lp_positions`.
        format!(
            "INSERT INTO lp_positions (pool_id, account_id, shares, first_deposit_ledger, last_updated_ledger) VALUES \
             (unhex('{CLASSIC}'), 1, 5, 1000, 1000), \
             (unhex('{CLASSIC}'), 2, 0, 1000, 2000), \
             (unhex('{SOROBAN}'), 3, 5, 1000, 1000)"
        ),
    ] {
        ch.query(&sql).execute().await.expect("seed rows");
    }

    let created = |hex: &'static str| {
        let ch = ch.clone();
        async move {
            fetch_pool_by_id(&ch, hex)
                .await
                .expect("detail query runs")
                .expect("pool exists")
                .created_at_ledger
        }
    };
    assert_eq!(created(CLASSIC).await, 1000, "first snapshot ledger");
    assert_eq!(
        created(SOROBAN).await,
        60_059_011,
        "a pool with no snapshot falls back to its own ledger, not 0"
    );

    let participants = |hex: &'static str| {
        let ch = ch.clone();
        async move {
            fetch_pool_by_id(&ch, hex)
                .await
                .expect("detail query runs")
                .expect("pool exists")
                .participant_count
        }
    };
    assert_eq!(participants(CLASSIC).await, Some(1), "open positions only");
    assert_eq!(
        participants(SOROBAN).await,
        None,
        "a soroban pool is not counted from lp_positions"
    );

    base.query(&format!("DROP DATABASE IF EXISTS {DB}"))
        .execute()
        .await
        .expect("drop throwaway db");
}
