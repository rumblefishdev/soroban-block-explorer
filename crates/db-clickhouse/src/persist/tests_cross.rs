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
            "total_supply",
            "holder_count",
            "icon_url",
        ],
    );
}

/// Task 0231 — off-chain enrichment side table. Keyed like `assets` +
/// enrichment columns + `version`; positional RowBinary must match `init.sql`.
#[test]
fn column_order_asset_enrichment() {
    assert_columns::<AssetEnrichmentRow>(
        "asset_enrichment",
        &[
            "asset_type",
            "asset_code",
            "issuer_id",
            "contract_id",
            "icon_url",
            "name",
            "version",
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

/// Task 0217 / 0220 — quarantine companion to [`NftRow`]. Same shape;
/// `clickhouse::Row::COLUMN_NAMES` ordering must stay byte-for-byte in
/// sync with `init.sql` `nfts_pending` because RowBinary is positional.
#[test]
fn column_order_nfts_pending() {
    assert_columns::<NftPendingRow>(
        "nfts_pending",
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

/// Task 0231 — off-chain enrichment side table for NFTs. Keyed like `nfts`
/// + metadata + `version`; positional RowBinary must match `init.sql`.
#[test]
fn column_order_nft_enrichment() {
    assert_columns::<NftEnrichmentRow>(
        "nft_enrichment",
        &[
            "contract_id",
            "token_id",
            "name",
            "media_url",
            "collection_name",
            "version",
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
            "pool_ids",
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

/// Task 0217 / 0220 — quarantine companion to [`NftOwnershipRow`].
/// Column order must stay byte-for-byte in sync with `init.sql`
/// `nft_ownership_pending` because RowBinary is positional.
#[test]
fn column_order_nft_ownership_pending() {
    assert_columns::<NftOwnershipPendingRow>(
        "nft_ownership_pending",
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
            "gross_volume_a",
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
fn prepare_path_payment_pool_ids_split_fold_and_sort() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x31);
    let dest = "G".to_string() + &"E".repeat(55);
    let pool_a = "11".repeat(32);
    let pool_b = "22".repeat(32);
    let make_op = |idx: u32, pools: Vec<&str>| ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: idx,
        op_type: OperationType::PathPaymentStrictSend,
        source_account: None,
        details: serde_json::json!({
            "destination": dest,
            "destAsset": "native",
            "poolIds": pools,
        }),
    };
    // op1 crosses B then A (deliberately unsorted), op2 crosses only A —
    // different pool sets refine the fold identity → two rows, not one.
    let ops = vec![(
        tx.hash.clone(),
        vec![
            make_op(1, vec![&pool_b, &pool_a]),
            make_op(2, vec![&pool_a]),
        ],
    )];

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
    )
    .expect("prepare");

    assert_eq!(staged.op_rows.len(), 2, "distinct pool sets must not fold");
    let mut rows = staged.op_rows.clone();
    rows.sort_by_key(|r| r.application_order);
    assert_eq!(rows[0].application_order, 1);
    assert_eq!(rows[0].amount, 1);
    assert_eq!(
        rows[0].pool_ids,
        vec![[0x11u8; 32], [0x22u8; 32]],
        "canonical sorted order regardless of crossing order"
    );
    assert_eq!(rows[1].application_order, 2);
    assert_eq!(rows[1].pool_ids, vec![[0x11u8; 32]]);
}

#[test]
fn prepare_sets_gross_volume_a_on_traded_pool_snapshot() {
    // 0261/0266: live ingest now derives gross_volume_a from claimed atoms and
    // stamps it onto the traded pool's snapshot row; an untraded pool stays None.
    let ledger = synthetic_ledger();
    let seq = ledger.sequence;
    let tx = synthetic_tx(0x33);
    let traded = "aa".repeat(32);
    let quiet = "bb".repeat(32);
    let op = ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: 1,
        op_type: OperationType::PathPaymentStrictSend,
        source_account: None,
        details: serde_json::json!({
            "poolIds": [traded],
            "claimedAtoms": [
                { "poolId": traded, "amountA": 500i64 },
                { "poolId": traded, "amountA": 300i64 },
            ],
        }),
    };
    let ops = vec![(tx.hash.clone(), vec![op])];
    let snap = |pool: &str| xdr_parser::types::ExtractedLiquidityPoolSnapshot {
        pool_id: pool.to_string(),
        ledger_sequence: seq,
        created_at: 0,
        reserves: serde_json::json!({ "a": 1_000i64, "b": 2_000i64 }),
        total_shares: "0".to_string(),
        tvl: None,
        volume: None,
        fee_revenue: None,
    };
    let snaps = vec![snap(&traded), snap(&quiet)];

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
        &snaps,
        &[],
        &[],
        &[],
        &[],
    )
    .expect("prepare");

    let traded_row = staged
        .snapshot_rows
        .iter()
        .find(|r| r.pool_id == [0xAAu8; 32])
        .expect("traded snapshot row");
    assert_eq!(
        traded_row.gross_volume_a,
        Some(800),
        "live prepare must sum claimed-atom amountA per pool"
    );
    let quiet_row = staged
        .snapshot_rows
        .iter()
        .find(|r| r.pool_id == [0xBBu8; 32])
        .expect("quiet snapshot row");
    assert_eq!(
        quiet_row.gross_volume_a, None,
        "untraded pool keeps gross_volume_a = None"
    );
}

#[test]
fn prepare_lp_deposit_single_element_pool_ids() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x32);
    let pool = "ab".repeat(32);
    let op = ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: 1,
        op_type: OperationType::LiquidityPoolDeposit,
        source_account: None,
        details: serde_json::json!({ "liquidityPoolId": pool }),
    };
    let ops = vec![(tx.hash.clone(), vec![op])];

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
    )
    .expect("prepare");

    assert_eq!(staged.op_rows.len(), 1);
    assert_eq!(staged.op_rows[0].pool_ids, vec![[0xABu8; 32]]);
}

