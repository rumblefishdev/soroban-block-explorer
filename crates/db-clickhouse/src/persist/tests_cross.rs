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
            "id", // lore-0331 surrogate (last)
        ],
    );
}

/// ADR 0051 — SAC facet side table. Positional RowBinary must match `init.sql`.
#[test]
fn column_order_asset_sac() {
    assert_columns::<AssetSacRow>(
        "asset_sac",
        &[
            "asset_type",
            "asset_code",
            "issuer_id",
            "contract_id",
            "sac_contract_id",
            "sac_deployed",
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
fn column_order_balances() {
    assert_columns::<BalanceRow>(
        "balances",
        &["holder_id", "asset_id", "amount", "last_updated_ledger"],
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
fn column_order_operation_asset_appearances() {
    assert_columns::<OperationAssetAppearanceRow>(
        "operation_asset_appearances",
        &[
            "asset_id",
            "ledger_sequence",
            "transaction_id",
            "net_settled",
        ],
    );
}

#[test]
fn column_order_operation_pools() {
    assert_columns::<OperationPoolRow>(
        "operation_pools",
        &["pool_id", "ledger_sequence", "transaction_id"],
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
        created_at: 1_700_000_000,
        parse_error: false,
        ledger_deltas: vec![],
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

/// Fee-bump: both the outer and the inner tx hash are indexed to the same
/// ledger, so a lookup by either resolves to the fee-bump (task 0375).
#[test]
fn prepare_fee_bump_indexes_inner_hash() {
    let ledger = synthetic_ledger();
    let mut tx = synthetic_tx(0x10);
    let mut inner = vec![0u8; 32];
    inner[31] = 0x20;
    tx.inner_tx_hash = Some(hex::encode(&inner));

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

    // Two index rows: outer + inner, both → the tx's ledger.
    assert_eq!(staged.hash_index_rows.len(), 2);
    let seq = staged.transaction_rows[0].ledger_sequence;
    let mut outer = [0u8; 32];
    outer[31] = 0x10;
    assert!(
        staged
            .hash_index_rows
            .iter()
            .any(|r| r.hash == outer && r.ledger_sequence == seq)
    );
    let mut inner_bytes = [0u8; 32];
    inner_bytes[31] = 0x20;
    assert!(
        staged
            .hash_index_rows
            .iter()
            .any(|r| r.hash == inner_bytes && r.ledger_sequence == seq)
    );
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
        asset_appearances: vec![],
        counterparties: vec![],
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
fn prepare_registers_fee_bump_fee_source_as_participant() {
    // Task 0359 K2-4: the fee-bump payer funds the fee but runs no ops and is
    // not the inner source — it must still land in transaction_participants.
    let ledger = synthetic_ledger();
    let mut tx = synthetic_tx(0x64);
    let payer = "G".to_string() + &"P".repeat(55);
    tx.fee_source = Some(payer.clone());

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

    assert!(
        staged
            .participant_rows
            .iter()
            .any(|r| r.account_id == ids::account_id(&payer)),
        "fee-bump payer registered as participant"
    );
    assert!(
        staged
            .account_rows
            .iter()
            .any(|a| a.id == ids::account_id(&payer)),
        "payer gets an accounts stub"
    );
}

#[test]
fn prepare_registers_op_counterparties_as_participants() {
    // Task 0359 F-C (K1-5): a parser-emitted counterparty (here a crossed-offer
    // seller) lands in transaction_participants and gets an accounts stub — a
    // role the string-`details` extraction dropped.
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x63);
    let seller = "G".to_string() + &"S".repeat(55);
    let op = ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: 1,
        op_type: OperationType::ManageBuyOffer,
        source_account: None,
        asset_appearances: vec![],
        counterparties: vec![seller.clone()],
        details: serde_json::json!({ "selling": "native", "buying": "native" }),
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

    assert!(
        staged
            .participant_rows
            .iter()
            .any(|r| r.account_id == ids::account_id(&seller)),
        "crossed-offer seller registered as tx participant"
    );
    assert!(
        staged
            .account_rows
            .iter()
            .any(|a| a.id == ids::account_id(&seller)),
        "seller gets an accounts stub"
    );
}

#[test]
fn prepare_stages_operation_asset_appearances() {
    use xdr_parser::asset_appearances::AssetRef;
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x35);
    let issuer = "G".to_string() + &"I".repeat(55);
    // A sell offer: ZERO assets in the legacy slot, two appearances here — native
    // must key as the FIRST-CLASS surrogate, not an empty sentinel.
    let op = ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: 1,
        op_type: OperationType::ManageSellOffer,
        source_account: None,
        asset_appearances: vec![
            AssetRef::Native,
            AssetRef::Credit {
                code: "USDC".into(),
                issuer: issuer.clone(),
            },
        ],
        counterparties: vec![],
        details: serde_json::json!({ "selling": "native", "buying": format!("USDC:{issuer}") }),
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

    assert_eq!(staged.op_asset_rows.len(), 2);
    // Native = ids::asset_id(0,"",0,0) — the golden-pinned first-class key.
    assert_eq!(staged.op_asset_rows[0].asset_id, ids::asset_id(0, "", 0, 0));
    // Classic credit hashes code:issuer_surrogate (issuer StrKey hashed first).
    assert_eq!(
        staged.op_asset_rows[1].asset_id,
        ids::asset_id(1, "USDC", ids::account_id(&issuer), 0)
    );
    // Same tx as the legacy fold row — join-back key intact.
    assert_eq!(
        staged.op_asset_rows[0].transaction_id,
        staged.op_rows[0].transaction_id
    );
    // Task 0359 decision 1c: the credit-leg issuer is NOT a tx participant. The
    // asset's activity lives on its asset page (`op_asset_rows` above); flooding
    // the issuer's participant list with every tx touching its asset would be
    // redundant with the asset index.
    assert!(
        !staged
            .participant_rows
            .iter()
            .any(|r| r.account_id == ids::account_id(&issuer)),
        "asset issuer must NOT be registered as a tx participant (decision 1c)"
    );
}

#[test]
fn op_asset_appearances_dedup_same_asset_across_ops_in_one_tx() {
    use xdr_parser::asset_appearances::AssetRef;
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x36);
    let issuer = "G".to_string() + &"I".repeat(55);
    // TWO sell-offer ops in ONE tx, each touching {native, USDC} = 4 appearances.
    // Per-tx dedup collapses to 2 rows, not 4 (PR #6).
    let mk = |idx: u32| ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: idx,
        op_type: OperationType::ManageSellOffer,
        source_account: None,
        asset_appearances: vec![
            AssetRef::Native,
            AssetRef::Credit {
                code: "USDC".into(),
                issuer: issuer.clone(),
            },
        ],
        counterparties: vec![],
        details: serde_json::json!({ "selling": "native", "buying": format!("USDC:{issuer}") }),
    };
    let ops = vec![(tx.hash.clone(), vec![mk(1), mk(2)])];

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

    assert_eq!(staged.op_asset_rows.len(), 2);
    let mut ids_seen: Vec<i64> = staged.op_asset_rows.iter().map(|r| r.asset_id).collect();
    ids_seen.sort_unstable();
    let mut want = vec![
        ids::asset_id(0, "", 0, 0),
        ids::asset_id(1, "USDC", ids::account_id(&issuer), 0),
    ];
    want.sort_unstable();
    assert_eq!(ids_seen, want);
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
        asset_appearances: vec![],
        counterparties: vec![],
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
        asset_appearances: vec![],
        counterparties: vec![],
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
        asset_appearances: vec![],
        counterparties: vec![],
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
    // task 0365: the same crossing fans out into operation_pools (pool, tx).
    assert_eq!(staged.op_pool_rows.len(), 1);
    assert_eq!(staged.op_pool_rows[0].pool_id, [0xABu8; 32]);
    assert_eq!(
        staged.op_pool_rows[0].transaction_id,
        staged.op_rows[0].transaction_id
    );
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
        asset_appearances: vec![],
        counterparties: vec![],
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
fn op_pool_rows_dedup_same_pool_across_ops_in_one_tx() {
    // Two ops in one tx crossing the SAME pool → one (pool, tx) row (the per-tx
    // dedup, task 0365). The RMT would collapse residuals anyway; deduping at write
    // cuts the backfilled volume up front — the pool twin of the asset fan-out.
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0x34);
    let pool = "ef".repeat(32);
    let mk = |idx: u32| ExtractedOperation {
        transaction_hash: tx.hash.clone(),
        operation_index: idx,
        op_type: OperationType::LiquidityPoolDeposit,
        source_account: None,
        asset_appearances: vec![],
        counterparties: vec![],
        details: serde_json::json!({ "liquidityPoolId": pool }),
    };
    let ops = vec![(tx.hash.clone(), vec![mk(1), mk(2)])];

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

    assert_eq!(staged.op_pool_rows.len(), 1);
    assert_eq!(staged.op_pool_rows[0].pool_id, [0xEFu8; 32]);
    assert_eq!(
        staged.op_pool_rows[0].transaction_id,
        staged.op_rows[0].transaction_id
    );
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
        upgradeable: false,
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
        upgradeable: false,
    }
}

/// NFT row whose contract was deployed in the same ledger with a
/// definitive `Nft` wasm-classifier verdict is written and, unlike an
/// unclassified one, is immediately visible to the read path.
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
        "Nft-classified contract: row written"
    );
    assert_eq!(staged.nft_ownership_rows.len(), 1);

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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &prior,
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    // G1: contract row flipped to Nft from the cross-ledger verdict.
    let contract_row = &staged.contract_rows[0];
    assert_eq!(contract_row.contract_id, contract);
    assert_eq!(contract_row.contract_type, Some(ContractType::Nft as i16));

    // G9-for-free: the corrected verdict feeds NFT routing in the same pass,
    // so the row is written AND immediately visible.
    assert_eq!(staged.nft_rows.len(), 1, "kept with an Nft verdict");
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &prior,
        prior_contract_rows: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    assert_eq!(staged.nft_rows.len(), 1, "kept via prior Nft verdict");
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &prior,
        prior_contract_rows: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    assert_eq!(staged.nft_rows.len(), 0, "SAC event dropped, not written");
}

/// Baseline: no prior verdict and no deploy here → nothing is written. The
/// verdict lookup itself fails CLOSED upstream (a ClickHouse error aborts the
/// ledger and is retried), so reaching this arm means the contract really is
/// unclassifiable, not that the lookup was unavailable.
#[test]
fn prepare_drops_event_without_prior_verdict() {
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    assert!(
        staged.nft_rows.is_empty(),
        "no verdict means no membership claim — the row is dropped and logged"
    );
    assert!(staged.nft_ownership_rows.is_empty());
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &prior,
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
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
        "Fungible verdict: NFT row must drop, not be written"
    );
    assert!(staged.nft_ownership_rows.is_empty());
}

