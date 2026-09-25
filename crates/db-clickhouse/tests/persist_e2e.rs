//! End-to-end persist test — drives the **production write path**
//! (`persist_ledger_clickhouse`, the exact wrapper the indexer Lambda
//! calls per ledger after the 0241 PG → CH swap) against a live
//! ClickHouse and asserts the row lands and a replay is idempotent.
//!
//! This closes the gap between "compiles + unit-tested" and "we know
//! it actually writes correctly to CH": the `mod.rs` unit tests cover
//! the retry classifier and log redaction, `tests_cross.rs` covers
//! staging column order, but nothing exercised
//! `persist_ledger_clickhouse` against a real server end-to-end.
//!
//! Coverage maps to two task-0241 acceptance criteria:
//!   * "Smoke test: ledger N writes to CH, query `SELECT … FROM
//!     ledgers WHERE sequence = N` returns the row".
//!   * "Replay safety: re-delivering an S3 event = no duplicates in
//!     CH (`ReplacingMergeTree` verified)".
//!
//! Gated on `CLICKHOUSE_URL`: skipped cleanly when unset, so CI
//! without a ClickHouse instance stays green (same pattern as
//! `smoke.rs`). Run locally:
//!
//!   docker compose up -d clickhouse
//!   CLICKHOUSE_URL=http://localhost:8123 \
//!       cargo test -p db-clickhouse --test persist_e2e

use db_clickhouse::persist::persist_ledger_clickhouse;
use db_clickhouse::{Config, apply_init_sql, client};
use domain::OperationType;
use xdr_parser::types::{ExtractedLedger, ExtractedOperation, ExtractedTransaction};

/// Out-of-band sentinel sequence — distinct from `smoke.rs`
/// (`99_999_001`) and far above any real backfilled / live-tail
/// ledger, so the test never collides with production data.
const E2E_LEDGER: u32 = 99_999_002;

/// Synthetic source account — valid StrKey shape (`G` + 55 chars) so
/// the surrogate-id derivation in staging produces a stable hub id.
fn source_account() -> String {
    "G".to_string() + &"A".repeat(55)
}

fn ch_url() -> Option<String> {
    std::env::var("CLICKHOUSE_URL").ok()
}

fn fixture_ledger() -> ExtractedLedger {
    ExtractedLedger {
        sequence: E2E_LEDGER,
        hash: "ab".repeat(32),
        closed_at: 1_700_000_000,
        protocol_version: 22,
        transaction_count: 1,
        base_fee: 100,
    }
}