#[test]
fn prepare_offer_op_pool_ids_from_details() {
    // An offer op crossing an LP carries poolIds in details (parser reads the
    // ManageOfferSuccessResult claim atoms — task 0261/0266 generic extractor);
    // the CH fold must tag pool_ids for it, not just path payments.
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x33);
    let pool = "cd".repeat(32);
    let op = ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: 1,
        op_type: OperationType::ManageBuyOffer,
        source_account: None,
        details: serde_json::json!({
            "offerId": 0,
            "poolIds": [pool],
        }),
    };
    let ops = vec![(tx.hash.clone(), vec![op])];

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
    )
    .expect("prepare");

    assert_eq!(staged.op_rows.len(), 1);
    assert_eq!(staged.op_rows[0].pool_ids, vec![[0xCDu8; 32]]);
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

// ---------------------------------------------------------------------------
// Task 0217 / 0220 — NFT routing into hot / pending / drop buckets
// ---------------------------------------------------------------------------

use domain::NftEventType;
use xdr_parser::SacOverride;
use xdr_parser::types::{
    ContractFunction, ExtractedContractInterface, ExtractedNft, ExtractedNftEvent, SacAssetIdentity,
};

fn synthetic_nft(contract: &str, token: &str) -> ExtractedNft {
    ExtractedNft {
        contract_id: contract.to_string(),
        token_id: token.to_string(),
        collection_name: None,
        owner_account: None,
        name: None,
        media_url: None,
        minted_at_ledger: Some(10),
        last_seen_ledger: 10,
        created_at: 1_700_000_000,
    }
}

fn synthetic_nft_event(
    tx_hash: &str,
    contract: &str,
    token: &str,
    event_order: u16,
) -> ExtractedNftEvent {
    ExtractedNftEvent {
        transaction_hash: tx_hash.to_string(),
        contract_id: contract.to_string(),
        token_id: token.to_string(),
        event_type: NftEventType::Mint,
        owner_account: None,
        event_order,
        ledger_sequence: 10,
        created_at: 1_700_000_000,
    }
}

/// Build a minimal `ExtractedContractInterface` that the wasm-spec
/// classifier reads as `Nft` (OwnerOf is a discriminator function).
fn nft_classified_interface(wasm_hash_hex: &str) -> ExtractedContractInterface {
    ExtractedContractInterface {
        wasm_hash: wasm_hash_hex.to_string(),
        functions: vec![ContractFunction {
            name: "owner_of".into(),
            doc: String::new(),
            inputs: vec![],
            outputs: vec!["Address".into()],
        }],
        wasm_byte_len: 256,
    }
}

