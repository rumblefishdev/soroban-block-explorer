//! Tests for `operation.rs`.
//!
//! Extracted from an inline `mod tests` — tests live in a sibling file so
//! the production module stays readable at a glance.

use super::*;

#[test]
fn external_ref_executable_renders_as_its_own_type_never_as_wasm() {
    // CAP-85 (protocol 28): the contract runs code owned by ANOTHER
    // contract, so it has no `wasm_hash` of its own. The one thing this
    // must never do is render as `wasm` with a borrowed or zeroed hash —
    // a consumer would read that as the contract's own code.
    let exec = ContractExecutable::ExternalRef(ContractExecutableExternalRef {
        executable_owner: ScAddress::Contract(ContractId(Hash([0x11; 32]))),
        tag: ScString::try_from(b"fleet-v2".to_vec()).expect("valid ScString"),
    });

    let rendered = format_contract_executable(&exec);

    assert_eq!(rendered["type"], "external_ref");
    assert_eq!(rendered["tag"], "fleet-v2");
    assert!(
        rendered["owner"].as_str().unwrap().starts_with('C'),
        "owner must be the contract StrKey, got {:?}",
        rendered["owner"]
    );
    assert!(
        rendered.get("hash").is_none(),
        "an external ref carries no hash of its own; emitting one would \
         hide the difference from a contract that does"
    );
}

fn pool_entry(reserve_a: i64, reserve_b: i64) -> LedgerEntry {
    LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::LiquidityPool(LiquidityPoolEntry {
            liquidity_pool_id: PoolId(Hash([0x44; 32])),
            body: LiquidityPoolEntryBody::LiquidityPoolConstantProduct(
                LiquidityPoolEntryConstantProduct {
                    params: LiquidityPoolConstantProductParameters {
                        asset_a: Asset::Native,
                        asset_b: Asset::CreditAlphanum4(AlphaNum4 {
                            asset_code: AssetCode4(*b"USDC"),
                            issuer: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0x05; 32]))),
                        }),
                        fee: 30,
                    },
                    reserve_a,
                    reserve_b,
                    total_pool_shares: 500,
                    pool_shares_trust_line_count: 3,
                },
            ),
        }),
        ext: LedgerEntryExt::V0,
    }
}

/// A deposit adds to both reserves and a withdrawal takes from both, so the
/// after-minus-before deltas must come out `+/+` and `-/-` — the sign
/// convention the trade path already uses.
#[test]
fn pool_delta_signs_deposits_positive_and_withdrawals_negative() {
    let pool_id = PoolId(Hash([0x44; 32]));
    let deposit = vec![
        LedgerEntryChange::State(pool_entry(1_000, 2_000)),
        LedgerEntryChange::Updated(pool_entry(1_500, 3_000)),
    ];
    let delta = pool_delta_details(&deposit, &pool_id).expect("deposit delta");
    assert_eq!(delta["amountA"], json!(500));
    assert_eq!(delta["amountB"], json!(1_000));
    assert_eq!(delta["assetA"], json!("native"));

    let withdraw = vec![
        LedgerEntryChange::State(pool_entry(1_500, 3_000)),
        LedgerEntryChange::Updated(pool_entry(1_000, 2_000)),
    ];
    let delta = pool_delta_details(&withdraw, &pool_id).expect("withdraw delta");
    assert_eq!(delta["amountA"], json!(-500));
    assert_eq!(delta["amountB"], json!(-1_000));
}

/// The two boundaries where an image is missing: a pool created by its
/// first deposit has no pre-image, and one emptied by its last withdrawal
/// is `Removed` with no post-image.
#[test]
fn pool_delta_handles_pool_creation_and_removal() {
    let pool_id = PoolId(Hash([0x44; 32]));
    let created = vec![LedgerEntryChange::Created(pool_entry(1_000, 2_000))];
    let delta = pool_delta_details(&created, &pool_id).expect("created delta");
    assert_eq!(delta["amountA"], json!(1_000));
    assert_eq!(delta["amountB"], json!(2_000));

    let emptied = vec![
        LedgerEntryChange::State(pool_entry(1_000, 2_000)),
        LedgerEntryChange::Removed(LedgerKey::LiquidityPool(LedgerKeyLiquidityPool {
            liquidity_pool_id: pool_id.clone(),
        })),
    ];
    let delta = pool_delta_details(&emptied, &pool_id).expect("removed delta");
    assert_eq!(delta["amountA"], json!(-1_000));
    assert_eq!(delta["amountB"], json!(-2_000));
}

