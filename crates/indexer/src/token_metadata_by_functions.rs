//! Reading a token's `decimals`, `name` and `symbol` by running its own
//! functions (task 0620): the run over the program bytes and instances, and
//! the loading of whatever else a run asks for from `wasm_programs` and
//! `contract_instances`. Used by `backfill-runner contract-metadata-backfill`.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use clickhouse::Row;
use contract_executor::{Ledger, ViewOutcome, call_view};
use serde::Deserialize;
use stellar_xdr::{
    ContractDataDurability, ContractId, Hash, LedgerEntry, LedgerEntryData, LedgerEntryExt,
    LedgerKey, LedgerKeyContractCode, LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal,
};

/// A run that keeps asking for entries is given up after this many rounds.
const MAX_ROUNDS: usize = 5;

/// The functions read, in the order they are run.
const FUNCTIONS: [&str; 3] = ["decimals", "name", "symbol"];

#[derive(Row, Deserialize)]
pub struct ProgramRow {
    pub wasm_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    pub code: Vec<u8>,
}

#[derive(Row, Deserialize)]
struct InstanceRow {
    contract: [u8; 32],
    #[serde(with = "serde_bytes")]
    latest_xdr: Vec<u8>,
    latest_ledger: i64,
}

/// What reading one contract's three values came to.
pub enum Answer {
    Values {
        name: Option<String>,
        symbol: Option<String>,
        decimals: Option<u32>,
    },
    NeedsContractData,
    Failed,
}

/// Run the declared functions among `decimals`, `name`, `symbol` for one
/// contract, adding the programs and instances a run asks for.
pub async fn read_metadata(
    client: &clickhouse::Client,
    ledger: &Ledger,
    contract: [u8; 32],
    wasm_hash: [u8; 32],
    declares: &[String],
    programs: &mut HashMap<[u8; 32], Option<Vec<u8>>>,
    instances: &mut HashMap<[u8; 32], Option<(Vec<u8>, i64)>>,
) -> Result<Answer, clickhouse::error::Error> {
    let mut entries: BTreeMap<LedgerKey, Option<LedgerEntry>> = BTreeMap::new();
    entries.insert(
        instance_key(contract),
        instance_entry(instances.get(&contract)),
    );
    entries.insert(
        code_key(wasm_hash),
        code_entry(wasm_hash, programs.get(&wasm_hash)),
    );
    let mut entries = Rc::new(entries);

    let (mut name, mut symbol, mut decimals) = (None, None, None);
    for function in FUNCTIONS {
        if !declares.iter().any(|d| d == function) {
            continue;
        }
        let mut value = None;
        for _ in 0..MAX_ROUNDS {
            match call_view(entries.clone(), ledger, contract, function) {
                ViewOutcome::Value(v) => {
                    value = Some(v);
                    break;
                }
                ViewOutcome::Failed(_) => return Ok(Answer::Failed),
                ViewOutcome::Missing(keys) => {
                    let map = Rc::make_mut(&mut entries);
                    for key in keys {
                        let Some(entry) = load_entry(client, &key, programs, instances).await?
                        else {
                            return Ok(Answer::NeedsContractData);
                        };
                        map.insert(key, entry);
                    }
                }
            }
        }
        let Some(value) = value else {
            return Ok(Answer::Failed);
        };
        match (function, value) {
            ("decimals", ScVal::U32(d)) => decimals = Some(d),
            ("name", ScVal::String(s)) => match String::from_utf8(s.0.to_vec()) {
                Ok(text) => name = Some(text),
                Err(_) => return Ok(Answer::Failed),
            },
            ("symbol", ScVal::String(s)) => match String::from_utf8(s.0.to_vec()) {
                Ok(text) => symbol = Some(text),
                Err(_) => return Ok(Answer::Failed),
            },
            _ => return Ok(Answer::Failed),
        }
    }
    Ok(Answer::Values {
        name,
        symbol,
        decimals,
    })
}

/// The entry for a key a run asked for, from the stored programs and
/// instances: `Some(Some(_))` found, `Some(None)` known not to exist, `None`
/// not something the database holds.
async fn load_entry(
    client: &clickhouse::Client,
    key: &LedgerKey,
    programs: &mut HashMap<[u8; 32], Option<Vec<u8>>>,
    instances: &mut HashMap<[u8; 32], Option<(Vec<u8>, i64)>>,
) -> Result<Option<Option<LedgerEntry>>, clickhouse::error::Error> {
    match key {
        LedgerKey::ContractCode(LedgerKeyContractCode { hash: Hash(hash) }) => {
            if !programs.contains_key(hash) {
                let row = client
                    .query("SELECT wasm_hash, code FROM wasm_programs WHERE wasm_hash = unhex(?) AND code != '' LIMIT 1")
                    .bind(hex::encode(hash))
                    .fetch_optional::<ProgramRow>()
                    .await?;
                programs.insert(*hash, row.map(|r| r.code));
            }
            Ok(Some(code_entry(*hash, programs.get(hash))))
        }
        LedgerKey::ContractData(LedgerKeyContractData {
            contract: ScAddress::Contract(ContractId(Hash(contract))),
            key: ScVal::LedgerKeyContractInstance,
            ..
        }) => {
            if !instances.contains_key(contract) {
                load_instances(client, &[*contract], instances).await?;
            }
            Ok(Some(instance_entry(instances.get(contract))))
        }
        _ => Ok(None),
    }
}

pub async fn load_instances(
    client: &clickhouse::Client,
    contracts: &[[u8; 32]],
    instances: &mut HashMap<[u8; 32], Option<(Vec<u8>, i64)>>,
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
        .fetch_all::<InstanceRow>()
        .await?;
    for contract in contracts {
        instances.insert(*contract, None);
    }
    for row in rows {
        instances.insert(row.contract, Some((row.latest_xdr, row.latest_ledger)));
    }
    Ok(())
}

fn code_key(wasm_hash: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractCode(LedgerKeyContractCode {
        hash: Hash(wasm_hash),
    })
}

/// A stored instance as a ledger entry; `None` when the contract has none.
fn instance_entry(stored: Option<&Option<(Vec<u8>, i64)>>) -> Option<LedgerEntry> {
    let (data_xdr, ledger) = stored?.as_ref()?;
    Some(LedgerEntry {
        last_modified_ledger_seq: *ledger as u32,
        data: LedgerEntryData::from_xdr(data_xdr, Limits::none()).ok()?,
        ext: LedgerEntryExt::V0,
    })
}

/// A stored program as a ledger entry; `None` when its bytes are not stored.
fn code_entry(hash: [u8; 32], stored: Option<&Option<Vec<u8>>>) -> Option<LedgerEntry> {
    let code = stored?.as_ref()?;
    Some(LedgerEntry {
        last_modified_ledger_seq: 0,
        data: LedgerEntryData::ContractCode(stellar_xdr::ContractCodeEntry {
            ext: stellar_xdr::ContractCodeEntryExt::V0,
            hash: Hash(hash),
            code: code.clone().try_into().ok()?,
        }),
        ext: LedgerEntryExt::V0,
    })
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
