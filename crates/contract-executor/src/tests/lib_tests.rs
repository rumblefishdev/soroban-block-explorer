//! A real mainnet token run locally: `CDDNHP3B…X4RV`, one of 2,276 contracts
//! of the most common token program. Expected values are what Soroban RPC
//! `simulateTransaction` returned for the same contract on 2026-10-07.

use super::*;
use soroban_env_host::xdr::{
    ContractDataDurability, LedgerEntryExt, LedgerKeyContractCode, LedgerKeyContractData, ScString,
};

const CONTRACT: [u8; 32] = [
    0xc6, 0xd3, 0xbf, 0x61, 0xc4, 0xc4, 0xad, 0x1f, 0x0f, 0x6f, 0x56, 0x93, 0x98, 0xbf, 0xad, 0x5c,
    0xa7, 0x63, 0x72, 0xf1, 0x0a, 0x07, 0x5c, 0x7c, 0x5d, 0x6b, 0xcd, 0xf4, 0x91, 0x22, 0xa5, 0xcb,
];
const PROGRAM: [u8; 32] = [
    0x0a, 0x41, 0x41, 0x1f, 0x72, 0x50, 0xa7, 0x8d, 0xa1, 0x5d, 0x6c, 0xd6, 0xcf, 0x78, 0x08, 0x6e,
    0x19, 0xd9, 0x8b, 0xc9, 0x17, 0xdd, 0x0a, 0x51, 0xe0, 0x3a, 0x34, 0xf5, 0xab, 0x96, 0x97, 0x81,
];

const LEDGER: Ledger = Ledger {
    sequence: 64_818_615,
    timestamp: 1_791_380_000,
    protocol_version: 29,
    network_id: [0; 32],
};

fn entry(base64: &str) -> LedgerEntry {
    LedgerEntry {
        last_modified_ledger_seq: 1,
        data: LedgerEntryData::from_xdr_base64(base64.trim(), Limits::none()).unwrap(),
        ext: LedgerEntryExt::V0,
    }
}

fn instance_key() -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(ContractId(Hash(CONTRACT))),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

fn code_key() -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(PROGRAM),
    })
}

fn both() -> BTreeMap<LedgerKey, Option<LedgerEntry>> {
    BTreeMap::from([
        (
            instance_key(),
            Some(entry(include_str!("../../tests/fixtures/instance.xdr.b64"))),
        ),
        (
            code_key(),
            Some(entry(include_str!("../../tests/fixtures/code.xdr.b64"))),
        ),
    ])
}

fn text(s: &str) -> ScVal {
    ScVal::String(ScString(s.as_bytes().to_vec().try_into().unwrap()))
}

#[test]
fn a_real_token_answers_like_the_network() {
    let entries = Rc::new(both());
    assert_eq!(
        call_view(entries.clone(), &LEDGER, CONTRACT, "decimals"),
        ViewOutcome::Value(ScVal::U32(0))
    );
    assert_eq!(
        call_view(entries.clone(), &LEDGER, CONTRACT, "symbol"),
        ViewOutcome::Value(text("SMOL"))
    );
    assert_eq!(
        call_view(entries, &LEDGER, CONTRACT, "name"),
        ViewOutcome::Value(text("We\u{2019}re Gonna Make the Best of It"))
    );
}

#[test]
fn an_entry_not_supplied_is_reported_missing() {
    let mut entries = both();
    entries.remove(&code_key());
    assert_eq!(
        call_view(Rc::new(entries), &LEDGER, CONTRACT, "decimals"),
        ViewOutcome::Missing(vec![code_key()])
    );
}

#[test]
fn an_entry_known_to_be_absent_is_a_failure_not_a_miss() {
    let mut entries = both();
    entries.insert(code_key(), None);
    assert!(matches!(
        call_view(Rc::new(entries), &LEDGER, CONTRACT, "decimals"),
        ViewOutcome::Failed(_)
    ));
}

#[test]
fn a_function_the_contract_lacks_fails() {
    assert!(matches!(
        call_view(Rc::new(both()), &LEDGER, CONTRACT, "no_such_function"),
        ViewOutcome::Failed(_)
    ));
}
