//! Reading a token's `decimals`, `name` and `symbol` by running its own
//! functions (task 0620): the run over the program bytes and instances, and
//! the loading of whatever else a run asks for from `wasm_programs` and
//! `contract_instances`. Used per ledger by the indexer and by
//! `backfill-runner run` ([`apply`]), and once over all tokens by
//! `backfill-runner contract-metadata-backfill`.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use clickhouse::Row;
use contract_executor::{Ledger, ViewOutcome, call_view};
use serde::Deserialize;
use stellar_xdr::{
    ContractDataDurability, ContractExecutable, ContractId, Hash, LedgerEntry, LedgerEntryData,
    LedgerEntryExt, LedgerKey, LedgerKeyContractCode, LedgerKeyContractData, Limits, ReadXdr,
    ScAddress, ScVal,
};
use xdr_parser::ExtractedContractMetadata;
use xdr_parser::contract_instance::ExtractedContractInstance;
use xdr_parser::token_metadata::TokenMetadata;
use xdr_parser::types::ExtractedWasmProgram;

#[derive(Row, Deserialize)]
struct StoredInterface {
    wasm_hash: [u8; 32],
    /// The functions among `decimals`, `name`, `symbol` the program declares.
    declares: Vec<String>,
}

/// Token metadata read by running each token's functions takes the place of
/// what the storage-key read found for the same contract; a token the run does
/// not answer keeps the storage-key value. A database error costs only this
/// ledger's function values, never the ledger.
pub async fn apply(client: &clickhouse::Client, parsed: &mut crate::handler::process::ParseOutput) {
    let ledger = Ledger {
        sequence: parsed.ledger.sequence,
        timestamp: parsed.ledger.closed_at as u64,
        protocol_version: parsed.ledger.protocol_version,
        network_id: *crate::handler::process::network_id(),
    };
    let writes = match ledger_metadata_writes(
        client,
        &ledger,
        &parsed.contract_instances,
        &parsed.programs,
    )
    .await
    {
        Ok(writes) => writes,
        Err(e) => {
            tracing::warn!(
                ledger = ledger.sequence,
                "token metadata from functions skipped: {e}"
            );
            return;
        }
    };
    for write in writes {
        parsed
            .contract_metadata_writes
            .retain(|w| w.contract_id != write.contract_id);
        parsed.contract_metadata_writes.push(write);
    }
}

/// The metadata of every token whose instance changed in this ledger — a
/// deploy, a program upgrade or a storage write — read by running its
/// functions.
///
/// A token is a contract whose program declares `decimals`. Programs and
/// instances come from this ledger first (they may be new), then from
/// `wasm_programs` and `contract_instances`. A token whose run fails or needs
/// data we do not store (task 0633) is left out, so the caller keeps whatever
/// it had for it.
pub async fn ledger_metadata_writes(
    client: &clickhouse::Client,
    ledger: &Ledger,
    changed_instances: &[ExtractedContractInstance],
    new_programs: &[ExtractedWasmProgram],
) -> Result<Vec<ExtractedContractMetadata>, clickhouse::error::Error> {
    // The last instance of each contract this ledger; `changed_instances` is
    // in application order.
    let mut instances: HashMap<[u8; 32], Option<(Vec<u8>, i64)>> = HashMap::new();
    for i in changed_instances {
        instances.insert(
            i.contract,
            Some((i.data_xdr.clone(), i64::from(i.ledger_sequence))),
        );
    }
    let mut changed: Vec<([u8; 32], [u8; 32])> = Vec::new();
    for (contract, stored) in &instances {
        if let Some((data_xdr, _)) = stored
            && let Some(wasm_hash) = wasm_hash_of(data_xdr)
        {
            changed.push((*contract, wasm_hash));
        }
    }
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    changed.sort();

    let mut declares: HashMap<[u8; 32], Vec<String>> = HashMap::new();
    let mut programs: HashMap<[u8; 32], Option<Vec<u8>>> = HashMap::new();
    for p in new_programs {
        let Ok(bytes) = hex::decode(&p.wasm_hash) else {
            continue;
        };
        let Ok(hash) = <[u8; 32]>::try_from(bytes) else {
            continue;
        };
        let names = match &p.functions {
            Some(functions) => functions.iter().map(|f| f.name.clone()).collect(),
            None => Vec::new(),
        };
        declares.insert(hash, declared(names));
        programs.insert(hash, Some(p.code.clone()));
    }
    let unknown: Vec<String> = changed
        .iter()
        .filter(|(_, h)| !declares.contains_key(h))
        .map(|(_, h)| hex::encode(h))
        .collect();
    if !unknown.is_empty() {
        // Only the interface first: most changed instances are not tokens
        // (a farm contract rewrites its instance every ledger), and their
        // bytes are not needed. `toFixedString`: see `load_instances`.
        for row in client
            .query(
                "SELECT wasm_hash, \
                        arrayIntersect(['decimals', 'name', 'symbol'], \
                            arrayMap(f -> JSONExtractString(f, 'name'), \
                                     JSONExtractArrayRaw(metadata, 'functions'))) AS declares \
                 FROM wasm_programs \
                 WHERE wasm_hash IN (SELECT toFixedString(unhex(arrayJoin(?)), 32)) \
                 LIMIT 1 BY wasm_hash",
            )
            .bind(unknown)
            .fetch_all::<StoredInterface>()
            .await?
        {
            declares.insert(row.wasm_hash, row.declares);
        }
    }

    let mut writes = Vec::new();
    for (contract, wasm_hash) in changed {
        let Some(declared) = declares.get(&wasm_hash) else {
            continue;
        };
        if !declared.iter().any(|d| d == "decimals") {
            continue;
        }
        let answer = read_metadata(
            client,
            ledger,
            contract,
            declared,
            &mut programs,
            &mut instances,
        )
        .await?;
        if let Answer::Values {
            name,
            symbol,
            decimals,
        } = answer
        {
            writes.push(ExtractedContractMetadata {
                contract_id: ScAddress::Contract(ContractId(Hash(contract))).to_string(),
                metadata: TokenMetadata {
                    name,
                    symbol,
                    decimals,
                },
                ledger: ledger.sequence,
            });
        }
    }
    Ok(writes)
}

/// The program an instance runs, when it is a Wasm program of its own (not a
/// Stellar asset contract, not an external reference).
fn wasm_hash_of(data_xdr: &[u8]) -> Option<[u8; 32]> {
    let LedgerEntryData::ContractData(data) =
        LedgerEntryData::from_xdr(data_xdr, Limits::none()).ok()?
    else {
        return None;
    };
    let ScVal::ContractInstance(instance) = data.val else {
        return None;
    };
    match instance.executable {
        ContractExecutable::Wasm(Hash(hash)) => Some(hash),
        _ => None,
    }
}

/// The functions among `decimals`, `name`, `symbol` a program declares.
fn declared(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .filter(|n| FUNCTIONS.contains(&n.as_str()))
        .collect()
}

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
    declares: &[String],
    programs: &mut HashMap<[u8; 32], Option<Vec<u8>>>,
    instances: &mut HashMap<[u8; 32], Option<(Vec<u8>, i64)>>,
) -> Result<Answer, clickhouse::error::Error> {
    let mut entries: BTreeMap<LedgerKey, Option<LedgerEntry>> = BTreeMap::new();
    entries.insert(
        instance_key(contract),
        instance_entry(instances.get(&contract)),
    );
    // The program is not seeded: the run asks for it, and `load_entry` takes
    // it from `programs` or, when not there yet, from `wasm_programs`.
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
