//! Cross-cutting tests for the CH writer.
//!
//! Two categories:
//!
//! 1. **Column-order pinning** — `clickhouse::Row::COLUMN_NAMES` per
//!    row struct must match `init.sql` byte-for-byte (RowBinary is
//!    positional; mismatch silently corrupts every row).
//! 2. **Staging behaviour** — `stage::prepare` smoke tests for the
//!    invariants production code depends on (surrogate-id derivation,
//!    FK consistency by integer equality, stub-rowing, Native
//!    singleton, op fold, signature extraction, diagnostic drop).

use super::*;
use clickhouse::Row;
use rows::*;

fn assert_columns<R: Row>(table: &'static str, expected: &[&str]) {
    assert_eq!(
        R::COLUMN_NAMES,
        expected,
        "row struct columns out of sync with init.sql for `{table}`. \
         RowBinary is positional — any drift silently corrupts every row written."
    );
}

#[test]
fn column_order_ledgers() {
    assert_columns::<LedgerRow>(
        "ledgers",
        &[
            "sequence",
            "hash",
            "closed_at",
            "protocol_version",
            "transaction_count",
            "base_fee",
        ],
    );
}

#[test]
fn column_order_wasm_interface_metadata() {
    assert_columns::<WasmInterfaceMetadataRow>(
        "wasm_interface_metadata",
        &["wasm_hash", "metadata"],
    );
}

#[test]
fn column_order_accounts() {
    assert_columns::<AccountRow>(
        "accounts",
        &[
            "id",
            "account_id",
            "first_seen_ledger",
            "last_seen_ledger",
            "sequence_number",
            "home_domain",
        ],
    );
}

#[test]
fn column_order_assets() {
    assert_columns::<AssetRow>(
        "assets",
        &[
            "asset_type",
            "asset_code",
            "issuer_id",
            "contract_id",
            "name",
            "total_supply",
            "holder_count",
            "icon_url",
        ],
    );
}

#[test]
fn column_order_account_balances_current() {
    assert_columns::<AccountBalanceRow>(
        "account_balances_current",
        &[
            "account_id",
            "asset_type",
            "asset_code",
            "issuer_id",
            "balance",
            "last_updated_ledger",
        ],
    );
}

#[test]
fn column_order_soroban_contracts() {
    assert_columns::<SorobanContractRow>(
        "soroban_contracts",
        &[
            "id",
            "contract_id",
            "wasm_hash",
            "wasm_uploaded_at_ledger",
            "deployer_id",
            "deployed_at_ledger",
            "contract_type",
            "is_sac",
            "name",
        ],
    );
}

#[test]
fn column_order_nfts() {
    assert_columns::<NftRow>(
        "nfts",
        &[
            "contract_id",
            "token_id",
            "collection_name",
            "name",
            "media_url",
            "minted_at_ledger",
            "current_owner_id",
            "current_owner_ledger",
        ],
    );
}

#[test]
fn column_order_liquidity_pools() {
    assert_columns::<LiquidityPoolRow>(
        "liquidity_pools",
        &[
            "pool_id",
            "asset_a_type",
            "asset_a_code",
            "asset_a_issuer_id",
            "asset_b_type",
            "asset_b_code",
            "asset_b_issuer_id",
            "fee_bps",
            "last_updated_ledger",
        ],
    );
}

#[test]
fn column_order_lp_positions() {
    assert_columns::<LpPositionRow>(
        "lp_positions",
        &[
            "pool_id",
            "account_id",
            "shares",
            "first_deposit_ledger",
            "last_updated_ledger",
        ],
    );
}

#[test]
fn column_order_transactions() {
    assert_columns::<TransactionRow>(
        "transactions",
        &[
            "id",
            "hash",
            "ledger_sequence",
            "application_order",
            "source_id",
            "fee_charged",
            "inner_tx_hash",
            "successful",
            "operation_count",
            "has_soroban",
            "parse_error",
        ],
    );
}

#[test]
fn column_order_transaction_hash_index() {
    assert_columns::<TransactionHashIndexRow>(
        "transaction_hash_index",
        &["hash", "ledger_sequence"],
    );
}

