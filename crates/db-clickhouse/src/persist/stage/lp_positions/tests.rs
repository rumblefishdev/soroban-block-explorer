use super::*;

const POOL: &str = "aa00000000000000000000000000000000000000000000000000000000000001";
const TX_SOURCE: &str = "GBTXSOURCEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const OP_SOURCE: &str = "GBOPSOURCEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const LEDGER: i64 = 64_000_000;

fn tx(hash: &str, successful: bool) -> ExtractedTransaction {
    ExtractedTransaction {
        hash: hash.to_string(),
        inner_tx_hash: None,
        ledger_sequence: 64_000_000,
        source_account: TX_SOURCE.to_string(),
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
        created_at: 1_700_000_000,
        parse_error: false,
        ledger_deltas: vec![],
    }
}

fn op(hash: &str, op_type: OperationType, source: Option<&str>) -> ExtractedOperation {
    ExtractedOperation {
        transaction_hash: hash.to_string(),
        operation_index: 0,
        op_type,
        source_account: source.map(str::to_string),
        asset_appearances: vec![],
        counterparties: vec![],
        source_muxed_id: None,
        destination_muxed_id: None,
        details: serde_json::json!({ "liquidityPoolId": POOL }),
    }
}

fn depositors(rows: &[LpFirstDepositRow]) -> Vec<i64> {
    let mut out: Vec<i64> = rows.iter().map(|r| r.account_id).collect();
    out.sort_unstable();
    out
}

/// A failed transaction keeps its operations in the record, but its deposit
/// issued no shares — it must not become anyone's first deposit.
#[test]
fn a_failed_transaction_deposits_nothing() {
    let txs = [tx("01", false)];
    let ops = [(
        "01".to_string(),
        vec![op("01", OperationType::LiquidityPoolDeposit, None)],
    )];

    assert!(
        lp_first_deposit_rows(&txs, &ops, LEDGER)
            .unwrap()
            .is_empty()
    );
}

/// The depositor is the op's own source when it has one, else the
/// transaction's.
#[test]
fn the_depositor_is_the_op_source_else_the_transaction_source() {
    let txs = [tx("01", true), tx("02", true)];
    let ops = [
        (
            "01".to_string(),
            vec![op("01", OperationType::LiquidityPoolDeposit, None)],
        ),
        (
            "02".to_string(),
            vec![op(
                "02",
                OperationType::LiquidityPoolDeposit,
                Some(OP_SOURCE),
            )],
        ),
    ];

    let rows = lp_first_deposit_rows(&txs, &ops, LEDGER).unwrap();

    let mut expected = vec![ids::account_id(TX_SOURCE), ids::account_id(OP_SOURCE)];
    expected.sort_unstable();
    assert_eq!(depositors(&rows), expected);
    assert!(rows.iter().all(|r| r.first_deposit_ledger == LEDGER));
    assert!(
        rows.iter()
            .all(|r| r.pool_id == decode_hash(POOL, "pool").unwrap())
    );
}

/// A withdrawal touches the same pool but is not a deposit; two deposits by
/// one account in one ledger are one row.
#[test]
fn only_deposits_count_once_per_account() {
    let txs = [tx("01", true)];
    let ops = [(
        "01".to_string(),
        vec![
            op("01", OperationType::LiquidityPoolWithdraw, None),
            op("01", OperationType::LiquidityPoolDeposit, None),
            op("01", OperationType::LiquidityPoolDeposit, None),
        ],
    )];

    let rows = lp_first_deposit_rows(&txs, &ops, LEDGER).unwrap();

    assert_eq!(depositors(&rows), vec![ids::account_id(TX_SOURCE)]);
}
