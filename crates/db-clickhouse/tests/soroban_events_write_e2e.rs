//! The event row the writer sends must match what `soroban_events` declares
//! (task 0541, ADR 0059).
//!
//! The clickhouse driver validates the row struct against `DESCRIBE` before it
//! sends anything, so a column the table has and the struct lacks — or the
//! other way round — fails every insert client-side, with the ledger already
//! parsed. That is how ingest stopped in tasks 0310 and 0548, both times
//! minutes after a deploy. This writes one event of each kind through the real
//! writer and reads it back, so the pairing is checked before the window, not
//! during it.
//!
//! Gated on `CLICKHOUSE_URL` like the other CH tests here.
//!
//! ```bash
//! CLICKHOUSE_URL=http://localhost:8123 \
//!     cargo test -p db-clickhouse --test soroban_events_write_e2e
//! ```

use db_clickhouse::persist::PartitionWriter;
use db_clickhouse::persist::rows::{ContractTransactionRow, LedgerRow, SorobanEventRow};
use db_clickhouse::persist::stage::StagedLedger;
use db_clickhouse::{Config, apply_init_sql, client};

/// Out-of-band sentinel, same convention as `smoke.rs`.
const TEST_LEDGER: i64 = 99_999_303;
const CONTRACT: i64 = -6_164_601_581_949_826_601;

#[tokio::test]
async fn the_writer_and_the_table_agree_on_the_event_row() {
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

    for table in ["soroban_events", "contract_transactions", "ledgers"] {
        let column = if table == "ledgers" {
            "sequence"
        } else {
            "ledger_sequence"
        };
        ch.query(&format!("ALTER TABLE {table} DELETE WHERE {column} = ?"))
            .bind(TEST_LEDGER)
            .with_setting("mutations_sync", "1")
            .execute()
            .await
            .expect("cleanup");
    }

    let event = |transaction_index: u32, operation_index: u16, event_index: u32| SorobanEventRow {
        contract_id: CONTRACT,
        ledger_sequence: TEST_LEDGER,
        transaction_index,
        operation_index,
        event_index,
        application_order: 1,
        event_type: 1,
        signature: Some("transfer".into()),
        topics_xdr: r#"[{"type":"sym","value":"transfer"}]"#.into(),
        data_xdr: r#"{"type":"i128","value":"1"}"#.into(),
    };
    let staged = StagedLedger {
        ledger_sequence: TEST_LEDGER,
        ledger_rows: vec![LedgerRow {
            sequence: TEST_LEDGER,
            hash: [0x7f; 32],
            closed_at: 1_760_000_000_000,
            protocol_version: 23,
            transaction_count: 1,
            base_fee: 100,
        }],
        // A fee charge, an operation event and an end-of-ledger refund: the
        // sentinels are the values a narrower column type would truncate.
        event_rows: vec![event(0, 0, 135), event(1, 0, 0), event(1_048_575, 0, 7)],
        // The presence index the contract's transaction list seeks (task
        // 0541) — a new table the driver validates the same way.
        contract_tx_rows: vec![ContractTransactionRow {
            contract_id: CONTRACT,
            ledger_sequence: TEST_LEDGER,
            application_order: 1,
        }],
        ..Default::default()
    };

    let mut writer = PartitionWriter::open(ch.clone());
    writer.write_ledger(staged).await.expect("write_ledger");
    writer.commit().await.expect("commit");

    let stored: Vec<(u32, u16, u32, i16)> = ch
        .query(
            "SELECT transaction_index, operation_index, event_index, application_order \
             FROM soroban_events WHERE ledger_sequence = ? \
             ORDER BY transaction_index, operation_index, event_index",
        )
        .bind(TEST_LEDGER)
        .fetch_all()
        .await
        .expect("read back");
    assert_eq!(
        stored,
        vec![(0, 0, 135, 1), (1, 0, 0, 1), (1_048_575, 0, 7, 1)],
        "the sentinels must survive the round trip"
    );

    let presence: Vec<(i64, i16)> = ch
        .query(
            "SELECT contract_id, application_order FROM contract_transactions \
             WHERE ledger_sequence = ?",
        )
        .bind(TEST_LEDGER)
        .fetch_all()
        .await
        .expect("read back the presence row");
    assert_eq!(presence, vec![(CONTRACT, 1)]);
}