#[test]
fn column_order_operations_appearances() {
    assert_columns::<OperationAppearanceRow>(
        "operations_appearances",
        &[
            "transaction_id",
            "application_order",
            "type",
            "source_id",
            "destination_id",
            "contract_id",
            "asset_code",
            "asset_issuer_id",
            "pool_id",
            "amount",
            "ledger_sequence",
        ],
    );
}

#[test]
fn column_order_transaction_participants() {
    assert_columns::<TransactionParticipantRow>(
        "transaction_participants",
        &["account_id", "ledger_sequence", "transaction_id"],
    );
}

#[test]
fn column_order_soroban_events() {
    assert_columns::<SorobanEventRow>(
        "soroban_events",
        &[
            "contract_id",
            "transaction_id",
            "ledger_sequence",
            "event_index",
            "event_type",
            "signature",
            "topics_xdr",
            "data_xdr",
        ],
    );
}

#[test]
fn column_order_soroban_invocations_appearances() {
    assert_columns::<SorobanInvocationAppearanceRow>(
        "soroban_invocations_appearances",
        &[
            "contract_id",
            "transaction_id",
            "ledger_sequence",
            "caller_id",
            "caller_contract_id",
            "amount",
        ],
    );
}

#[test]
fn column_order_nft_ownership() {
    assert_columns::<NftOwnershipRow>(
        "nft_ownership",
        &[
            "contract_id",
            "token_id",
            "ledger_sequence",
            "event_order",
            "transaction_id",
            "owner_id",
            "event_type",
        ],
    );
}

#[test]
fn column_order_liquidity_pool_snapshots() {
    assert_columns::<LiquidityPoolSnapshotRow>(
        "liquidity_pool_snapshots",
        &[
            "pool_id",
            "ledger_sequence",
            "reserve_a",
            "reserve_b",
            "total_shares",
            "tvl",
            "volume",
            "fee_revenue",
        ],
    );
}

// ---------------------------------------------------------------------------
// Staging smoke
// ---------------------------------------------------------------------------

use domain::{ContractEventType, ContractType, OperationType, TokenAssetType};
use xdr_parser::types::{
    EventSource, ExtractedContractDeployment, ExtractedEvent, ExtractedLedger, ExtractedOperation,
    ExtractedTransaction,
};

fn synthetic_ledger() -> ExtractedLedger {
    ExtractedLedger {
        sequence: 10,
        hash: "00".repeat(32),
        closed_at: 1_700_000_000,
        protocol_version: 22,
        transaction_count: 1,
        base_fee: 100,
    }
}

fn synthetic_tx(hash_seed: u8) -> ExtractedTransaction {
    let mut bytes = vec![0u8; 32];
    bytes[31] = hash_seed;
    ExtractedTransaction {
        hash: hex::encode(&bytes),
        inner_tx_hash: None,
        ledger_sequence: 10,
        source_account: "G".to_string() + &"A".repeat(55),
        fee_charged: 100,
        successful: true,
        result_code: "txSuccess".into(),
        envelope_xdr: String::new(),
        result_xdr: String::new(),
        result_meta_xdr: None,
        operation_tree: None,
        memo_type: None,
        memo: None,
        created_at: 1_700_000_000,
        parse_error: false,
    }
}

#[test]
fn prepare_empty_inputs_yields_ledger_and_native_asset() {
    let ledger = synthetic_ledger();
    let staged = stage::prepare(
        &ledger,
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
    )
    .expect("prepare");
    assert_eq!(staged.ledger_rows.len(), 1);
    assert_eq!(staged.transaction_rows.len(), 0);
    assert_eq!(staged.account_rows.len(), 0);
    assert_eq!(staged.event_rows.len(), 0);
    // Native XLM singleton — schema-level concern, writer stages it.
    assert_eq!(staged.asset_rows.len(), 1);
    let native = &staged.asset_rows[0];
    assert_eq!(native.asset_type, TokenAssetType::Native as i16);
    assert_eq!(native.asset_code, "");
    assert_eq!(native.issuer_id, 0);
    assert_eq!(native.contract_id, 0);
    assert_eq!(native.name.as_deref(), Some("Stellar Lumen"));
}

