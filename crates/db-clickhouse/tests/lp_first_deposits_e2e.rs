//! `lp_first_deposits` through the production write path (task 0468).
//!
//! The unit tests stop at staging. This drives `persist_ledger_clickhouse`
//! against a live ClickHouse, so the insert passes the client's schema check
//! against a `SimpleAggregateFunction(min, Int64)` column, and the engine's
//! merge is what keeps the first deposit. The ledgers are written out of order
//! on purpose — a parallel backfill writes them that way — and a deposit in a
//! failed transaction, earlier than both, must not count.
//!
//! Gated on `CLICKHOUSE_URL`. Run locally against a throwaway database:
//!
//!   CLICKHOUSE_URL=http://localhost:8123 CLICKHOUSE_DATABASE=scratch_0468 \
//!       cargo test -p db-clickhouse --test lp_first_deposits_e2e

use db_clickhouse::persist::{ClassificationCache, ids, persist_ledger_clickhouse};
use db_clickhouse::{Config, apply_init_sql, client};
use domain::OperationType;
use xdr_parser::types::{ExtractedLedger, ExtractedOperation, ExtractedTransaction};

const POOL: &str = "c6590a4ee0e2c721a21b96a554473fe74c2cb210bd9e0ea7430000000000468e";
const DEPOSITOR: &str = "GDKHHVS4SBVOLDGZNF2CVYW3TM7LRHK3NCVZE22MUEOYMQSMDQD2AVLX";
const FAILED_AT: u32 = 99_999_050;
const FIRST_AT: u32 = 99_999_100;
const LATER_AT: u32 = 99_999_200;

fn ledger(sequence: u32) -> ExtractedLedger {
    ExtractedLedger {
        sequence,
        hash: format!("{sequence:064x}"),
        closed_at: 1_789_000_000,
        protocol_version: 28,
        transaction_count: 1,
        base_fee: 100,
    }
}

fn deposit_tx(sequence: u32, successful: bool) -> (ExtractedTransaction, ExtractedOperation) {
    let hash = format!("{:064x}", u64::from(sequence) * 7);
    let tx = ExtractedTransaction {
        hash: hash.clone(),
        inner_tx_hash: None,
        ledger_sequence: sequence,
        source_account: DEPOSITOR.to_string(),
        fee_source: None,
        fee_charged: 100,
        successful,
        result_code: String::new(),
        envelope_xdr: String::new(),
        result_xdr: String::new(),
        result_meta_xdr: None,
        operation_tree: None,
        memo_type: None,
        memo: None,
        source_muxed_id: None,
        created_at: 1_789_000_000,
        parse_error: false,
        ledger_deltas: vec![],
    };
    // No source of its own: the depositor is the transaction's source.
    let op = ExtractedOperation {
        transaction_hash: hash,
        operation_index: 0,
        op_type: OperationType::LiquidityPoolDeposit,
        source_account: None,
        asset_appearances: vec![],
        counterparties: vec![],
        source_muxed_id: None,
        destination_muxed_id: None,
        details: serde_json::json!({ "liquidityPoolId": POOL }),
    };
    (tx, op)
}

async fn persist(cl: &clickhouse::Client, sequence: u32, successful: bool) {
    let (tx, op) = deposit_tx(sequence, successful);
    persist_ledger_clickhouse(
        cl,
        &ledger(sequence),
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![op])],
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
        &[],
        &[],
        &[],
        &ClassificationCache::new(),
    )
    .await
    .expect("persist must succeed against live CH");
}

#[tokio::test]
async fn the_first_successful_deposit_survives_later_and_failed_ones() {
    let Some(url) = std::env::var("CLICKHOUSE_URL").ok() else {
        eprintln!("CLICKHOUSE_URL not set — skipping lp_first_deposits e2e test");
        return;
    };
    let cl = client(&Config {
        url,
        ..Config::from_env()
    });
    apply_init_sql(&cl).await.expect("apply init.sql");
    let account = ids::account_id(DEPOSITOR);
    cl.query(
        "ALTER TABLE lp_first_deposits DELETE WHERE pool_id = unhex(?) AND account_id = ? \
         SETTINGS mutations_sync = 2",
    )
    .bind(POOL)
    .bind(account)
    .execute()
    .await
    .expect("clear leftovers");

    persist(&cl, LATER_AT, true).await;
    persist(&cl, FIRST_AT, true).await;
    persist(&cl, FAILED_AT, false).await;

    let first = cl
        .query(
            "SELECT min(first_deposit_ledger) FROM lp_first_deposits \
             WHERE pool_id = unhex(?) AND account_id = ?",
        )
        .bind(POOL)
        .bind(account)
        .fetch_one::<i64>()
        .await
        .expect("read the first deposit");
    assert_eq!(first, i64::from(FIRST_AT));
}