/// Task 0392 — an NFT row whose contract has NO verdict at stage time is
/// DROPPED, not stored. `nfts` means "these are NFTs"; a row we cannot judge
/// has no membership claim to make, and parking it somewhere "just in case" is
/// what the removed quarantine did for two years without anyone draining it.
///
/// The drop is deliberately loud — `prepare` logs the contract so an
/// unrecognised interface shows up as a classifier gap instead of vanishing.
#[test]
fn prepare_drops_nft_row_of_unclassified_contract() {
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
        "unclassified contract: nothing may enter `nfts`"
    );
    assert!(staged.nft_ownership_rows.is_empty());
}

// ---------------------------------------------------------------------------
// Task 0323 — un-deployed SAC modelled as asset (was task 0220 re-insert)
// ---------------------------------------------------------------------------

/// Task 0323 → ADR 0051 — an un-deployed SAC (in `sac_overrides`, not deployed
/// this ledger) is modelled as an ASSET FACET, not a contract: NO
/// `soroban_contracts` row is written for it (the Pass-2 FK stub is suppressed),
/// and the SAC handle is folded onto the underlying classic/native asset row
/// (`sac_contract_id` set, `sac_deployed = false`). (Was the task-0220 skeleton.)
#[test]
fn prepare_models_undeployed_sac_override_as_asset_not_contract() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA0);

    // XLM SAC contract_id (well-known mainnet address). Production feeds these
    // ready-made from `detect_undeployed_sac_overrides` over the ledger's events.
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &overrides,
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    // No soroban_contracts row for the un-deployed SAC — it is an asset now.
    assert!(
        !staged
            .contract_rows
            .iter()
            .any(|r| r.contract_id == xlm_sac),
        "un-deployed SAC override writes NO contract row",
    );
    // The native (type=0) identity row exists (one, merged with the singleton) —
    // and the SAC handle is folded into `asset_sac`, NOT onto the `assets` row
    // (ADR 0051 — the assets table is a no-version RMT, so the facet lives in the
    // AggregatingMergeTree side table). `sac_deployed = 0` for an un-deployed override.
    let native_rows: Vec<&_> = staged
        .asset_rows
        .iter()
        .filter(|a| a.asset_type == domain::TokenAssetType::Native as i16)
        .collect();
    assert_eq!(
        native_rows.len(),
        1,
        "one native asset identity row (override merged)"
    );
    assert_eq!(native_rows[0].asset_code, "");
    assert_eq!(native_rows[0].issuer_id, 0);

    let sac_rows: Vec<&_> = staged
        .asset_sac_rows
        .iter()
        .filter(|s| s.asset_type == domain::TokenAssetType::Native as i16)
        .collect();
    assert_eq!(
        sac_rows.len(),
        1,
        "one asset_sac facet row for the native SAC"
    );
    assert_eq!(sac_rows[0].asset_code, "");
    assert_eq!(sac_rows[0].issuer_id, 0);
    assert_eq!(
        sac_rows[0].contract_id, 0,
        "facet keyed on the carrier (contract_id 0)"
    );
    assert_eq!(
        sac_rows[0].sac_contract_id,
        super::ids::contract_id(xlm_sac),
        "SAC handle stored as the C… surrogate",
    );
    assert_eq!(
        sac_rows[0].sac_deployed, 0,
        "un-deployed override → sac_deployed = 0",
    );
    // No retired `asset_type = 2` rows — that value is gone (ADR 0051).
    assert!(
        !staged.asset_rows.iter().any(|a| a.asset_type == 2),
        "no retired type=2 rows emitted",
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
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &overrides,
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
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

    // ADR 0051: one asset_sac facet row, `sac_deployed = 1` — the deploy sighting
    // (1) `max`-beats the same-ledger override sighting (0) via push_sac.
    let facet: Vec<&_> = staged
        .asset_sac_rows
        .iter()
        .filter(|s| s.asset_type == domain::TokenAssetType::Native as i16)
        .collect();
    assert_eq!(facet.len(), 1, "one native asset_sac facet row");
    assert_eq!(
        facet[0].sac_contract_id,
        super::ids::contract_id(xlm_sac),
        "facet carries the SAC surrogate",
    );
    assert_eq!(
        facet[0].sac_deployed, 1,
        "deploy sighting wins over override (max-merge)",
    );
}

