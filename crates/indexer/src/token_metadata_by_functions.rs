//! Reading a contract's `decimals`, `name` and `symbol` by running its own
//! functions (task 0620): the only source of `soroban_contract_metadata`.
//! The run over the program bytes and instances, and the loading of whatever
//! else a run asks for from `wasm_programs` and `contract_instances`. Used per
//! ledger by the indexer and by `backfill-runner run`
//! ([`contract_metadata_writes`]), and once over all contracts by
//! `backfill-runner contract-metadata-backfill`.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use clickhouse::Row;
use contract_executor::{Ledger, ViewOutcome, call_view};
use serde::Deserialize;
use stellar_xdr::{
    ContractCodeEntry, ContractCodeEntryExt, ContractDataDurability, ContractExecutable,
    ContractId, Hash, LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey,
    LedgerKeyContractCode, LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal,
};
use xdr_parser::ExtractedContractMetadata;
use xdr_parser::contract_instance::ExtractedContractInstance;
use xdr_parser::token_metadata::TokenMetadata;
use xdr_parser::types::ExtractedWasmProgram;

/// The ledger entries a run may ask for, keyed as the host asks for them: the
/// entry, or `None` for one known not to exist. It grows as runs ask for more.
pub type Entries = BTreeMap<LedgerKey, Option<LedgerEntry>>;

/// Whether a program's contracts get metadata: a SEP-41 token declares
/// `decimals`, a SEP-50 NFT declares `name` and `symbol`. A program declaring
/// only `name` or only `symbol` is left out — on mainnet those are mostly
/// contracts of other kinds (a reward claim, a pool) with a `name` of their own.
pub fn has_metadata(declares: &[String]) -> bool {
    let decimals = declares.iter().any(|d| d == "decimals");
    let name = declares.iter().any(|d| d == "name");
    let symbol = declares.iter().any(|d| d == "symbol");
    decimals || (name && symbol)
}

/// What reading one contract's three values came to.
pub enum Answer {
    Metadata(TokenMetadata),
    /// A function asked for persistent contract data, which is not stored
    /// (task 0633).
    NeedsContractData,
    /// A function failed, or returned a value of the wrong type.
    Failed,
}

/// A run that keeps asking for entries is given up after this many rounds.
const MAX_ROUNDS: usize = 5;

/// The functions read, in the order they are run.
const FUNCTIONS: [&str; 3] = ["decimals", "name", "symbol"];

#[derive(Row, Deserialize)]
struct StoredInterface {
    wasm_hash: [u8; 32],
    /// The functions among `decimals`, `name`, `symbol` the program declares.
    declares: Vec<String>,
}

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

/// The `soroban_contract_metadata` writes of a parsed ledger.
pub async fn contract_metadata_writes(
    client: &clickhouse::Client,
    parsed: &crate::handler::process::ParseOutput,
) -> Result<Vec<ExtractedContractMetadata>, clickhouse::error::Error> {
    let ledger = Ledger {
        sequence: parsed.ledger.sequence,
        timestamp: parsed.ledger.closed_at as u64,
        protocol_version: parsed.ledger.protocol_version,
        network_id: *crate::handler::process::network_id(),
    };
    ledger_metadata_writes(
        client,
        &ledger,
        &parsed.contract_instances,
        &parsed.programs,
    )
    .await
}

