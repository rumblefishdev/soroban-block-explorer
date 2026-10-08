//! Reading a contract's `decimals`, `name` and `symbol` by running its own
//! functions (task 0620): the only source of `soroban_contract_metadata`. The
//! run loads what else it asks for through [`crate::contract_state`]. Used per
//! ledger by the indexer and by `backfill-runner run`
//! ([`contract_metadata_writes`]), and once over all contracts by
//! `backfill-runner contract-metadata-backfill`.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use clickhouse::Row;
use contract_executor::{Ledger, ViewOutcome, call_view};
use db_clickhouse::persist::contract_metadata::{ExtractedContractMetadata, TokenMetadata};
use serde::Deserialize;
use stellar_xdr::{ContractId, Hash, ScAddress, ScVal};
use xdr_parser::contract_instance::ExtractedContractInstance;
use xdr_parser::types::ExtractedWasmProgram;

use crate::contract_state::{
    Entries, code_entry, code_key, instance_entry, instance_key, load_entry, wasm_hash_of,
};

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