/// An op that touches one pool repeatedly must be measured end to end —
/// first pre-image to last post-image — not just its final touch. No
/// current caller emits this shape (deposits and withdrawals touch a pool
/// once), but a path payment would: CAP-38 interleaved matching writes a
/// State/Updated pair per fill, and reading the LAST `State` would report
/// only the last fill.
#[test]
fn pool_delta_spans_every_touch_of_one_op() {
    let pool_id = PoolId(Hash([0x44; 32]));
    let two_fills = vec![
        LedgerEntryChange::State(pool_entry(1_000, 2_000)),
        LedgerEntryChange::Updated(pool_entry(1_400, 1_500)),
        LedgerEntryChange::State(pool_entry(1_400, 1_500)),
        LedgerEntryChange::Updated(pool_entry(1_900, 900)),
    ];
    let delta = pool_delta_details(&two_fills, &pool_id).expect("delta");
    // 1_900 - 1_000 and 900 - 2_000, not the second fill's 500 / -600.
    assert_eq!(delta["amountA"], json!(900));
    assert_eq!(delta["amountB"], json!(-1_100));
}

/// Another pool's entry in the same op must not be read as this pool's.
#[test]
fn pool_delta_ignores_a_different_pool() {
    let changes = vec![
        LedgerEntryChange::State(pool_entry(1_000, 2_000)),
        LedgerEntryChange::Updated(pool_entry(1_500, 3_000)),
    ];
    assert!(pool_delta_details(&changes, &PoolId(Hash([0x99; 32]))).is_none());
}

#[test]
fn extract_payment_operation() {
    let op = Operation {
        source_account: None,
        body: OperationBody::Payment(PaymentOp {
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            asset: Asset::Native,
            amount: 10_000_000,
        }),
    };
    let inner_ops = vec![op];
    let tx = build_v1_tx(inner_ops);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].op_type, OperationType::Payment);
    assert_eq!(result[0].transaction_hash, "abcd1234");
    assert_eq!(result[0].operation_index, 1);
    assert!(result[0].source_account.is_none());
    assert_eq!(result[0].details["asset"], "native");
    assert_eq!(result[0].details["amount"], 10_000_000);
}

#[test]
fn extract_create_account_operation() {
    let op = Operation {
        source_account: Some(MuxedAccount::Ed25519(Uint256([0xAA; 32]))),
        body: OperationBody::CreateAccount(CreateAccountOp {
            destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xCC; 32]))),
            starting_balance: 100_000_000,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].op_type, OperationType::CreateAccount);
    assert!(result[0].source_account.is_some());
    assert_eq!(result[0].details["startingBalance"], 100_000_000);
}

#[test]
fn extract_invoke_host_function_with_args() {
    let contract_addr = ScAddress::Contract(ContractId(Hash([0xDD; 32])));
    let func_name = ScSymbol::try_from("transfer".as_bytes().to_vec()).unwrap();
    let args = vec![ScVal::U64(42), ScVal::Bool(true)].try_into().unwrap();

    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: contract_addr,
                function_name: func_name,
                args,
            }),
            auth: VecM::default(),
        }),
    };

    let return_val = ScVal::I128(Int128Parts { hi: 0, lo: 999 });
    let soroban_meta = SorobanTransactionMeta {
        ext: SorobanTransactionMetaExt::V0,
        events: VecM::default(),
        return_value: return_val,
        diagnostic_events: VecM::default(),
    };
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: Some(soroban_meta),
    });

    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, Some(&tx_meta), None, "abcd1234", 100, 0);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].op_type, OperationType::InvokeHostFunction);
    let details = &result[0].details;
    assert_eq!(details["hostFunctionType"], "invokeContract");
    assert_eq!(details["functionName"], "transfer");
    assert!(!details["contractId"].as_str().unwrap().is_empty());

    // Check function args are ScVal-decoded
    let args = details["functionArgs"].as_array().unwrap();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0]["type"], "u64");
    assert_eq!(args[0]["value"], 42);
    assert_eq!(args[1]["type"], "bool");
    assert_eq!(args[1]["value"], true);

    // Check return value is ScVal-decoded
    let ret = &details["returnValue"];
    assert_eq!(ret["type"], "i128");
    assert_eq!(ret["value"], "999");
}

