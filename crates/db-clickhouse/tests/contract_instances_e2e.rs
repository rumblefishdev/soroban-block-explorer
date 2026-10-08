//! `contract_instances` through the production write path (task 0620).
//!
//! Unit tests stop at staging; this drives `persist_ledger_clickhouse` against
//! a live ClickHouse so a staged row the writer never drains, or a column the
//! row type does not match, fails here. An instance written in one ledger and
//! changed in a later one must read back as the later entry, byte for byte.
//!
//! Gated on `CLICKHOUSE_URL`. Run locally against a throwaway database:
//!
//!   CLICKHOUSE_URL=http://localhost:8123 CLICKHOUSE_DATABASE=scratch_0620 \
//!       cargo test -p db-clickhouse --test contract_instances_e2e

use db_clickhouse::persist::{ClassificationCache, persist_ledger_clickhouse};
use db_clickhouse::{Config, apply_init_sql, client};
use xdr_parser::contract_instance::ExtractedContractInstance;
use xdr_parser::types::ExtractedLedger;

const FIRST: u32 = 99_999_620;
const LATER: u32 = 99_999_625;
const CONTRACT: [u8; 32] = [0x62; 32];

fn ledger(sequence: u32) -> ExtractedLedger {
    ExtractedLedger {
        sequence,
        hash: format!("{sequence:064x}"),
        closed_at: 1_789_000_000,
        protocol_version: 28,
        transaction_count: 0,
        base_fee: 100,
    }
}

async fn persist(cl: &clickhouse::Client, sequence: u32, data_xdr: &[u8]) {
    let instances = [ExtractedContractInstance {
        contract: CONTRACT,
        data_xdr: data_xdr.to_vec(),
        ledger_sequence: sequence,
    }];
    persist_ledger_clickhouse(
        cl,
        &ledger(sequence),
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &instances,
        &[],
        &[],
        &[],
        &[],
        &[],
        &ClassificationCache::new(),
    )
    .await
    .expect("persist must succeed against live CH");
}

#[derive(clickhouse::Row, serde::Deserialize, Debug, PartialEq)]
struct Instance {
    #[serde(with = "serde_bytes")]
    data_xdr: Vec<u8>,
    ledger: i64,
}

#[tokio::test]
async fn a_later_change_replaces_the_instance() {
    let Some(url) = std::env::var("CLICKHOUSE_URL").ok() else {
        eprintln!("CLICKHOUSE_URL not set — skipping contract instances e2e test");
        return;
    };
    let cl = client(&Config {
        url,
        ..Config::from_env()
    });
    apply_init_sql(&cl).await.expect("apply init.sql");
    cl.query(
        "ALTER TABLE contract_instances DELETE WHERE contract = unhex(?) SETTINGS mutations_sync = 2",
    )
    .bind(hex::encode(CONTRACT))
    .execute()
    .await
    .expect("clear leftovers");

    let read = || async {
        cl.query("SELECT data_xdr, ledger FROM contract_instances FINAL WHERE contract = unhex(?)")
            .bind(hex::encode(CONTRACT))
            .fetch_all::<Instance>()
            .await
            .expect("read instances")
    };

    // Bytes that are not valid UTF-8 must survive the binary `String` column.
    persist(&cl, FIRST, &[0x00, 0xff, 0x10]).await;
    persist(&cl, LATER, &[0x00, 0xfe, 0x20, 0x30]).await;

    assert_eq!(
        read().await,
        vec![Instance {
            data_xdr: vec![0x00, 0xfe, 0x20, 0x30],
            ledger: i64::from(LATER),
        }]
    );
}