/// ADR 0051 regression — the clobber bug the columns-on-`assets` design had. A
/// ledger with ONLY trustline activity (a classic_credit asset, no SAC sighting)
/// emits its identity row but ZERO `asset_sac` rows, so a per-ledger re-emit can
/// never zero a previously-recorded SAC facet (the facet lives in the
/// AggregatingMergeTree side table, written only on a SAC sighting).
#[test]
fn prepare_trustline_only_ledger_emits_no_sac_facet() {
    let ledger = synthetic_ledger();
    let tx = synthetic_tx(0xA2);
    let usdc = ExtractedAsset {
        asset_type: domain::TokenAssetType::ClassicCredit,
        asset_code: Some("USDC".to_string()),
        issuer_address: Some(
            "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".to_string(),
        ),
        contract_id: None,
        sac_contract_id: None,
        sac_deployed: false,
        total_supply: None,
        holder_count: None,
    };

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
        assets: std::slice::from_ref(&usdc),
        nfts: &[],
        nft_events: &[],
        lp_positions: &[],
        contract_metadata_writes: &[],
        soroban_token_balances: &[],
        sac_classic: &std::collections::HashMap::new(),
        sac_overrides: &[],
        prior_wasm_verdicts: &std::collections::HashMap::new(),
        prior_contract_verdicts: &std::collections::HashMap::new(),
        prior_contract_rows: &std::collections::HashMap::new(),
    })
    .expect("prepare_with_sac_overrides");

    // Identity row present…
    assert!(
        staged
            .asset_rows
            .iter()
            .any(|a| a.asset_type == 1 && a.asset_code == "USDC"),
        "classic_credit identity row emitted",
    );
    // …but NO facet row → nothing that could clobber a prior SAC facet.
    assert!(
        staged.asset_sac_rows.is_empty(),
        "trustline-only ledger writes no asset_sac rows",
    );
}