/// Build a minimal `ExtractedContractInterface` that the wasm-spec
/// classifier reads as `Fungible` (Decimals is a discriminator function).
fn fungible_classified_interface(wasm_hash_hex: &str) -> ExtractedContractInterface {
    ExtractedContractInterface {
        wasm_hash: wasm_hash_hex.to_string(),
        functions: vec![ContractFunction {
            name: "decimals".into(),
            doc: String::new(),
            inputs: vec![],
            outputs: vec!["u32".into()],
        }],
        wasm_byte_len: 256,
    }
}

/// NFT row whose contract was deployed in the same ledger with a
/// definitive `Nft` wasm-classifier verdict routes into the hot
/// `nft_rows` bucket and produces zero `nft_pending_rows`.
#[test]
fn prepare_routes_nft_classified_contract_to_hot_bucket() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x90);
    let contract = "C".to_string() + &"A".repeat(55);
    let wasm_hex = "11".repeat(32);

    let iface = nft_classified_interface(&wasm_hex);
    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other, // parser default; classifier overrides
        is_sac: false,
        sac_asset: None,
    };
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &[],
        &[],
        std::slice::from_ref(&iface),
        std::slice::from_ref(&dep),
        &[],
        &[],
        &[],
        &[],
        std::slice::from_ref(&nft),
        std::slice::from_ref(&ev),
        &[],
    )
    .expect("prepare");

    assert_eq!(
        staged.nft_rows.len(),
        1,
        "Nft-classified contract: row in hot bucket"
    );
    assert_eq!(
        staged.nft_pending_rows.len(),
        0,
        "Nft-classified contract: nothing in pending"
    );
    assert_eq!(staged.nft_ownership_rows.len(), 1);
    assert_eq!(staged.nft_ownership_pending_rows.len(), 0);

    // Classifier override visible on the contract row.
    let contract_row = &staged.contract_rows[0];
    assert_eq!(contract_row.contract_id, contract);
    assert_eq!(contract_row.contract_type, Some(ContractType::Nft as i16));
}

// ---------------------------------------------------------------------------
// Task 0283 live G1 — cross-ledger WASM verdict via `prior_wasm_verdicts`
// ---------------------------------------------------------------------------

/// The common Soroban case: the WASM was uploaded in an EARLIER ledger, so it
/// is NOT in this ledger's `contract_interfaces` (the same-ledger map is
/// empty). The writer pre-fetched its `Nft` verdict from
/// `wasm_interface_metadata` and passes it via `prior_wasm_verdicts`. The
/// deploy override must consult that fallback and flip the contract to `Nft`
/// — and because the override runs before NFT routing, this ledger's NFT row
/// routes straight to the hot bucket (no quarantine round-trip).
#[test]
fn prepare_applies_prior_wasm_verdict_when_wasm_uploaded_earlier_ledger() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x9A);
    let contract = "C".to_string() + &"D".repeat(55);
    let wasm_hex = "11".repeat(32);

    // No interface in THIS ledger — the upload happened earlier. The verdict
    // comes only from the pre-fetched cross-ledger map (keyed by raw hash).
    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other, // parser default; prior verdict overrides
        is_sac: false,
        sac_asset: None,
    };
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);
    let prior: std::collections::HashMap<[u8; 32], ContractType> =
        std::collections::HashMap::from([([0x11u8; 32], ContractType::Nft)]);

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[], // EMPTY — wasm not uploaded this ledger
        contract_deployments: std::slice::from_ref(&dep),
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: std::slice::from_ref(&nft),
        nft_events: std::slice::from_ref(&ev),
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &[],
        prior_wasm_verdicts: &prior,
        prior_contract_verdicts: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    // G1: contract row flipped to Nft from the cross-ledger verdict.
    let contract_row = &staged.contract_rows[0];
    assert_eq!(contract_row.contract_id, contract);
    assert_eq!(contract_row.contract_type, Some(ContractType::Nft as i16));

    // G9-for-free: the corrected verdict feeds NFT routing in the same pass,
    // so the NFT lands in the hot bucket, not quarantine.
    assert_eq!(staged.nft_rows.len(), 1, "routed to hot, not pending");
    assert_eq!(staged.nft_pending_rows.len(), 0);
}

// ---------------------------------------------------------------------------
// Task 0283 live G2 — assets type-3 row for WASM-classified Soroban fungibles
// ---------------------------------------------------------------------------