/// FK consistency by Int64 equality: `transactions.source_id` is the
/// same i64 as `accounts.id` for the same StrKey. JOIN by raw integer
/// equality.
#[test]
fn prepare_surrogate_id_fk_consistency() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x10);
    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
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
    )
    .expect("prepare");

    assert_eq!(staged.account_rows.len(), 1);
    assert_eq!(staged.transaction_rows.len(), 1);
    assert_eq!(staged.hash_index_rows.len(), 1);
    assert_eq!(staged.participant_rows.len(), 1);

    let tx_row = &staged.transaction_rows[0];
    let acc_row = &staged.account_rows[0];
    let part_row = &staged.participant_rows[0];

    // FK = derived i64 via ids::*. Cross-table consistency by integer
    // equality.
    assert_eq!(tx_row.source_id, acc_row.id);
    assert_eq!(part_row.account_id, acc_row.id);
    assert_eq!(part_row.transaction_id, tx_row.id);

    // tx surrogate id derived from same hash bytes as `hash` column.
    assert_eq!(tx_row.id, ids::transaction_id(&tx_row.hash));
}

#[test]
fn prepare_extracts_signature_from_first_symbol_topic() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x60);
    let contract = "C".to_string() + &"F".repeat(55);

    let make = |topics: serde_json::Value| ExtractedEvent {
        transaction_hash: tx.hash.clone(),
        event_type: ContractEventType::Contract,
        source: EventSource::TxLevel,
        contract_id: Some(contract.clone()),
        topics,
        data: serde_json::json!({}),
        event_index: 0,
        ledger_sequence: 10,
        created_at: 1_700_000_000,
    };
    let events = vec![(
        tx.hash.clone(),
        vec![
            make(serde_json::json!([
                {"type": "sym", "value": "transfer"},
                {"type": "address", "value": "G..."}
            ])),
            make(serde_json::json!([
                {"type": "sym", "value": "fee"},
                {"type": "address", "value": "G..."}
            ])),
            make(serde_json::json!([
                {"type": "address", "value": "G..."}
            ])),
        ],
    )];

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &events,
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
    )
    .expect("prepare");

    assert_eq!(staged.event_rows.len(), 3);
    let sigs: Vec<Option<String>> = staged
        .event_rows
        .iter()
        .map(|r| r.signature.clone())
        .collect();
    assert!(sigs.contains(&Some("transfer".to_string())));
    assert!(sigs.contains(&Some("fee".to_string())));
    assert!(sigs.contains(&None));
}

#[test]
fn prepare_drops_diagnostic_events_and_orphans() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x20);
    let contract = "C".to_string() + &"D".repeat(55);
    let make = |contract_id: Option<String>, source: EventSource| ExtractedEvent {
        transaction_hash: tx.hash.clone(),
        event_type: ContractEventType::Contract,
        source,
        contract_id,
        topics: serde_json::json!([{"type": "sym", "value": "transfer"}]),
        data: serde_json::json!({}),
        event_index: 0,
        ledger_sequence: 10,
        created_at: 1_700_000_000,
    };
    let events = vec![(
        tx.hash.clone(),
        vec![
            make(Some(contract.clone()), EventSource::TxLevel),
            make(Some(contract.clone()), EventSource::Diagnostic),
            make(None, EventSource::TxLevel),
        ],
    )];

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &events,
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
    )
    .expect("prepare");

    assert_eq!(
        staged.event_rows.len(),
        1,
        "expected diagnostic + orphan to be filtered, got {}",
        staged.event_rows.len()
    );
    assert_eq!(
        staged.event_rows[0].contract_id,
        ids::contract_id(&contract)
    );
}

