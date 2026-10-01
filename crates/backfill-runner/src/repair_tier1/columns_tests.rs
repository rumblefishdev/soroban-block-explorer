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
use db_clickhouse::persist::rows::SorobanContractRow;

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
async fn soroban_contracts_rebuild_keeps_the_executable_reference() {
    let Some(client) = ch_client().await else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let strkey = "CREPAIRTIER1COLUMNSTESTFLEETMEMBERAAAAAAAAAAAAAAAAAAAAAAA";
    let id = db_clickhouse::persist::ids::contract_id(strkey);
    let cleanup = format!("ALTER TABLE soroban_contracts DELETE WHERE id = {id}");
    delete(&client, &cleanup).await;

    let mut insert = client
        .insert::<SorobanContractRow>("soroban_contracts")
        .await
        .expect("open soroban_contracts insert");
    insert
        .write(&SorobanContractRow {
            id,
            contract_id: strkey.to_string(),
            wasm_hash: None,
            wasm_uploaded_at_ledger: 900,
            deployer_id: Some(7),
            deployed_at_ledger: Some(900),
            contract_type: Some(0),
            is_sac: false,
            executable_owner_id: Some(42),
            executable_tag: Some("fleet-v2".to_string()),
        })
        .await
        .expect("write fleet member");
    insert.end().await.expect("close soroban_contracts insert");

    rebuild_soroban_contracts(&client, /* dry_run */ false)
        .await
        .expect("rebuild_soroban_contracts must succeed");

    let (owner, tag): (Option<i64>, Option<String>) = client
        .query(
            "SELECT executable_owner_id, executable_tag FROM soroban_contracts FINAL WHERE id = ?",
        )
        .bind(id)
        .fetch_one()
        .await
        .expect("read fleet member after rebuild");
    assert_eq!(owner, Some(42), "the rebuild must not reset the owner");
    assert_eq!(
        tag.as_deref(),
        Some("fleet-v2"),
        "the rebuild must not reset the tag"
    );

    delete(&client, &cleanup).await;
}