#[test]
fn extract_invoke_upload_wasm() {
    let wasm_bytes = BytesM::try_from(vec![0u8; 256]).unwrap();
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::UploadContractWasm(wasm_bytes),
            auth: VecM::default(),
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].details["hostFunctionType"], "uploadContractWasm");
    assert_eq!(result[0].details["wasmLength"], 256);
}

#[test]
fn extract_multiple_operations_preserves_order() {
    let ops = vec![
        Operation {
            source_account: None,
            body: OperationBody::Inflation,
        },
        Operation {
            source_account: None,
            body: OperationBody::BumpSequence(BumpSequenceOp {
                bump_to: SequenceNumber(42),
            }),
        },
        Operation {
            source_account: None,
            body: OperationBody::EndSponsoringFutureReserves,
        },
    ];
    let tx = build_v1_tx(ops);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].operation_index, 1);
    assert_eq!(result[0].op_type, OperationType::Inflation);
    assert_eq!(result[1].operation_index, 2);
    assert_eq!(result[1].op_type, OperationType::BumpSequence);
    assert_eq!(result[1].details["bumpTo"], 42);
    assert_eq!(result[2].operation_index, 3);
    assert_eq!(
        result[2].op_type,
        OperationType::EndSponsoringFutureReserves
    );
}