#[test]
fn prepare_folds_identical_operations() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x30);
    let dest = "G".to_string() + &"E".repeat(55);
    let make_op = |idx: u32| ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: idx,
        op_type: OperationType::Payment,
        source_account: None,
        details: serde_json::json!({
            "destination": dest,
            "asset": "native",
        }),
    };
    let ops = vec![(tx.hash.clone(), vec![make_op(1), make_op(2)])];

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
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
    )
    .expect("prepare");

    assert_eq!(staged.op_rows.len(), 1);
    let op_row = &staged.op_rows[0];
    assert_eq!(op_row.amount, 2);
    assert_eq!(op_row.application_order, 1);
    assert_eq!(op_row.op_type, OperationType::Payment as i16);
    assert_eq!(op_row.destination_id, Some(ids::account_id(&dest)));
}

#[test]
fn prepare_is_deterministic_across_runs() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x40);
    let a = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
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
    )
    .expect("first run");
    let b = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
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
    )
    .expect("second run");

    assert_eq!(a.transaction_rows[0].id, b.transaction_rows[0].id);
    assert_eq!(a.account_rows[0].id, b.account_rows[0].id);
    assert_eq!(a.ledger_rows[0].sequence, b.ledger_rows[0].sequence);
}

#[test]
fn prepare_emits_stub_soroban_contract_rows_for_referenced_only() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x70);
    let referenced_contract = "C".to_string() + &"E".repeat(55);

    let event = ExtractedEvent {
        transaction_hash: tx.hash.clone(),
        event_type: ContractEventType::Contract,
        source: EventSource::TxLevel,
        contract_id: Some(referenced_contract.clone()),
        topics: serde_json::json!([{"type": "sym", "value": "transfer"}]),
        data: serde_json::json!({}),
        event_index: 0,
        ledger_sequence: 10,
        created_at: 1_700_000_000,
    };

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &[(tx.hash.clone(), vec![event])],
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
    )
    .expect("prepare");

    assert_eq!(staged.contract_rows.len(), 1);
    let row = &staged.contract_rows[0];
    assert_eq!(row.contract_id, referenced_contract);
    assert_eq!(row.id, ids::contract_id(&referenced_contract));
    assert_eq!(row.wasm_uploaded_at_ledger, 0);
    assert!(row.wasm_hash.is_none());
    assert!(!row.is_sac);

    // FK from event references the same id.
    assert_eq!(staged.event_rows[0].contract_id, row.id);
}

#[test]
fn prepare_does_not_duplicate_when_contract_both_deployed_and_referenced() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x71);
    let contract = "C".to_string() + &"7".repeat(55);

    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: None,
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other,
        is_sac: false,
        name: None,
        sac_asset: None,
    };
    let event = ExtractedEvent {
        transaction_hash: tx.hash.clone(),
        event_type: ContractEventType::Contract,
        source: EventSource::TxLevel,
        contract_id: Some(contract.clone()),
        topics: serde_json::json!([{"type": "sym", "value": "init"}]),
        data: serde_json::json!({}),
        event_index: 0,
        ledger_sequence: 10,
        created_at: 1_700_000_000,
    };

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &[(tx.hash.clone(), vec![event])],
        &[],
        &[],
        std::slice::from_ref(&dep),
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    )
    .expect("prepare");

    assert_eq!(staged.contract_rows.len(), 1);
    let row = &staged.contract_rows[0];
    assert_eq!(row.contract_id, contract);
    assert_eq!(
        row.wasm_uploaded_at_ledger, 10,
        "real deploy wins over stub"
    );
}

#[test]
fn enum_discriminants_lock_in_with_schema() {
    assert_eq!(TokenAssetType::Native as i16, 0);
    assert_eq!(TokenAssetType::ClassicCredit as i16, 1);
    assert_eq!(ContractType::Token as i16, 0);
    assert_eq!(ContractType::Other as i16, 1);
    assert_eq!(ContractType::Nft as i16, 2);
    assert_eq!(ContractEventType::System as i16, 0);
    assert_eq!(ContractEventType::Contract as i16, 1);
    assert_eq!(ContractEventType::Diagnostic as i16, 2);
    assert_eq!(OperationType::CreateAccount as i16, 0);
    assert_eq!(OperationType::Payment as i16, 1);
}
