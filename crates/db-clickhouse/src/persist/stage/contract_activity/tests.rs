use std::collections::{BTreeSet, HashMap};

use clickhouse::Row;
use xdr_parser::types::ExtractedInvocation;

use super::rows;
use crate::persist::ids;
use crate::persist::rows::{ContractActivityRow, TransactionOperationRow, TransactionRow};
use crate::persist::stage::StagedLedger;

#[test]
fn column_order_contract_activity() {
    // RowBinary is positional — the struct must match init.sql column by column.
    assert_eq!(
        ContractActivityRow::COLUMN_NAMES,
        [
            "contract_id",
            "ledger_sequence",
            "application_order",
            "caller_id",
            "caller_contract_id",
            "invocation_count"
        ]
    );
}

/// Every way a transaction touches a contract lands in `contract_activity`,
/// keyed by the transaction's position, and only an invoked contract carries
/// its caller — an account in `caller_id`, a contract in `caller_contract_id`
/// — and a non-zero `invocation_count`.
#[test]
fn contract_activity_is_the_presence_plus_the_invocation_caller() {
    let hash = "ab".repeat(32);
    let tx_id = 111;
    let account = "G".to_string() + &"A".repeat(55);
    let caller_contract = "C".to_string() + &"B".repeat(55);
    let (by_account, by_contract, named_by_op, by_event) = (
        "C".to_string() + &"D".repeat(55),
        "C".to_string() + &"E".repeat(55),
        ids::contract_id(&("C".to_string() + &"F".repeat(55))),
        ids::contract_id(&("C".to_string() + &"G".repeat(55))),
    );

    let mut out = StagedLedger {
        transaction_rows: vec![TransactionRow {
            id: tx_id,
            hash: [0xab; 32],
            ledger_sequence: 10,
            application_order: 2,
            source_id: 0,
            fee_charged: 0,
            inner_tx_hash: None,
            successful: true,
            operation_count: 1,
            has_soroban: true,
            parse_error: false,
        }],
        tx_operation_rows: vec![TransactionOperationRow {
            ledger_sequence: 10,
            application_order: 2,
            operation_index: 0,
            op_type: 24,
            source_id: None,
            destination_id: None,
            contract_id: Some(named_by_op),
            asset_code: String::new(),
            asset_issuer_id: None,
            pool_ids: vec![],
        }],
        ..Default::default()
    };
    let invocation = |contract: &str, caller: &str| ExtractedInvocation {
        transaction_hash: hash.clone(),
        contract_id: Some(contract.to_owned()),
        caller_account: Some(caller.to_owned()),
        function_name: Some("f".into()),
        function_args: serde_json::json!([]),
        return_value: serde_json::Value::Null,
        successful: true,
        invocation_index: 0,
        depth: 0,
        ledger_sequence: 10,
        created_at: 1_700_000_000,
    };
    let invocations = vec![(
        hash.clone(),
        vec![
            invocation(&by_account, &account),
            invocation(&by_contract, &caller_contract),
            // A second invocation of the same contract in the same
            // transaction, by another caller: the first one's caller stays.
            invocation(&by_account, &caller_contract),
        ],
    )];
    let tx_id_by_hash = HashMap::from([(hash.clone(), tx_id)]);
    // An operation event of another transaction, position 1.
    let from_events = BTreeSet::from([(by_event, 1)]);

    rows(&mut out, &invocations, &tx_id_by_hash, from_events, 10).expect("stage");

    let row = |contract_id, application_order, caller_id, caller_contract_id, invocation_count| {
        ContractActivityRow {
            contract_id,
            ledger_sequence: 10,
            application_order,
            caller_id,
            caller_contract_id,
            invocation_count,
        }
    };
    let mut expected = vec![
        row(
            ids::contract_id(&by_account),
            2,
            Some(ids::account_id(&account)),
            None,
            2, // invoked twice: the first caller stays, both calls count
        ),
        row(
            ids::contract_id(&by_contract),
            2,
            None,
            Some(ids::contract_id(&caller_contract)),
            1,
        ),
        row(named_by_op, 2, None, None, 0),
        row(by_event, 1, None, None, 0),
    ];
    expected.sort();
    assert_eq!(out.contract_activity_rows, expected);

    // Same keys as the presence index it replaces.
    let keys: Vec<_> = out
        .contract_tx_rows
        .iter()
        .map(|r| (r.contract_id, r.application_order))
        .collect();
    let activity_keys: Vec<_> = out
        .contract_activity_rows
        .iter()
        .map(|r| (r.contract_id, r.application_order))
        .collect();
    assert_eq!(keys, activity_keys);
}