/// A contract whose WASM classifies `Fungible` gets a bespoke-Soroban
/// (`asset_type = 3`) row in `assets`, carrying only the surrogate
/// `contract_id` (empty code, issuer 0). Mirror of PG
/// `insert_assets_from_reclassified_contracts`.
#[test]
fn prepare_emits_soroban_asset_row_for_fungible_contract() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x9D);
    let contract = "C".to_string() + &"G".repeat(55);
    let wasm_hex = "33".repeat(32);
    let iface = fungible_classified_interface(&wasm_hex);
    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other,
        is_sac: false,
        sac_asset: None,
    };

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &[],
        &[],
        std::slice::from_ref(&iface),
        std::slice::from_ref(&dep),
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    )
    .expect("prepare");

    let crow = &staged.contract_rows[0];
    assert_eq!(crow.contract_type, Some(ContractType::Fungible as i16));

    let asset = staged
        .asset_rows
        .iter()
        .find(|a| {
            a.contract_id == crow.id && a.asset_type == domain::TokenAssetType::Soroban as i16
        })
        .expect("fungible contract gets a Soroban (type-3) asset row");
    assert_eq!(asset.asset_code, "");
    assert_eq!(asset.issuer_id, 0);
}

/// An `Nft`-classified contract is NOT an asset — it must never produce a
/// Soroban asset row (it routes to `nfts` instead).
#[test]
fn prepare_no_soroban_asset_row_for_nft_contract() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x9E);
    let contract = "C".to_string() + &"H".repeat(55);
    let wasm_hex = "44".repeat(32);
    let iface = nft_classified_interface(&wasm_hex);
    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other,
        is_sac: false,
        sac_asset: None,
    };

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &[],
        &[],
        std::slice::from_ref(&iface),
        std::slice::from_ref(&dep),
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
    )
    .expect("prepare");

    let crow = &staged.contract_rows[0];
    assert_eq!(crow.contract_type, Some(ContractType::Nft as i16));
    let has_soroban = staged.asset_rows.iter().any(|a| {
        a.contract_id == crow.id && a.asset_type == domain::TokenAssetType::Soroban as i16
    });
    assert!(
        !has_soroban,
        "Nft contract must NOT produce a Soroban asset row"
    );
}

// ---------------------------------------------------------------------------
// Task 0283 live G9 — event routing via cross-ledger `prior_contract_verdicts`
// ---------------------------------------------------------------------------

/// A transfer event arrives for a contract deployed in an EARLIER ledger (no
/// deploy here, so `verdict_by_contract` is empty for it). The writer supplied
/// the contract's `Nft` verdict via `prior_contract_verdicts` (read from
/// `soroban_contracts`), so `route_for` sends the row to the HOT bucket instead
/// of quarantine.
#[test]
fn prepare_routes_event_to_hot_via_prior_contract_verdict() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA5);
    let contract = "C".to_string() + &"K".repeat(55);
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);
    let prior: std::collections::HashMap<String, ContractType> =
        std::collections::HashMap::from([(contract.clone(), ContractType::Nft)]);

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: &[], // no deploy this ledger — contract deployed earlier
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: std::slice::from_ref(&nft),
        nft_events: std::slice::from_ref(&ev),
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &prior,
    })
    .expect("prepare_with_sac_overrides");

    assert_eq!(staged.nft_rows.len(), 1, "routed to hot via prior verdict");
    assert_eq!(staged.nft_pending_rows.len(), 0);
}

/// Same shape, but the prior verdict is `Token` (a SAC). The event must DROP,
/// not quarantine — closing the 0221 write-time SAC leak at routing time.
#[test]
fn prepare_drops_event_when_prior_contract_verdict_is_sac() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA6);
    let contract = "C".to_string() + &"L".repeat(55);
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);
    let prior: std::collections::HashMap<String, ContractType> =
        std::collections::HashMap::from([(contract.clone(), ContractType::Token)]);

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: &[],
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: std::slice::from_ref(&nft),
        nft_events: std::slice::from_ref(&ev),
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &prior,
    })
    .expect("prepare_with_sac_overrides");

    assert_eq!(staged.nft_rows.len(), 0, "SAC event dropped, not hot");
    assert_eq!(
        staged.nft_pending_rows.len(),
        0,
        "SAC event dropped, not pending"
    );
}

