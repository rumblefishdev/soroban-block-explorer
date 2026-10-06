use super::*;

fn make_account_entry(account_id: AccountId, balance: i64) -> LedgerEntry {
    LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::Account(AccountEntry {
            account_id,
            balance,
            seq_num: SequenceNumber(1),
            num_sub_entries: 0,
            inflation_dest: None,
            flags: 0,
            home_domain: String32::default(),
            thresholds: Thresholds([1, 0, 0, 0]),
            signers: VecM::default(),
            ext: AccountEntryExt::V0,
        }),
        ext: LedgerEntryExt::V0,
    }
}

fn make_account_id(byte: u8) -> AccountId {
    AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([byte; 32])))
}

#[test]
fn extract_created_account() {
    let entry = make_account_entry(make_account_id(0xAA), 1_000_000);
    let change = LedgerEntryChange::Created(entry);
    let changes: LedgerEntryChanges = vec![change].try_into().unwrap();

    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: vec![OperationMeta { changes }].try_into().unwrap(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);

    let r = &results[0];
    assert_eq!(r.change_type, "created");
    assert_eq!(r.entry_type, "account");
    assert_eq!(r.operation_index, Some(0));
    assert_eq!(r.change_index, 0);
    assert!(r.data.is_some());
    assert_eq!(r.data.as_ref().unwrap()["balance"], 1_000_000);
    assert!(r.key["account_id"].as_str().unwrap().starts_with('G'));
}

#[test]
fn extract_removed_account() {
    let account_id = make_account_id(0xBB);
    let change = LedgerEntryChange::Removed(LedgerKey::Account(LedgerKeyAccount {
        account_id: account_id.clone(),
    }));
    let changes: LedgerEntryChanges = vec![change].try_into().unwrap();

    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);

    let r = &results[0];
    assert_eq!(r.change_type, "removed");
    assert_eq!(r.entry_type, "account");
    assert!(r.data.is_none());
    assert!(r.operation_index.is_none());
}

#[test]
fn state_and_updated_pair() {
    let account_id = make_account_id(0xCC);
    let state_entry = make_account_entry(account_id.clone(), 500);
    let updated_entry = make_account_entry(account_id, 1000);

    let changes: LedgerEntryChanges = vec![
        LedgerEntryChange::State(state_entry),
        LedgerEntryChange::Updated(updated_entry),
    ]
    .try_into()
    .unwrap();

    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: vec![OperationMeta { changes }].try_into().unwrap(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].change_type, "state");
    assert_eq!(results[0].data.as_ref().unwrap()["balance"], 500);
    assert_eq!(results[1].change_type, "updated");
    assert_eq!(results[1].data.as_ref().unwrap()["balance"], 1000);
}

#[test]
fn tx_changes_before_and_after_ordering() {
    let before_entry = make_account_entry(make_account_id(0x01), 100);
    let after_entry = make_account_entry(make_account_id(0x02), 200);
    let op_entry = make_account_entry(make_account_id(0x03), 300);

    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: vec![LedgerEntryChange::Created(before_entry)]
            .try_into()
            .unwrap(),
        operations: vec![OperationMeta {
            changes: vec![LedgerEntryChange::Created(op_entry)]
                .try_into()
                .unwrap(),
        }]
        .try_into()
        .unwrap(),
        tx_changes_after: vec![LedgerEntryChange::Updated(after_entry)]
            .try_into()
            .unwrap(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 3);

    // tx_changes_before
    assert_eq!(results[0].change_index, 0);
    assert!(results[0].operation_index.is_none());
    assert_eq!(results[0].data.as_ref().unwrap()["balance"], 100);

    // operation changes
    assert_eq!(results[1].change_index, 1);
    assert_eq!(results[1].operation_index, Some(0));
    assert_eq!(results[1].data.as_ref().unwrap()["balance"], 300);

    // tx_changes_after
    assert_eq!(results[2].change_index, 2);
    assert!(results[2].operation_index.is_none());
    assert_eq!(results[2].data.as_ref().unwrap()["balance"], 200);
}

#[test]
fn extract_trustline_change() {
    let asset = TrustLineAsset::CreditAlphanum4(AlphaNum4 {
        asset_code: AssetCode4(*b"USDC"),
        issuer: make_account_id(0xDD),
    });
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::Trustline(TrustLineEntry {
            account_id: make_account_id(0xAA),
            asset,
            balance: 5000,
            limit: 10000,
            flags: 1,
            ext: TrustLineEntryExt::V0,
        }),
        ext: LedgerEntryExt::V0,
    };

    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "trustline");
    assert_eq!(results[0].data.as_ref().unwrap()["balance"], 5000);
    assert_eq!(results[0].key["asset"]["code"], "USDC");
}

