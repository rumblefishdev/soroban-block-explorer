use super::*;

fn contract_data(contract: [u8; 32], key: ScVal) -> LedgerEntry {
    LedgerEntry {
        last_modified_ledger_seq: 100,
        data: LedgerEntryData::ContractData(ContractDataEntry {
            ext: ExtensionPoint::V0,
            contract: ScAddress::Contract(ContractId(Hash(contract))),
            key,
            durability: ContractDataDurability::Persistent,
            val: ScVal::ContractInstance(ScContractInstance {
                executable: ContractExecutable::Wasm(Hash([7; 32])),
                storage: None,
            }),
        }),
        ext: LedgerEntryExt::V0,
    }
}

fn meta(changes: Vec<LedgerEntryChange>) -> TransactionMeta {
    TransactionMeta::V3(TransactionMetaV3 {
        ext: ExtensionPoint::V0,
        tx_changes_before: changes.try_into().unwrap(),
        operations: VecM::default(),
        tx_changes_after: LedgerEntryChanges::default(),
        soroban_meta: None,
    })
}

#[test]
fn an_instance_is_kept_as_the_xdr_of_its_entry_data() {
    let entry = contract_data([1; 32], ScVal::LedgerKeyContractInstance);
    let instances =
        extract_contract_instances(&meta(vec![LedgerEntryChange::Created(entry.clone())]), 500);

    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].contract, [1; 32]);
    assert_eq!(instances[0].ledger_sequence, 500);
    let decoded = LedgerEntryData::from_xdr(&instances[0].data_xdr, Limits::none()).unwrap();
    assert_eq!(decoded, entry.data);
}

#[test]
fn created_updated_and_restored_instances_are_taken_state_is_not() {
    let entry = contract_data([2; 32], ScVal::LedgerKeyContractInstance);
    let changes = vec![
        LedgerEntryChange::State(entry.clone()),
        LedgerEntryChange::Updated(entry.clone()),
        LedgerEntryChange::Restored(entry.clone()),
        LedgerEntryChange::Created(entry),
    ];
    assert_eq!(extract_contract_instances(&meta(changes), 1).len(), 3);
}

#[test]
fn other_contract_data_is_not_an_instance() {
    let balance = contract_data([3; 32], ScVal::Symbol("Balance".try_into().unwrap()));
    let instances = extract_contract_instances(&meta(vec![LedgerEntryChange::Updated(balance)]), 1);
    assert!(instances.is_empty());
}