#[test]
fn manage_data_with_value() {
    let op = Operation {
        source_account: None,
        body: OperationBody::ManageData(ManageDataOp {
            data_name: String64::try_from("mykey".as_bytes().to_vec()).unwrap(),
            data_value: Some(DataValue::try_from(vec![0xDE, 0xAD]).unwrap()),
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::ManageData);
    assert_eq!(result[0].details["name"], "mykey");
    // base64 of [0xDE, 0xAD] = "3q0="
    assert_eq!(result[0].details["value"], "3q0=");
}

#[test]
fn manage_sell_offer_details() {
    let op = Operation {
        source_account: None,
        body: OperationBody::ManageSellOffer(ManageSellOfferOp {
            selling: Asset::Native,
            buying: Asset::Native,
            amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 123,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::ManageSellOffer);
    assert_eq!(result[0].details["amount"], 500);
    assert_eq!(result[0].details["price"]["n"], 1);
    assert_eq!(result[0].details["price"]["d"], 2);
    assert_eq!(result[0].details["offerId"], 123);
}

#[test]
fn set_options_partial_fields() {
    let op = Operation {
        source_account: None,
        body: OperationBody::SetOptions(SetOptionsOp {
            inflation_dest: None,
            clear_flags: Some(1),
            set_flags: None,
            master_weight: Some(10),
            low_threshold: None,
            med_threshold: None,
            high_threshold: None,
            home_domain: None,
            signer: None,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::SetOptions);
    assert_eq!(result[0].details["clearFlags"], 1);
    assert_eq!(result[0].details["masterWeight"], 10);
    // Fields not set should not be present
    assert!(result[0].details.get("inflationDest").is_none());
    assert!(result[0].details.get("setFlags").is_none());
}

#[test]
fn extend_footprint_ttl_details() {
    let op = Operation {
        source_account: None,
        body: OperationBody::ExtendFootprintTtl(ExtendFootprintTtlOp {
            ext: ExtensionPoint::V0,
            extend_to: 1000,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::ExtendFootprintTtl);
    assert_eq!(result[0].details["extendTo"], 1000);
}

#[test]
fn restore_footprint_details() {
    let op = Operation {
        source_account: None,
        body: OperationBody::RestoreFootprint(RestoreFootprintOp {
            ext: ExtensionPoint::V0,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::RestoreFootprint);
    assert_eq!(result[0].details, json!({}));
}

#[test]
fn invoke_host_function_without_meta_has_null_return() {
    let contract_addr = ScAddress::Contract(ContractId(Hash([0xDD; 32])));
    let func_name = ScSymbol::try_from("hello".as_bytes().to_vec()).unwrap();
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: HostFunction::InvokeContract(InvokeContractArgs {
                contract_address: contract_addr,
                function_name: func_name,
                args: VecM::default(),
            }),
            auth: VecM::default(),
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);

    assert_eq!(result[0].details["hostFunctionType"], "invokeContract");
    assert!(result[0].details["returnValue"].is_null());
}

// --- path-payment pool claims (task 0261) ---

#[test]
fn path_payment_strict_send_single_pool() {
    let op = build_path_payment_send_op();
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let results = vec![path_payment_send_result(vec![
        order_book_atom(),
        lp_atom([0x11; 32], 500, 200),
    ])];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].op_type, OperationType::PathPaymentStrictSend);
    let details = &result[0].details;
    let pool_ids = details["poolIds"].as_array().unwrap();
    assert_eq!(pool_ids.len(), 1);
    assert_eq!(pool_ids[0], hex::encode([0x11; 32]));
    let atoms = details["claimedAtoms"].as_array().unwrap();
    assert_eq!(atoms.len(), 1);
    assert_eq!(atoms[0]["poolId"], hex::encode([0x11; 32]));
    assert_eq!(atoms[0]["amountSold"], 500);
    assert_eq!(atoms[0]["amountBought"], 200);
    assert_eq!(atoms[0]["assetSold"], "native");
    // both legs native here → amountA = amountSold side.
    assert_eq!(atoms[0]["amountA"], 500);
}

#[test]
fn amount_a_picks_canonical_asset_a_side() {
    // native < credit (XDR Asset order: type 0 < type 1), so asset A is
    // native regardless of which leg of the atom it is.
    let credit = Asset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"USDC"),
        issuer: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0x09; 32]))),
    });
    // sold = native (A), bought = credit (B) → amountA = amountSold.
    let a1 = ClaimAtom::LiquidityPool(ClaimLiquidityAtom {
        liquidity_pool_id: PoolId(Hash([0x11; 32])),
        asset_sold: Asset::Native,
        amount_sold: 700,
        asset_bought: credit.clone(),
        amount_bought: 300,
    });
    // sold = credit (B), bought = native (A) → amountA = amountBought.
    let a2 = ClaimAtom::LiquidityPool(ClaimLiquidityAtom {
        liquidity_pool_id: PoolId(Hash([0x22; 32])),
        asset_sold: credit,
        amount_sold: 250,
        asset_bought: Asset::Native,
        amount_bought: 900,
    });
    let op = build_path_payment_send_op();
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let results = vec![path_payment_send_result(vec![a1, a2])];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);
    let atoms = result[0].details["claimedAtoms"].as_array().unwrap();
    assert_eq!(atoms[0]["amountA"], 700, "native is A, on the sold side");
    assert_eq!(atoms[1]["amountA"], 900, "native is A, on the bought side");
}