// ---- Task 0320: live WASM-upgrade row (build_wasm_upgrade_rows) ----

fn executable_update_event(contract: &str) -> ExtractedEvent {
    // topics = [Symbol("executable_update"), vec[Symbol("Wasm"), Bytes(old=0x11)],
    //                                        vec[Symbol("Wasm"), Bytes(new=0x22)]]
    ExtractedEvent {
        transaction_hash: "abcd".into(),
        event_type: ContractEventType::System,
        source: EventSource::TxLevel,
        contract_id: Some(contract.to_string()),
        topics: serde_json::json!([
            {"type":"sym","value":"executable_update"},
            {"type":"vec","value":[{"type":"sym","value":"Wasm"},{"type":"bytes","value":"ERERERERERERERERERERERERERERERERERERERERERE="}]},
            {"type":"vec","value":[{"type":"sym","value":"Wasm"},{"type":"bytes","value":"IiIiIiIiIiIiIiIiIiIiIiIiIiIiIiIiIiIiIiIiIiI="}]}
        ]),
        data: serde_json::json!({"type":"vec","value":[]}),
        event_index: 0,
        ledger_sequence: 555,
        created_at: 1_700_000_000,
    }
}

/// A prior `soroban_contracts` row for the upgrade-prefetch map. Only the
/// identity columns are meaningful here; `wasm_hash` / `wasm_uploaded_at_ledger`
/// are the pre-upgrade values that `build_wasm_upgrade_rows` overrides.
fn prior_contract_row(
    addr: &str,
    deployer_id: Option<i64>,
    deployed_at_ledger: Option<i64>,
    contract_type: Option<i16>,
    is_sac: bool,
) -> SorobanContractRow {
    SorobanContractRow {
        id: ids::contract_id(addr),
        contract_id: addr.to_string(),
        wasm_hash: Some([0x11u8; 32]),
        wasm_uploaded_at_ledger: deployed_at_ledger.unwrap_or(0),
        deployer_id,
        deployed_at_ledger,
        contract_type,
        is_sac,
    }
}

