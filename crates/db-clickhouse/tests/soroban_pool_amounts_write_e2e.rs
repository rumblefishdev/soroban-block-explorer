//! Task 0374 (W1): the full ledger write drains `soroban_pool_event_amounts`
//! and round-trips an `Int128` amount no `Int64` could hold.
//!
//! The writer's two exhaustive destructures make a forgotten table a compile
//! error; this proves the rows actually land, both on the full write and on a
//! targeted (`--only`) one.
//!
//! ```bash
//! CLICKHOUSE_URL=http://localhost:8123 \
//!     cargo test -p db-clickhouse --test soroban_pool_amounts_write_e2e
//! ```

use db_clickhouse::persist::rows::{LedgerRow, SorobanPoolEventAmountRow};
use db_clickhouse::persist::stage::StagedLedger;
use db_clickhouse::persist::{PartitionWriter, TargetedTables};
use db_clickhouse::{Config, apply_init_sql, client};

/// Out-of-band sentinels, same convention as `smoke.rs`.
const FULL_LEDGER: i64 = 99_999_401;
const TARGETED_LEDGER: i64 = 99_999_402;
/// An 18-decimal amount past `i64::MAX` (9.2e18).
const BIG: i128 = 1_282_501_540_990_846_914_271_528;

fn staged(ledger: i64) -> StagedLedger {
    StagedLedger {
        ledger_sequence: ledger,
        ledger_rows: vec![LedgerRow {
            sequence: ledger,
            hash: [0x7e; 32],
            closed_at: 1_760_000_000_000,
            protocol_version: 23,
            transaction_count: 1,
            base_fee: 100,
        }],
        soroban_pool_amount_rows: vec![SorobanPoolEventAmountRow {
            pool_id: [0x55; 32],
            ledger_sequence: ledger,
            application_order: 2,
            operation_index: 0,
            event_index: 7,
            asset_id: 42,
            amount: -BIG,
        }],
        ..Default::default()
    }
}

async fn cleanup(ch: &clickhouse::Client) {
    for (table, col) in [
        ("soroban_pool_event_amounts", "ledger_sequence"),
        ("ledgers", "sequence"),
    ] {
        ch.query(&format!(
            "ALTER TABLE {table} DELETE WHERE {col} IN ({FULL_LEDGER}, {TARGETED_LEDGER})"
        ))
        .with_setting("mutations_sync", "1")
        .execute()
        .await
        .expect("cleanup");
    }
}

#[tokio::test]
async fn soroban_pool_amounts_land_on_full_and_targeted_writes() {
    let Some(url) = std::env::var("CLICKHOUSE_URL").ok() else {
        eprintln!("CLICKHOUSE_URL not set — skipping");
        return;
    };
    let ch = client(&Config {
        url,
        ..Config::from_env()
    });
    apply_init_sql(&ch).await.expect("apply init.sql");
    cleanup(&ch).await;

    let mut writer = PartitionWriter::open(ch.clone());
    writer
        .write_ledger(staged(FULL_LEDGER))
        .await
        .expect("full write");
    writer.commit().await.expect("commit");

    let mut writer = PartitionWriter::open(ch.clone());
    let only = TargetedTables::parse("soroban_pool_event_amounts").expect("targetable");
    writer
        .write_only(&staged(TARGETED_LEDGER), &only)
        .await
        .expect("targeted write");
    writer.commit().await.expect("commit");

    let rows: Vec<(i64, u32, String)> = ch
        .query(
            "SELECT ledger_sequence, event_index, toString(amount) \
             FROM soroban_pool_event_amounts \
             WHERE ledger_sequence IN (?, ?) ORDER BY ledger_sequence",
        )
        .bind(FULL_LEDGER)
        .bind(TARGETED_LEDGER)
        .fetch_all()
        .await
        .expect("read back");
    assert_eq!(
        rows,
        vec![
            (FULL_LEDGER, 7, (-BIG).to_string()),
            (TARGETED_LEDGER, 7, (-BIG).to_string()),
        ]
    );

    cleanup(&ch).await;
}