#[test]
fn path_payment_strict_receive_multi_hop_dedups_pool_ids() {
    let op = Operation {
        source_account: None,
        body: OperationBody::PathPaymentStrictReceive(PathPaymentStrictReceiveOp {
            send_asset: Asset::Native,
            send_max: 1_000,
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            dest_asset: Asset::Native,
            dest_amount: 900,
            path: VecM::default(),
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    // Two distinct pools + a repeat crossing of the first pool.
    let results = vec![path_payment_receive_result(vec![
        lp_atom([0x11; 32], 500, 200),
        lp_atom([0x22; 32], 200, 150),
        lp_atom([0x11; 32], 100, 40),
    ])];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::PathPaymentStrictReceive);
    let details = &result[0].details;
    let pool_ids = details["poolIds"].as_array().unwrap();
    assert_eq!(pool_ids.len(), 2, "pool ids deduped, order preserved");
    assert_eq!(pool_ids[0], hex::encode([0x11; 32]));
    assert_eq!(pool_ids[1], hex::encode([0x22; 32]));
    // claimedAtoms keeps every fill (amounts feed gross_volume_a).
    assert_eq!(details["claimedAtoms"].as_array().unwrap().len(), 3);
}

#[test]
fn failed_path_payment_has_no_pool_claims() {
    let op = build_path_payment_send_op();
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let results = vec![OperationResult::OpInner(
        OperationResultTr::PathPaymentStrictSend(PathPaymentStrictSendResult::Underfunded),
    )];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);

    let details = &result[0].details;
    assert!(details.get("poolIds").is_none());
    assert!(details.get("claimedAtoms").is_none());
}

#[test]
fn order_book_only_path_payment_has_no_pool_claims() {
    let op = build_path_payment_send_op();
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let results = vec![path_payment_send_result(vec![order_book_atom()])];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);

    let details = &result[0].details;
    assert!(details.get("poolIds").is_none());
    assert!(details.get("claimedAtoms").is_none());
}

