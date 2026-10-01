//! A rebuild must carry every column of the live table through to the swap,
//! not only the ones it repairs. A hand-typed INSERT column list did not: the
//! staging table is a copy of the live one, so columns added after the list
//! was written were filled from their DEFAULT and the EXCHANGE replaced the
//! live values — `soroban_contracts.executable_owner_id` / `executable_tag`
//! (task 0548) and `lp_positions.closed_at_ledger` (ADR 0055).
//!
//! Real runs (staging built AND exchanged), so run against a throwaway
//! database, `--test-threads=1`. Skipped without `CLICKHOUSE_URL`.

use super::*;
use db_clickhouse::persist::rows::LpPositionRow;

async fn ch_client() -> Option<ClickhouseClient> {
    let url = std::env::var("CLICKHOUSE_URL").ok()?;
    let cfg = db_clickhouse::Config {
        url,
        ..db_clickhouse::Config::from_env()
    };
    let client = db_clickhouse::client(&cfg);
    if let Err(err) = db_clickhouse::apply_init_sql(&client).await {
        eprintln!("CLICKHOUSE_URL set but apply_init_sql failed ({err}) — skipping");
        return None;
    }
    Some(client)
}

async fn delete(client: &ClickhouseClient, sql: &str) {
    let _ = client
        .query(sql)
        .with_setting("mutations_sync", "1")
        .execute()
        .await;
}

#[tokio::test]
async fn lp_positions_rebuild_keeps_closed_at_ledger() {
    let Some(client) = ch_client().await else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let pool_id = [0xa5u8; 32];
    let account_id = 4_000_021_001_i64;
    let cleanup = format!("ALTER TABLE lp_positions DELETE WHERE account_id = {account_id}");
    delete(&client, &cleanup).await;

    let mut insert = client
        .insert::<LpPositionRow>("lp_positions")
        .await
        .expect("open lp_positions insert");
    insert
        .write(&LpPositionRow {
            pool_id,
            account_id,
            shares: 0,
            first_deposit_ledger: 100,
            last_updated_ledger: 200,
            closed_at_ledger: 200,
        })
        .await
        .expect("write closed position");
    insert.end().await.expect("close lp_positions insert");

    rebuild_lp_positions(&client, /* dry_run */ false)
        .await
        .expect("rebuild_lp_positions must succeed");

    let closed_at: i64 = client
        .query("SELECT closed_at_ledger FROM lp_positions FINAL WHERE account_id = ?")
        .bind(account_id)
        .fetch_one()
        .await
        .expect("read position after rebuild");
    assert_eq!(
        closed_at, 200,
        "the rebuild must not reopen a closed position"
    );

    delete(&client, &cleanup).await;
}