/// Baseline: no prior verdict and no deploy here → the event still routes to
/// quarantine (pre-G9 behaviour preserved when the map is empty / fail-open).
#[test]
fn prepare_routes_event_to_pending_without_prior_verdict() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA7);
    let contract = "C".to_string() + &"M".repeat(55);
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: &[],
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: std::slice::from_ref(&nft),
        nft_events: std::slice::from_ref(&ev),
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    assert_eq!(staged.nft_rows.len(), 0);
    assert_eq!(
        staged.nft_pending_rows.len(),
        1,
        "quarantined without a verdict"
    );
}

/// A SAC deploy is never reclassified from WASM — `is_sac` short-circuits the
/// override. Even if a (spurious) verdict is present in `prior_wasm_verdicts`
/// for the same hash, the SAC row stays `Token`.
#[test]
fn prepare_prior_wasm_verdict_leaves_sac_untouched() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x9B);
    let contract = "C".to_string() + &"E".repeat(55);
    let wasm_hex = "11".repeat(32);

    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Token,
        is_sac: true,
        sac_asset: None,
    };
    let prior: std::collections::HashMap<[u8; 32], ContractType> =
        std::collections::HashMap::from([([0x11u8; 32], ContractType::Nft)]);

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: std::slice::from_ref(&dep),
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: &[],
        nft_events: &[],
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &[],
        prior_wasm_verdicts: &prior,
        prior_contract_verdicts: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    let row = staged
        .contract_rows
        .iter()
        .find(|r| r.contract_id == contract)
        .expect("contract row present");
    assert_eq!(row.contract_type, Some(ContractType::Token as i16));
}

/// No verdict for the deploy's hash (empty map, no same-ledger interface) →
/// the contract keeps the parser default `Other`. This is the fail-open path:
/// the writer's prefetch found nothing, so behaviour is identical to pre-0283.
#[test]
fn prepare_keeps_other_when_no_prior_verdict() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x9C);
    let contract = "C".to_string() + &"F".repeat(55);
    let wasm_hex = "11".repeat(32);

    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other,
        is_sac: false,
        sac_asset: None,
    };

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: std::slice::from_ref(&dep),
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: &[],
        nft_events: &[],
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    let row = staged
        .contract_rows
        .iter()
        .find(|r| r.contract_id == contract)
        .expect("contract row present");
    assert_eq!(row.contract_type, Some(ContractType::Other as i16));
}

/// NFT row whose contract was deployed in the same ledger with a
/// definitive `Fungible` verdict is dropped entirely (zero rows in
/// either bucket). Locks in the filter-time drop semantics.
#[test]
fn prepare_drops_nft_row_when_contract_classified_fungible() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x91);
    let contract = "C".to_string() + &"B".repeat(55);
    let wasm_hex = "22".repeat(32);

    let iface = fungible_classified_interface(&wasm_hex);
    let dep = ExtractedContractDeployment {
        contract_id: contract.clone(),
        wasm_hash: Some(wasm_hex.clone()),
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Other,
        is_sac: false,
        sac_asset: None,
    };
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);

    let staged = stage::prepare(
        &ledger,
        std::slice::from_ref(&tx),
        &[(tx.hash.clone(), vec![])],
        &[],
        &[],
        std::slice::from_ref(&iface),
        std::slice::from_ref(&dep),
        &[],
        &[],
        &[],
        &[],
        std::slice::from_ref(&nft),
        std::slice::from_ref(&ev),
        &[],
    )
    .expect("prepare");

    assert!(
        staged.nft_rows.is_empty(),
        "Fungible verdict: NFT row must drop, not route to hot"
    );
    assert!(
        staged.nft_pending_rows.is_empty(),
        "Fungible verdict: NFT row must drop, not route to pending"
    );
    assert!(staged.nft_ownership_rows.is_empty());
    assert!(staged.nft_ownership_pending_rows.is_empty());
}