#[test]
fn extract_offer_change() {
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::Offer(OfferEntry {
            seller_id: make_account_id(0xAA),
            offer_id: 42,
            selling: Asset::Native,
            buying: Asset::Native,
            amount: 1000,
            price: Price { n: 1, d: 2 },
            flags: 0,
            ext: OfferEntryExt::V0,
        }),
        ext: LedgerEntryExt::V0,
    };

    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "offer");
    assert_eq!(results[0].key["offer_id"], 42);
    assert_eq!(results[0].data.as_ref().unwrap()["amount"], 1000);
}

#[test]
fn extract_contract_data_change() {
    let contract = ScAddress::Contract(ContractId(Hash([0xCC; 32])));
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: contract.clone(),
            key: ScVal::Symbol(ScSymbol::try_from("counter".as_bytes().to_vec()).unwrap()),
            durability: ContractDataDurability::Persistent,
            val: ScVal::U64(99),
        }),
        ext: LedgerEntryExt::V0,
    };

    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "contract_data");
    assert_eq!(results[0].key["durability"], "persistent");
    assert_eq!(results[0].data.as_ref().unwrap()["val"]["type"], "u64");
    assert_eq!(results[0].data.as_ref().unwrap()["val"]["value"], 99);
}

#[test]
fn extract_contract_code_change() {
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash: Hash([0xEE; 32]),
            code: vec![0x00, 0x61, 0x73, 0x6d].try_into().unwrap(),
        }),
        ext: LedgerEntryExt::V0,
    };

    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "contract_code");
    assert_eq!(results[0].key["hash"], "ee".repeat(32));
    assert_eq!(results[0].data.as_ref().unwrap()["code_byte_len"], 4);
}

#[test]
fn extract_ttl_change() {
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::Ttl(TtlEntry {
            key_hash: Hash([0xFF; 32]),
            live_until_ledger_seq: 5000,
        }),
        ext: LedgerEntryExt::V0,
    };

    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Updated(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "ttl");
    assert_eq!(results[0].change_type, "updated");
    assert_eq!(
        results[0].data.as_ref().unwrap()["live_until_ledger_seq"],
        5000
    );
}

#[test]
fn no_changes_for_non_v3v4() {
    let tx_meta = TransactionMeta::V0(VecM::default());
    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert!(results.is_empty());
}

#[test]
fn v4_meta_extraction() {
    let entry = make_account_entry(make_account_id(0xAA), 500);
    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();

    let tx_meta = TransactionMeta::V4(TransactionMetaV4 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
        events: VecM::default(),
        diagnostic_events: VecM::default(),
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "account");
    assert_eq!(results[0].data.as_ref().unwrap()["balance"], 500);
}

#[test]
fn multiple_operations_track_index() {
    let entry1 = make_account_entry(make_account_id(0x01), 100);
    let entry2 = make_account_entry(make_account_id(0x02), 200);

    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: LedgerEntryChanges::default(),
        operations: vec![
            OperationMeta {
                changes: vec![LedgerEntryChange::Created(entry1)].try_into().unwrap(),
            },
            OperationMeta {
                changes: vec![LedgerEntryChange::Created(entry2)].try_into().unwrap(),
            },
        ]
        .try_into()
        .unwrap(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].operation_index, Some(0));
    assert_eq!(results[0].change_index, 0);
    assert_eq!(results[1].operation_index, Some(1));
    assert_eq!(results[1].change_index, 1);
}

#[test]
fn extract_liquidity_pool_change() {
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::LiquidityPool(LiquidityPoolEntry {
            liquidity_pool_id: PoolId(Hash([0xAB; 32])),
            body: LiquidityPoolEntryBody::LiquidityPoolConstantProduct(
                LiquidityPoolEntryConstantProduct {
                    params: LiquidityPoolConstantProductParameters {
                        asset_a: Asset::Native,
                        asset_b: Asset::Native,
                        fee: 30,
                    },
                    reserve_a: 10000,
                    reserve_b: 20000,
                    total_pool_shares: 5000,
                    pool_shares_trust_line_count: 3,
                },
            ),
        }),
        ext: LedgerEntryExt::V0,
    };

    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "liquidity_pool");
    assert_eq!(results[0].data.as_ref().unwrap()["reserve_a"], 10000);
    assert_eq!(results[0].data.as_ref().unwrap()["reserve_b"], 20000);
    assert_eq!(results[0].data.as_ref().unwrap()["params"]["fee"], 30);
}

#[test]
fn removed_contract_data_key_only() {
    let contract = ScAddress::Contract(ContractId(Hash([0xCC; 32])));
    let change = LedgerEntryChange::Removed(LedgerKey::ContractData(LedgerKeyContractData {
        contract: contract.clone(),
        key: ScVal::Symbol(ScSymbol::try_from("counter".as_bytes().to_vec()).unwrap()),
        durability: ContractDataDurability::Temporary,
    }));

    let changes: LedgerEntryChanges = vec![change].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].change_type, "removed");
    assert_eq!(results[0].entry_type, "contract_data");
    assert!(results[0].data.is_none());
    assert_eq!(results[0].key["durability"], "temporary");
    assert!(
        results[0].key["contract"]
            .as_str()
            .unwrap()
            .starts_with('C')
    );
}

