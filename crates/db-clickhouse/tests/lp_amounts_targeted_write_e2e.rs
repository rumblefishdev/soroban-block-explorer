//! The 0279 targeted write must persist `lp_operation_amounts` and NOTHING
//! else.
//!
//! That promise is what keeps the historical re-parse additive: a run that
//! also re-emitted the other tables would rewrite the 12 Tier-1 columns which
//! cannot survive parallel `ReplacingMergeTree` collapse, and would owe a
//! `repair-tier1` pass afterwards (`docs/backfills.md` §3). It is also silent
//! when broken — the extra rows are valid, they just quietly re-arm that
//! obligation — so it gets an assertion rather than a comment.
//!
//! Gated on `CLICKHOUSE_URL`, like every other CH test here: skipped cleanly
//! when no instance is reachable.
//!
//! ```bash
//! CLICKHOUSE_URL=http://localhost:8123 \
//!     cargo test -p db-clickhouse --test lp_amounts_targeted_write_e2e
//! ```

use db_clickhouse::persist::PartitionWriter;
use db_clickhouse::persist::rows::{LedgerRow, LpOperationAmountRow};
use db_clickhouse::persist::stage::StagedLedger;
use db_clickhouse::{Config, apply_init_sql, client};

/// Out-of-band sentinel, same convention as `smoke.rs`.
const TEST_LEDGER: i64 = 99_999_301;

#[tokio::test]
async fn targeted_write_persists_only_lp_operation_amounts() {
    let Some(url) = std::env::var("CLICKHOUSE_URL").ok() else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let cfg = Config {
        url,
        ..Config::from_env()
    };
    let ch = client(&cfg);
    apply_init_sql(&ch).await.expect("apply init.sql");

    for table in ["lp_operation_amounts", "ledgers"] {
        ch.query(&format!(
            "ALTER TABLE {table} DELETE WHERE {} = ?",
            if table == "ledgers" {
                "sequence"
            } else {
                "ledger_sequence"
            }
        ))
        .bind(TEST_LEDGER)
        .with_setting("mutations_sync", "1")
        .execute()
        .await
        .expect("cleanup");
    }

    // A staged ledger carrying BOTH kinds of row: the amounts we want and a
    // `ledgers` commit marker we must not get.
    let staged = StagedLedger {
        ledger_sequence: TEST_LEDGER,
        ledger_rows: vec![LedgerRow {
            sequence: TEST_LEDGER,
            hash: [0x7d; 32],
            closed_at: 1_760_000_000_000,
            protocol_version: 23,
            transaction_count: 1,
            base_fee: 100,
        }],
        lp_amount_rows: vec![LpOperationAmountRow {
            pool_id: [0x44; 32],
            ledger_sequence: TEST_LEDGER,
            transaction_id: 7,
            application_order: 1,
            asset_id: 42,
            amount: -1_000,
        }],
        ..Default::default()
    };

    let mut writer = PartitionWriter::open(ch.clone());
    writer
        .write_lp_amounts_only(&staged)
        .await
        .expect("targeted write");
    writer.commit().await.expect("commit");

    let amounts: u64 = ch
        .query("SELECT count() FROM lp_operation_amounts WHERE ledger_sequence = ?")
        .bind(TEST_LEDGER)
        .fetch_one()
        .await
        .expect("count amounts");
    assert_eq!(amounts, 1, "the targeted table must receive its row");

    // The marker is the canary: `write_ledger` would have buffered and
    // flushed it on commit, so its absence proves the other 20-odd tables
    // were skipped too.
    let markers: u64 = ch
        .query("SELECT count() FROM ledgers WHERE sequence = ?")
        .bind(TEST_LEDGER)
        .fetch_one()
        .await
        .expect("count ledgers");
    assert_eq!(
        markers, 0,
        "targeted write must not write the ledgers commit marker"
    );

    for table in ["lp_operation_amounts", "ledgers"] {
        let _ = ch
            .query(&format!(
                "ALTER TABLE {table} DELETE WHERE {} = ?",
                if table == "ledgers" {
                    "sequence"
                } else {
                    "ledger_sequence"
                }
            ))
            .bind(TEST_LEDGER)
            .with_setting("mutations_sync", "1")
            .execute()
            .await;
    }
}