#[test]
fn build_wasm_upgrade_rows_carries_identity_and_overrides_hash() {
    let addr = "C".to_string() + &"U".repeat(55);
    let events = vec![("abcd".to_string(), vec![executable_update_event(&addr)])];

    let mut prior = std::collections::HashMap::new();
    prior.insert(
        addr.clone(),
        prior_contract_row(&addr, Some(7), Some(100), Some(1), false),
    );

    let rows = stage::build_wasm_upgrade_rows(&events, &prior, 555);
    assert_eq!(rows.len(), 1, "one upgrade row");
    let r = &rows[0];
    assert_eq!(r.contract_id, addr);
    assert_eq!(r.id, ids::contract_id(&addr));
    assert_eq!(r.wasm_hash, Some([0x22u8; 32]), "overridden to NEW hash");
    assert_eq!(
        r.wasm_uploaded_at_ledger, 555,
        "RMT version = upgrade ledger"
    );
    assert_eq!(r.deployer_id, Some(7), "deployer carried forward");
    assert_eq!(r.deployed_at_ledger, Some(100), "deploy ledger carried");
    assert_eq!(r.contract_type, Some(1), "verdict carried (no flip)");
    assert!(!r.is_sac);
}

#[test]
fn build_wasm_upgrade_rows_skips_when_no_prior_row() {
    // No prior row → must NOT emit: a partial row would clobber identity
    // columns to NULL under RMT whole-row replace.
    let addr = "C".to_string() + &"V".repeat(55);
    let events = vec![("abcd".to_string(), vec![executable_update_event(&addr)])];
    let prior = std::collections::HashMap::new();
    assert!(stage::build_wasm_upgrade_rows(&events, &prior, 555).is_empty());
}