/// NFT row whose contract is NOT deployed in the same ledger (no
/// classifier verdict accessible at stage time) routes to the
/// quarantine `nft_pending_rows` bucket. Mirrors PG `Other`/uncached
/// semantics — CH has no DB access in stage, so prior-ledger
/// classifications are unreachable here.
#[test]
fn prepare_routes_unclassified_contract_nft_to_pending_bucket() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x92);
    let contract = "C".to_string() + &"C".repeat(55);
    let nft = synthetic_nft(&contract, "tk1");
    let ev = synthetic_nft_event(&tx.hash, &contract, "tk1", 0);

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
        std::slice::from_ref(&nft),
        std::slice::from_ref(&ev),
        &[],
    )
    .expect("prepare");

    assert!(
        staged.nft_rows.is_empty(),
        "Unclassified contract: nothing in hot"
    );
    assert_eq!(
        staged.nft_pending_rows.len(),
        1,
        "Unclassified contract: row in pending bucket"
    );
    assert_eq!(staged.nft_ownership_rows.len(), 0);
    assert_eq!(staged.nft_ownership_pending_rows.len(), 1);
}

// ---------------------------------------------------------------------------
// Task 0220 — SAC override re-insert (Option A)
// ---------------------------------------------------------------------------

/// SAC override emits a corrected `SorobanContractRow` with
/// `is_sac=true, contract_type=Token, wasm_uploaded_at_ledger=0`. The
/// matching Pass-2 stub for the same contract_id (would otherwise
/// arrive via the `assets[].contract_id` reference) is suppressed so
/// RMT doesn't tie-break nondeterministically on equal version.
#[test]
fn prepare_emits_sac_override_contract_row_for_xlm_native() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA0);

    // XLM SAC contract_id is the well-known mainnet address; we don't
    // need to derive it inside the test — `prepare_with_sac_overrides`
    // receives it ready-made (as the production parse_ledger step
    // does via `xdr_parser::derive_sac_overrides_from_assets`).
    let xlm_sac = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
    let overrides = vec![SacOverride {
        contract_id: xlm_sac.to_string(),
        identity: SacAssetIdentity::Native,
    }];

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: &[],
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: &[],
        nft_events: &[],
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &overrides,
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    // Exactly one row for the SAC contract — the override; no
    // duplicate Pass-2 stub with the same contract_id.
    let sac_rows: Vec<&_> = staged
        .contract_rows
        .iter()
        .filter(|r| r.contract_id == xlm_sac)
        .collect();
    assert_eq!(sac_rows.len(), 1, "exactly one row emitted for the SAC");
    let row = sac_rows[0];
    assert!(row.is_sac, "is_sac flipped to true via override");
    assert_eq!(row.contract_type, Some(ContractType::Token as i16));
    assert_eq!(
        row.wasm_uploaded_at_ledger, 0,
        "version 0 sentinel — real deploys later win over the stub"
    );
}

/// When the same contract is deployed in the current ledger (real
/// deploy with `is_sac=true` from parser metadata) AND surfaces in
/// `sac_overrides`, the deploy-time row wins and the override is
/// suppressed (no duplicate emission). Locks in the dedup precedence.
#[test]
fn prepare_skips_sac_override_when_contract_deployed_same_ledger() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA1);

    let xlm_sac = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
    let dep = ExtractedContractDeployment {
        contract_id: xlm_sac.to_string(),
        wasm_hash: None,
        deployer_account: None,
        deployed_at_ledger: 10,
        contract_type: ContractType::Token,
        is_sac: true,
        sac_asset: Some(SacAssetIdentity::Native),
    };
    let overrides = vec![SacOverride {
        contract_id: xlm_sac.to_string(),
        identity: SacAssetIdentity::Native,
    }];

    let staged = stage::prepare_with_sac_overrides(&stage::StageInputs {
        ledger: &ledger,
        transactions: std::slice::from_ref(&tx),
        operations: &[(tx.hash.clone(), vec![])],
        events: &[],
        invocations: &[],
        contract_interfaces: &[],
        contract_deployments: std::slice::from_ref(&dep),
        account_states: &[],
        liquidity_pools: &[],
        pool_snapshots: &[],
        assets: &[],
        nfts: &[],
        nft_events: &[],
        lp_positions: &[],
        contract_metadata_writes: &[],
        sac_overrides: &overrides,
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    let sac_rows: Vec<&_> = staged
        .contract_rows
        .iter()
        .filter(|r| r.contract_id == xlm_sac)
        .collect();
    assert_eq!(sac_rows.len(), 1, "deploy-time row preserved, no duplicate");
    let row = sac_rows[0];
    assert!(row.is_sac);
    assert_eq!(
        row.wasm_uploaded_at_ledger, 10,
        "real deploy carries the deploy ledger as version"
    );
}
