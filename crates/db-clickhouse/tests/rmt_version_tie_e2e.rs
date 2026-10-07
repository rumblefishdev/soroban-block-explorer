//! The ReplacingMergeTree tie rule the checkpoint seed relies on (task 0629):
//! two rows with one key and one version keep the most recently inserted, on
//! a `FINAL` read and after a merge; a newer version inserted earlier still
//! beats an older one inserted later. `account_entry_state` is repaired by a
//! seed row at the SAME version as a row written before the sponsorship
//! counters existed — if a ClickHouse upgrade changed this rule, those
//! accounts would silently read 0 again.
//!
//! Gated on `CLICKHOUSE_URL`: skipped cleanly when unset.

use db_clickhouse::{Config, client};

const TABLE: &str = "rmt_version_tie_e2e";

#[tokio::test]
async fn equal_version_keeps_the_later_insert_newer_version_still_wins() {
    let Ok(url) = std::env::var("CLICKHOUSE_URL") else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let cfg = Config {
        url,
        ..Config::from_env()
    };
    let ch = client(&cfg);
    let run = |sql: String| {
        let ch = ch.clone();
        async move { ch.query(&sql).execute().await.expect(&sql) }
    };

    run(format!("DROP TABLE IF EXISTS {TABLE}")).await;
    run(format!(
        "CREATE TABLE {TABLE} (account_id Int64, num_sponsored UInt32 DEFAULT 0, \
         last_updated_ledger Int64) \
         ENGINE = ReplacingMergeTree(last_updated_ledger) ORDER BY account_id"
    ))
    .await;
    // Each INSERT is its own part, so the tie is between parts, as in production.
    run(format!("INSERT INTO {TABLE} VALUES (1, 0, 64800534)")).await; // pre-column row
    run(format!("INSERT INTO {TABLE} VALUES (2, 9, 64900000)")).await; // newer live row
    run(format!("INSERT INTO {TABLE} VALUES (1, 3, 64800534)")).await; // seed, same version
    run(format!("INSERT INTO {TABLE} VALUES (2, 3, 64800534)")).await; // seed, older version

    let read = || async {
        ch.query(&format!(
            "SELECT account_id, num_sponsored FROM {TABLE} FINAL ORDER BY account_id"
        ))
        .fetch_all::<(i64, u32)>()
        .await
        .expect("read")
    };
    assert_eq!(read().await, [(1, 3), (2, 9)], "FINAL on unmerged parts");

    run(format!("OPTIMIZE TABLE {TABLE} FINAL")).await;
    assert_eq!(read().await, [(1, 3), (2, 9)], "after the merge");

    run(format!("DROP TABLE {TABLE}")).await;
}