/// The metadata of every contract whose instance changed in this ledger — a
/// deploy, a program upgrade or a storage write — and whose program
/// [`has_metadata`], read by running its functions.
///
/// Programs and instances come from this ledger first (they may be new), then
/// from `wasm_programs` and `contract_instances`. A contract whose run fails or
/// needs data we do not store (task 0633) gets no write, so its stored row, if
/// any, stays.
pub async fn ledger_metadata_writes(
    client: &clickhouse::Client,
    ledger: &Ledger,
    changed_instances: &[ExtractedContractInstance],
    new_programs: &[ExtractedWasmProgram],
) -> Result<Vec<ExtractedContractMetadata>, clickhouse::error::Error> {
    // This ledger's instances, the last of each contract (`changed_instances`
    // is in application order), and the programs they run.
    let mut entries = Entries::new();
    let mut changed: BTreeMap<[u8; 32], [u8; 32]> = BTreeMap::new();
    for i in changed_instances {
        let Some(entry) = instance_entry(&i.data_xdr, i64::from(i.ledger_sequence)) else {
            continue;
        };
        if let Some(wasm_hash) = wasm_hash_of(&entry) {
            changed.insert(i.contract, wasm_hash);
        }
        entries.insert(instance_key(i.contract), Some(entry));
    }
    if changed.is_empty() {
        return Ok(Vec::new());
    }

    let mut declares: HashMap<[u8; 32], Vec<String>> = HashMap::new();
    for p in new_programs {
        let Ok(bytes) = hex::decode(&p.wasm_hash) else {
            continue;
        };
        let Ok(hash) = <[u8; 32]>::try_from(bytes) else {
            continue;
        };
        let names: Vec<String> = match &p.functions {
            Some(functions) => functions.iter().map(|f| f.name.clone()).collect(),
            None => Vec::new(),
        };
        let declared = names
            .into_iter()
            .filter(|n| FUNCTIONS.contains(&n.as_str()))
            .collect();
        declares.insert(hash, declared);
        entries.insert(code_key(hash), code_entry(hash, p.code.clone()));
    }
    // Many contracts share a program; each hash once keeps the query short.
    let mut unknown: Vec<String> = changed
        .values()
        .filter(|h| !declares.contains_key(*h))
        .map(hex::encode)
        .collect();
    unknown.sort();
    unknown.dedup();
    if !unknown.is_empty() {
        // Only the interface first: most changed instances have no metadata
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

    let mut entries = Rc::new(entries);
    let mut writes = Vec::new();
    for (contract, wasm_hash) in changed {
        let Some(declared) = declares.get(&wasm_hash) else {
            continue;
        };
        if !has_metadata(declared) {
            continue;
        }
        if let Answer::Metadata(metadata) =
            read_metadata(client, ledger, contract, declared, &mut entries).await?
        {
            writes.push(ExtractedContractMetadata {
                contract_id: ScAddress::Contract(ContractId(Hash(contract))).to_string(),
                metadata,
                ledger: ledger.sequence,
            });
        }
    }
    Ok(writes)
}

/// Run the declared functions among `decimals`, `name`, `symbol` for one
/// contract over `entries`, loading into it the programs and instances a run
/// asks for. The contract's own instance must already be in `entries`.
pub async fn read_metadata(
    client: &clickhouse::Client,
    ledger: &Ledger,
    contract: [u8; 32],
    declares: &[String],
    entries: &mut Rc<Entries>,
) -> Result<Answer, clickhouse::error::Error> {
    let mut metadata = TokenMetadata {
        name: None,
        symbol: None,
        decimals: None,
    };
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
                    for key in keys {
                        if !load_entry(client, &key, Rc::make_mut(entries)).await? {
                            return Ok(Answer::NeedsContractData);
                        }
                    }
                }
            }
        }
        let Some(value) = value else {
            return Ok(Answer::Failed);
        };
        match (function, value) {
            ("decimals", ScVal::U32(d)) => metadata.decimals = Some(d),
            ("name", ScVal::String(s)) => match String::from_utf8(s.0.to_vec()) {
                Ok(text) => metadata.name = Some(text),
                Err(_) => return Ok(Answer::Failed),
            },
            ("symbol", ScVal::String(s)) => match String::from_utf8(s.0.to_vec()) {
                Ok(text) => metadata.symbol = Some(text),
                Err(_) => return Ok(Answer::Failed),
            },
            _ => return Ok(Answer::Failed),
        }
    }
    Ok(Answer::Metadata(metadata))
}

/// Add the entry a run asked for to `entries`, from `wasm_programs` or
/// `contract_instances` — found, or known not to exist. `false` when it is
/// something the database does not hold (persistent contract data).
async fn load_entry(
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
fn wasm_hash_of(entry: &LedgerEntry) -> Option<[u8; 32]> {
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