#[test]
fn plain_payment_ignores_op_results() {
    let op = Operation {
        source_account: None,
        body: OperationBody::Payment(PaymentOp {
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            asset: Asset::Native,
            amount: 10,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let results = vec![OperationResult::OpInner(OperationResultTr::Payment(
        PaymentResult::Success,
    ))];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);

    assert!(result[0].details.get("poolIds").is_none());
    assert!(result[0].details.get("claimedAtoms").is_none());
}

#[test]
fn tx_op_results_success_only_and_unwraps_fee_bump() {
    let ops: VecM<OperationResult> = vec![OperationResult::OpInner(OperationResultTr::Payment(
        PaymentResult::Success,
    ))]
    .try_into()
    .unwrap();

    let plain = build_tx_result(TransactionResultResult::TxSuccess(ops.clone()));
    assert_eq!(tx_op_results(&plain).unwrap().len(), 1);

    // TxFailed rolls every op back; op-level Success results in it must NOT
    // yield pool data (task 0261 — phantom-crossing guard).
    let failed = build_tx_result(TransactionResultResult::TxFailed(ops.clone()));
    assert!(tx_op_results(&failed).is_none());

    let inner_ok = InnerTransactionResultPair {
        transaction_hash: Hash([0xEE; 32]),
        result: InnerTransactionResult {
            fee_charged: 100,
            result: InnerTransactionResultResult::TxSuccess(ops.clone()),
            ext: InnerTransactionResultExt::V0,
        },
    };
    let fee_bump = build_tx_result(TransactionResultResult::TxFeeBumpInnerSuccess(inner_ok));
    assert_eq!(
        tx_op_results(&fee_bump).unwrap().len(),
        1,
        "fee-bump nesting unwrapped"
    );

    // Fee-bump wrapping a FAILED inner tx → no op results.
    let inner_fail = InnerTransactionResultPair {
        transaction_hash: Hash([0xEE; 32]),
        result: InnerTransactionResult {
            fee_charged: 100,
            result: InnerTransactionResultResult::TxFailed(ops),
            ext: InnerTransactionResultExt::V0,
        },
    };
    let fee_bump_fail = build_tx_result(TransactionResultResult::TxFeeBumpInnerFailed(inner_fail));
    assert!(tx_op_results(&fee_bump_fail).is_none());

    let validation_failed = build_tx_result(TransactionResultResult::TxBadSeq);
    assert!(tx_op_results(&validation_failed).is_none());
}

#[test]
fn tx_op_results_any_covers_failed_arms() {
    // The 0352 fixture shape (7af6d0ed…): TxFailed still carries the
    // per-op array — success op, LowReserve, op-level OpNoAccount.
    let ops: VecM<OperationResult> = vec![
        OperationResult::OpInner(OperationResultTr::BeginSponsoringFutureReserves(
            BeginSponsoringFutureReservesResult::Success,
        )),
        OperationResult::OpInner(OperationResultTr::CreateAccount(
            CreateAccountResult::LowReserve,
        )),
        OperationResult::OpNoAccount,
    ]
    .try_into()
    .unwrap();

    let failed = build_tx_result(TransactionResultResult::TxFailed(ops.clone()));
    let results = tx_op_results_any(&failed).expect("failed arm carries op results");
    assert_eq!(
        results.iter().map(op_result_code).collect::<Vec<_>>(),
        vec!["Success", "LowReserve", "OpNoAccount"]
    );

    let inner_fail = InnerTransactionResultPair {
        transaction_hash: Hash([0xEE; 32]),
        result: InnerTransactionResult {
            fee_charged: 100,
            result: InnerTransactionResultResult::TxFailed(ops),
            ext: InnerTransactionResultExt::V0,
        },
    };
    let fee_bump_fail = build_tx_result(TransactionResultResult::TxFeeBumpInnerFailed(inner_fail));
    assert_eq!(
        tx_op_results_any(&fee_bump_fail).map(<[OperationResult]>::len),
        Some(3),
        "fee-bump failed inner unwrapped"
    );

    // Validation-level failure: no op was attempted, no array exists.
    let validation_failed = build_tx_result(TransactionResultResult::TxBadSeq);
    assert!(tx_op_results_any(&validation_failed).is_none());
}

#[test]
fn failed_tx_drops_pool_claims_for_op_level_success() {
    // op0 = path payment that crossed pool P at the op level, but the whole
    // tx failed (a later op failed). tx_op_results returns None for the
    // failed tx → no poolIds, so rolled-back crossings stay out of the DB
    // and out of gross_volume_a.
    let op = build_path_payment_send_op();
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let ops: VecM<OperationResult> = vec![path_payment_send_result(vec![lp_atom(
        [0x11; 32], 500, 200,
    )])]
    .try_into()
    .unwrap();
    let tx_result = build_tx_result(TransactionResultResult::TxFailed(ops));

    let op_results = tx_op_results(&tx_result);
    assert!(op_results.is_none(), "failed tx exposes no op results");

    let result = extract_operations(&inner, None, op_results, "abcd1234", 100, 0);
    assert!(result[0].details.get("poolIds").is_none());
    assert!(result[0].details.get("claimedAtoms").is_none());
}

#[test]
fn manage_buy_offer_crossing_pool_tags_pool_ids() {
    // An offer that fills against an AMM carries LP claim atoms in its
    // result (ManageOfferSuccessResult.offers_claimed) — the unified
    // extractor must surface them too (task 0261 / 0266 single-run scope).
    let op = Operation {
        source_account: None,
        body: OperationBody::ManageBuyOffer(ManageBuyOfferOp {
            selling: Asset::Native,
            buying: Asset::Native,
            buy_amount: 500,
            price: Price { n: 1, d: 2 },
            offer_id: 0,
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);
    let results = vec![manage_buy_offer_result(vec![
        order_book_atom(),
        lp_atom([0x33; 32], 700, 350),
    ])];

    let result = extract_operations(&inner, None, Some(&results), "abcd1234", 100, 0);

    assert_eq!(result[0].op_type, OperationType::ManageBuyOffer);
    let details = &result[0].details;
    let pool_ids = details["poolIds"].as_array().unwrap();
    assert_eq!(pool_ids.len(), 1);
    assert_eq!(pool_ids[0], hex::encode([0x33; 32]));
    assert_eq!(details["claimedAtoms"].as_array().unwrap().len(), 1);
    // Plain (non-LP) offer fields are preserved alongside the claims.
    assert_eq!(details["offerId"], 0);
}

// --- test helpers ---

fn build_path_payment_send_op() -> Operation {
    Operation {
        source_account: None,
        body: OperationBody::PathPaymentStrictSend(PathPaymentStrictSendOp {
            send_asset: Asset::Native,
            send_amount: 1_000,
            destination: MuxedAccount::Ed25519(Uint256([0xBB; 32])),
            dest_asset: Asset::Native,
            dest_min: 900,
            path: VecM::default(),
        }),
    }
}

fn lp_atom(pool_id: [u8; 32], amount_sold: i64, amount_bought: i64) -> ClaimAtom {
    ClaimAtom::LiquidityPool(ClaimLiquidityAtom {
        liquidity_pool_id: PoolId(Hash(pool_id)),
        asset_sold: Asset::Native,
        amount_sold,
        asset_bought: Asset::Native,
        amount_bought,
    })
}

fn order_book_atom() -> ClaimAtom {
    ClaimAtom::OrderBook(ClaimOfferAtom {
        seller_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xCC; 32]))),
        offer_id: 7,
        asset_sold: Asset::Native,
        amount_sold: 10,
        asset_bought: Asset::Native,
        amount_bought: 10,
    })
}