fn fixture_tx() -> ExtractedTransaction {
    let mut bytes = vec![0u8; 32];
    // A distinct first 8 bytes, so the hash-prefix check below pins the byte
    // order, not a zero (task 0580).
    bytes[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    bytes[31] = 0x42;
    ExtractedTransaction {
        hash: hex::encode(&bytes),
        inner_tx_hash: None,
        ledger_sequence: E2E_LEDGER,
        source_account: source_account(),
        fee_source: None,
        fee_charged: 100,
        successful: true,
        result_code: "txSuccess".into(),
        envelope_xdr: String::new(),
        result_xdr: String::new(),
        result_meta_xdr: None,
        operation_tree: None,
        memo_type: None,
        memo: None,
        source_muxed_id: None,
        created_at: 1_700_000_000,
        parse_error: false,
        ledger_deltas: vec![],
    }
}

/// One liquidity-pool deposit — enough to exercise both operation tables and
/// both amount tables through the real writer (task 0372).
fn fixture_ops(tx_hash: &str) -> Vec<(String, Vec<ExtractedOperation>)> {
    let pool = "ab".repeat(32);
    let op = ExtractedOperation {
        transaction_hash: tx_hash.to_string(),
        operation_index: 1,
        op_type: OperationType::LiquidityPoolDeposit,
        source_account: None,
        asset_appearances: vec![],
        counterparties: vec![],
        source_muxed_id: None,
        destination_muxed_id: None,
        details: serde_json::json!({
            "liquidityPoolId": pool,
            "poolDelta": {
                "poolId": pool,
                "assetA": "native",
                "amountA": 1_000,
                "assetB": "native",
                "amountB": 2_000,
            },
        }),
    };
    vec![(tx_hash.to_string(), vec![op])]
}

/// Drive the production per-ledger persist wrapper once. Empty slices for
/// everything except the ledger, one transaction and its one operation keep
/// the fixture minimal while still exercising the multi-table write
/// (ledgers + transactions + operations + the surrogate-id `accounts` hub).
async fn persist_once(cl: &clickhouse::Client) {
    let ledger = fixture_ledger();
    let txs = vec![fixture_tx()];
    let ops = fixture_ops(&txs[0].hash);
    persist_ledger_clickhouse(
        cl,
        &ledger,
        &txs,
        &ops,
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
        &db_clickhouse::persist::ClassificationCache::new(),
    )
    .await
    .expect("persist_ledger_clickhouse must succeed against live CH");
}

async fn cleanup(cl: &clickhouse::Client) {
    let acct = source_account();
    for stmt in [
        format!("ALTER TABLE ledgers DELETE WHERE sequence = {E2E_LEDGER}"),
        format!("ALTER TABLE transactions DELETE WHERE ledger_sequence = {E2E_LEDGER}"),
        format!(
            "ALTER TABLE transaction_hash_prefix_index DELETE WHERE ledger_sequence = {E2E_LEDGER}"
        ),
        format!("ALTER TABLE transaction_participants DELETE WHERE ledger_sequence = {E2E_LEDGER}"),
        format!("ALTER TABLE transaction_operations DELETE WHERE ledger_sequence = {E2E_LEDGER}"),
        format!("ALTER TABLE pool_operation_amounts DELETE WHERE ledger_sequence = {E2E_LEDGER}"),
        format!("ALTER TABLE accounts DELETE WHERE account_id = '{acct}'"),
    ] {
        // Best-effort: mutations are async server-side; failures here
        // (e.g. table empty) must not fail the test.
        let _ = cl.query(&stmt).execute().await;
    }
}

#[tokio::test]
async fn persist_ledger_clickhouse_writes_and_dedupes() {
    let Some(url) = ch_url() else {
        eprintln!("CLICKHOUSE_URL not set — skipping persist e2e test");
        return;
    };
    let cfg = Config {
        url,
        ..Config::from_env()
    };
    let cl = client(&cfg);
    apply_init_sql(&cl)
        .await
        .expect("apply init.sql (idempotent)");
    cleanup(&cl).await;

    // ---- write once via the production persist path ----
    persist_once(&cl).await;

    // AC: SELECT … FROM ledgers WHERE sequence = N returns the row,
    // with every scalar column round-tripping its input value.
    let proto: i32 = cl
        .query("SELECT protocol_version FROM ledgers WHERE sequence = ?")
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("ledger row must exist after persist");
    assert_eq!(proto, 22, "protocol_version round-trips");

    let tx_count: i32 = cl
        .query("SELECT transaction_count FROM ledgers WHERE sequence = ?")
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("transaction_count");
    assert_eq!(tx_count, 1, "transaction_count round-trips");

    let base_fee: i64 = cl
        .query("SELECT base_fee FROM ledgers WHERE sequence = ?")
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("base_fee");
    assert_eq!(base_fee, 100, "base_fee round-trips");

    // Multi-table write: the transaction landed too (FK surrogate path).
    let tx_rows: u64 = cl
        .query("SELECT count() FROM transactions WHERE ledger_sequence = ?")
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("count transactions");
    assert!(tx_rows >= 1, "transaction row written, got {tx_rows}");

    // The surrogate-id accounts hub got the source account.
    let acct_rows: u64 = cl
        .query("SELECT count() FROM accounts WHERE account_id = ?")
        .bind(source_account())
        .fetch_one()
        .await
        .expect("count accounts");
    assert!(
        acct_rows >= 1,
        "source account hub row written, got {acct_rows}"
    );

    // The prefix row the writer computed in Rust is the one ClickHouse finds
    // from the full hash — `reinterpretAsUInt64(substring(…, 1, 8))`, the
    // readers' expression (task 0580).
    let prefix: u64 = cl
        .query(
            "SELECT hash_prefix FROM transaction_hash_prefix_index \
             WHERE ledger_sequence = ? \
               AND hash_prefix = (SELECT reinterpretAsUInt64(substring(hash, 1, 8)) \
                                  FROM transactions WHERE ledger_sequence = ? LIMIT 1) \
             LIMIT 1",
        )
        .bind(E2E_LEDGER)
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("prefix row found by the SQL prefix of the transaction's hash");
    assert_eq!(
        prefix, 0x0807_0605_0403_0201,
        "little-endian u64 of bytes 0..8"
    );

    // Task 0372: the operation tables are located by position — the
    // transaction at position 1, its first operation at index 0.
    let op_keys: Vec<(i16, i16)> = cl
        .query(
            "SELECT DISTINCT application_order, operation_index FROM transaction_operations \
             WHERE ledger_sequence = ?",
        )
        .bind(E2E_LEDGER)
        .fetch_all()
        .await
        .expect("read transaction_operations");
    assert_eq!(op_keys, vec![(1, 0)]);
    let amounts: u64 = cl
        .query(
            "SELECT count() FROM pool_operation_amounts \
             WHERE ledger_sequence = ? AND application_order = 1 AND operation_index = 0",
        )
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("count pool_operation_amounts");
    assert!(amounts >= 1, "the deposit wrote pool_operation_amounts");

    // ---- replay: re-deliver the same S3 event (same ledger) ----
    persist_once(&cl).await;

    // `ledgers` is plain MergeTree, so a replay may add a duplicate
    // physical row — but the gap-check invariant (the one the runbook
    // B-2 uses) `count(DISTINCT sequence)` must stay 1.
    let distinct_ledgers: u64 = cl
        .query("SELECT count(DISTINCT sequence) FROM ledgers WHERE sequence = ?")
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("distinct ledger count");
    assert_eq!(
        distinct_ledgers, 1,
        "count(DISTINCT sequence) stays 1 after replay"
    );

    // `transactions` is ReplacingMergeTree → FINAL collapses the
    // replayed row. This is the "ReplacingMergeTree verified" AC.
    let tx_final: u64 = cl
        .query("SELECT count() FROM transactions FINAL WHERE ledger_sequence = ?")
        .bind(E2E_LEDGER)
        .fetch_one()
        .await
        .expect("count transactions FINAL");
    assert_eq!(
        tx_final, 1,
        "ReplacingMergeTree dedupes the replayed transaction under FINAL"
    );

    cleanup(&cl).await;
}