#[test]
fn contract_instance_metadata_extracted_on_created() {
    // Mainnet shape (liquidFi bridge CDKRSOVB…): instance storage holds a
    // Symbol("METADATA") => Map{decimal, name, symbol} struct.
    let sym = |s: &str| ScVal::Symbol(ScSymbol::try_from(s.as_bytes().to_vec()).unwrap());
    let sstr = |s: &str| ScVal::String(ScString::try_from(s.as_bytes().to_vec()).unwrap());
    let metadata = ScVal::Map(Some(
        ScMap::try_from(vec![
            ScMapEntry {
                key: sym("decimal"),
                val: ScVal::U32(7),
            },
            ScMapEntry {
                key: sym("name"),
                val: sstr("liquidFi bridge token"),
            },
            ScMapEntry {
                key: sym("symbol"),
                val: sstr("lUSDC"),
            },
        ])
        .unwrap(),
    ));
    let instance = ScVal::ContractInstance(ScContractInstance {
        executable: ContractExecutable::Wasm(Hash([0xAA; 32])),
        storage: Some(
            ScMap::try_from(vec![ScMapEntry {
                key: sym("METADATA"),
                val: metadata,
            }])
            .unwrap(),
        ),
    });
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: ScAddress::Contract(ContractId(Hash([0xCC; 32]))),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
            val: instance,
        }),
        ext: LedgerEntryExt::V0,
    };
    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Created(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entry_type, "contract_data");
    let md = results[0]
        .token_metadata
        .as_ref()
        .expect("instance METADATA should be extracted onto the change");
    assert_eq!(md.name.as_deref(), Some("liquidFi bridge token"));
    assert_eq!(md.symbol.as_deref(), Some("lUSDC"));
    assert_eq!(md.decimals, Some(7));
}

#[test]
fn contract_instance_metadata_extracted_on_updated() {
    // Deploy-then-init: METADATA set by a later init() lands on an `updated`
    // instance change (chain-confirmed updated path — task 0297 Option B).
    let sym = |s: &str| ScVal::Symbol(ScSymbol::try_from(s.as_bytes().to_vec()).unwrap());
    let sstr = |s: &str| ScVal::String(ScString::try_from(s.as_bytes().to_vec()).unwrap());
    let metadata = ScVal::Map(Some(
        ScMap::try_from(vec![
            ScMapEntry {
                key: sym("decimal"),
                val: ScVal::U32(7),
            },
            ScMapEntry {
                key: sym("name"),
                val: sstr("liquidFi LP token"),
            },
            ScMapEntry {
                key: sym("symbol"),
                val: sstr("lUSDC"),
            },
        ])
        .unwrap(),
    ));
    let instance = ScVal::ContractInstance(ScContractInstance {
        executable: ContractExecutable::Wasm(Hash([0xAA; 32])),
        storage: Some(
            ScMap::try_from(vec![ScMapEntry {
                key: sym("METADATA"),
                val: metadata,
            }])
            .unwrap(),
        ),
    });
    let entry = LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: ScAddress::Contract(ContractId(Hash([0xCC; 32]))),
            key: ScVal::LedgerKeyContractInstance,
            durability: ContractDataDurability::Persistent,
            val: instance,
        }),
        ext: LedgerEntryExt::V0,
    };
    let changes: LedgerEntryChanges = vec![LedgerEntryChange::Updated(entry)].try_into().unwrap();
    let tx_meta = TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes,
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    });

    let results = extract_ledger_entry_changes(&tx_meta, "abc123", 100, 1700000000);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].change_type, "updated");
    let md = results[0]
        .token_metadata
        .as_ref()
        .expect("instance METADATA should be extracted on `updated` too");
    assert_eq!(md.name.as_deref(), Some("liquidFi LP token"));
    assert_eq!(md.decimals, Some(7));
}

/// Every config-setting variant the compiled `stellar-xdr` defines must
/// map to a stored identifier. A variant landing in the `"unknown"`
/// catch-all means the hand-maintained table lags the protocol: the next
/// protocol bump should fail this test, not silently store `"unknown"`
/// (task 0434).
#[test]
fn config_setting_id_table_covers_every_variant() {
    let unmapped: Vec<&str> = ConfigSettingEntry::VARIANTS
        .iter()
        .filter(|id| config_setting_id_name(**id) == "unknown")
        .map(|id| id.name())
        .collect();
    assert!(
        unmapped.is_empty(),
        "config_setting_id_name lacks arms for: {unmapped:?}"
    );
}
