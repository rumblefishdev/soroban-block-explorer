//! The stored state a contract run asks for (task 0620): a program's bytes
//! from `wasm_programs`, a contract's instance from `contract_instances`, each
//! turned into the ledger entry the host expects, and the ledger keys they are
//! asked for under. Nothing here is specific to tokens.

use std::collections::BTreeMap;

use clickhouse::Row;
use serde::Deserialize;
use stellar_xdr::{
    ContractCodeEntry, ContractCodeEntryExt, ContractDataDurability, ContractExecutable,
    ContractId, Hash, LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey,
    LedgerKeyContractCode, LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal,
};

/// The ledger entries a run may ask for, keyed as the host asks for them: the
/// entry, or `None` for one known not to exist. It grows as runs ask for more.
pub type Entries = BTreeMap<LedgerKey, Option<LedgerEntry>>;

#[derive(Row, Deserialize)]
struct StoredProgram {
    #[serde(with = "serde_bytes")]
    code: Vec<u8>,
}

#[derive(Row, Deserialize)]
struct StoredInstance {
    contract: [u8; 32],
    #[serde(with = "serde_bytes")]
    latest_xdr: Vec<u8>,
    latest_ledger: i64,
}

/// Add the entry a run asked for to `entries`, from `wasm_programs` or
/// `contract_instances` — found, or known not to exist. `false` when it is
/// something the database does not hold (persistent contract data).
pub(crate) async fn load_entry(
    client: &clickhouse::Client,
    key: &LedgerKey,
    entries: &mut Entries,
) -> Result<bool, clickhouse::error::Error> {
    match key {
        LedgerKey::ContractCode(LedgerKeyContractCode { hash: Hash(hash) }) => {
            let row = client
                .query("SELECT code FROM wasm_programs WHERE wasm_hash = unhex(?) AND code != '' LIMIT 1")
                .bind(hex::encode(hash))
                .fetch_optional::<StoredProgram>()
                .await?;
            let entry = match row {
                Some(row) => code_entry(*hash, row.code),
                None => None,
            };
            entries.insert(key.clone(), entry);
            Ok(true)
        }
        LedgerKey::ContractData(LedgerKeyContractData {
            contract: ScAddress::Contract(ContractId(Hash(contract))),
            key: ScVal::LedgerKeyContractInstance,
            ..
        }) => {
            load_instances(client, &[*contract], entries).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Add the stored instances of `contracts` to `entries`; a contract without
/// one is added as known not to exist.
pub async fn load_instances(
    client: &clickhouse::Client,
    contracts: &[[u8; 32]],
    entries: &mut Entries,
) -> Result<(), clickhouse::error::Error> {
    let hexes: Vec<String> = contracts.iter().map(hex::encode).collect();
    let rows = client
        .query(
            // Aliases differ from the columns: an alias named like a column
            // shadows it inside the other aggregate (ClickHouse code 184).
            // `toFixedString`: without it the subquery's `String` loses a
            // trailing zero byte against the `FixedString` column, and every
            // contract id ending in 0x00 goes unmatched.
            "SELECT contract, argMax(data_xdr, ledger) AS latest_xdr, max(ledger) AS latest_ledger \
             FROM contract_instances \
             WHERE contract IN (SELECT toFixedString(unhex(arrayJoin(?)), 32)) \
             GROUP BY contract",
        )
        .bind(hexes)
        .fetch_all::<StoredInstance>()
        .await?;
    for contract in contracts {
        entries.insert(instance_key(*contract), None);
    }
    for row in rows {
        entries.insert(
            instance_key(row.contract),
            instance_entry(&row.latest_xdr, row.latest_ledger),
        );
    }
    Ok(())
}

/// An instance as a ledger entry, from its `LedgerEntryData` XDR.
pub fn instance_entry(data_xdr: &[u8], ledger: i64) -> Option<LedgerEntry> {
    Some(LedgerEntry {
        last_modified_ledger_seq: ledger as u32,
        data: LedgerEntryData::from_xdr(data_xdr, Limits::none()).ok()?,
        ext: LedgerEntryExt::V0,
    })
}

/// A program as a ledger entry.
pub fn code_entry(hash: [u8; 32], code: Vec<u8>) -> Option<LedgerEntry> {
    Some(LedgerEntry {
        last_modified_ledger_seq: 0,
        data: LedgerEntryData::ContractCode(ContractCodeEntry {
            ext: ContractCodeEntryExt::V0,
            hash: Hash(hash),
            code: code.try_into().ok()?,
        }),
        ext: LedgerEntryExt::V0,
    })
}

/// The program an instance runs, when it is a Wasm program of its own (not a
/// Stellar asset contract, not an external reference).
pub(crate) fn wasm_hash_of(entry: &LedgerEntry) -> Option<[u8; 32]> {
    let LedgerEntryData::ContractData(data) = &entry.data else {
        return None;
    };
    let ScVal::ContractInstance(instance) = &data.val else {
        return None;
    };
    match instance.executable {
        ContractExecutable::Wasm(Hash(hash)) => Some(hash),
        _ => None,
    }
}

/// The ledger key of a contract's instance: its persistent contract-data
/// entry under the instance key.
pub fn instance_key(contract: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(ContractId(Hash(contract))),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

/// The ledger key of a program.
pub fn code_key(wasm_hash: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(wasm_hash),
    })
}