fn simple_payment_result() -> SimplePaymentResult {
    SimplePaymentResult {
        destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0xBB; 32]))),
        asset: Asset::Native,
        amount: 900,
    }
}

fn path_payment_send_result(offers: Vec<ClaimAtom>) -> OperationResult {
    OperationResult::OpInner(OperationResultTr::PathPaymentStrictSend(
        PathPaymentStrictSendResult::Success(PathPaymentStrictSendResultSuccess {
            offers: offers.try_into().unwrap(),
            last: simple_payment_result(),
        }),
    ))
}

fn path_payment_receive_result(offers: Vec<ClaimAtom>) -> OperationResult {
    OperationResult::OpInner(OperationResultTr::PathPaymentStrictReceive(
        PathPaymentStrictReceiveResult::Success(PathPaymentStrictReceiveResultSuccess {
            offers: offers.try_into().unwrap(),
            last: simple_payment_result(),
        }),
    ))
}

fn manage_buy_offer_result(claimed: Vec<ClaimAtom>) -> OperationResult {
    OperationResult::OpInner(OperationResultTr::ManageBuyOffer(
        ManageBuyOfferResult::Success(ManageOfferSuccessResult {
            offers_claimed: claimed.try_into().unwrap(),
            offer: ManageOfferSuccessResultOffer::Deleted,
        }),
    ))
}

fn build_tx_result(result: TransactionResultResult) -> TransactionResult {
    TransactionResult {
        fee_charged: 100,
        result,
        ext: TransactionResultExt::V0,
    }
}

fn build_v1_tx(operations: Vec<Operation>) -> Transaction {
    Transaction {
        source_account: MuxedAccount::Ed25519(Uint256([0xAA; 32])),
        fee: 100,
        seq_num: SequenceNumber(1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: operations.try_into().unwrap(),
        ext: TransactionExt::V0,
    }
}

#[test]
fn create_claimable_balance_lists_claimants() {
    // Task 0460 #16: `claimants` used to be a bare count — now it IS the
    // vec; addresses and the (recursive) predicates must survive.
    let claimant = |byte: u8, predicate: ClaimPredicate| {
        Claimant::ClaimantTypeV0(ClaimantV0 {
            destination: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32]))),
            predicate,
        })
    };
    let op = Operation {
        source_account: None,
        body: OperationBody::CreateClaimableBalance(CreateClaimableBalanceOp {
            asset: Asset::Native,
            amount: 5_000,
            claimants: vec![
                claimant(0xBB, ClaimPredicate::Unconditional),
                claimant(
                    0xCC,
                    ClaimPredicate::Not(Some(Box::new(ClaimPredicate::BeforeAbsoluteTime(
                        1_700_000_000,
                    )))),
                ),
            ]
            .try_into()
            .unwrap(),
        }),
    };
    let tx = build_v1_tx(vec![op]);
    let inner = InnerTxRef::V1(&tx);

    let result = extract_operations(&inner, None, None, "abcd1234", 100, 0);
    let details = &result[0].details;

    let list = details["claimants"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    let dest = list[0]["destination"].as_str().unwrap();
    assert!(
        dest.starts_with('G') && dest.len() == 56,
        "destination must be a G-strkey, got {dest}"
    );
    assert_eq!(list[0]["predicate"]["type"], "unconditional");
    assert_eq!(list[1]["predicate"]["type"], "not");
    assert_eq!(
        list[1]["predicate"]["predicate"]["type"],
        "beforeAbsoluteTime"
    );
    assert_eq!(
        list[1]["predicate"]["predicate"]["timePoint"],
        1_700_000_000_i64
    );
}