#[test]
fn build_wasm_upgrade_rows_ignores_non_upgrade_events() {
    let addr = "C".to_string() + &"W".repeat(55);
    let mut ev = executable_update_event(&addr);
    ev.topics = serde_json::json!([{"type":"sym","value":"transfer"}]);
    let events = vec![("abcd".to_string(), vec![ev])];
    let mut prior = std::collections::HashMap::new();
    prior.insert(
        addr.clone(),
        prior_contract_row(&addr, Some(7), Some(100), Some(1), false),
    );
    assert!(stage::build_wasm_upgrade_rows(&events, &prior, 555).is_empty());
}

#[test]
fn build_wasm_upgrade_rows_ignores_diagnostic_source() {
    // A diagnostic-container copy (or a failed-tx event) must NOT drive a write —
    // it can carry a hash the chain never applied. Mirrors the soroban_events
    // staging guard + the backfill's already-filtered source table.
    let addr = "C".to_string() + &"X".repeat(55);
    let mut ev = executable_update_event(&addr);
    ev.source = EventSource::Diagnostic;
    let events = vec![("abcd".to_string(), vec![ev])];
    let mut prior = std::collections::HashMap::new();
    prior.insert(
        addr.clone(),
        prior_contract_row(&addr, Some(7), Some(100), Some(1), false),
    );
    assert!(stage::build_wasm_upgrade_rows(&events, &prior, 555).is_empty());
}

#[test]
fn build_wasm_upgrade_rows_ignores_non_system_event_type() {
    // A Contract-typed event with executable_update-shaped topics is a spoof —
    // only host-emitted System events may rewrite wasm_hash.
    let addr = "C".to_string() + &"Z".repeat(55);
    let mut ev = executable_update_event(&addr);
    ev.event_type = ContractEventType::Contract;
    let events = vec![("abcd".to_string(), vec![ev])];
    let mut prior = std::collections::HashMap::new();
    prior.insert(
        addr.clone(),
        prior_contract_row(&addr, Some(7), Some(100), Some(1), false),
    );
    assert!(stage::build_wasm_upgrade_rows(&events, &prior, 555).is_empty());
}

#[test]
fn build_wasm_upgrade_rows_carries_is_sac_from_prior() {
    // is_sac rides along from the read-back row (matches the backfill SQL, which
    // also passes it through). No upgrader is a mislabeled SAC on current data,
    // so carry-forward and force-false are equivalent in practice.
    let addr = "C".to_string() + &"Y".repeat(55);
    let events = vec![("abcd".to_string(), vec![executable_update_event(&addr)])];
    let mut prior = std::collections::HashMap::new();
    prior.insert(
        addr.clone(),
        prior_contract_row(&addr, None, None, Some(1), true),
    );
    let rows = stage::build_wasm_upgrade_rows(&events, &prior, 555);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].is_sac, "is_sac carried forward from the prior row");
}
