//! Task 0620 — `decimals`, `name` and `symbol` of every token contract, from
//! its own functions, into `soroban_contract_metadata`.
//!
//! For each non-SAC contract whose program declares `decimals`, the functions
//! its program declares among the three are run locally (`contract-executor`)
//! over the program bytes (`wasm_programs.code`) and the instance entry
//! (`contract_instances`). A run that asks for another contract's instance or
//! program gets it from the same tables and runs again. A run that asks for
//! anything else — persistent contract data — cannot be answered from the
//! database (task 0633): the contract is skipped and keeps whatever row it has.
//!
//! A row is written only when every declared function returned a value of its
//! standard type, versioned by the instance's ledger, so a later instance
//! change written by the indexer still wins. `--dry-run` computes and compares
//! with the stored rows without writing.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use clickhouse::Row;
use contract_executor::{Ledger, ViewOutcome, call_view};
use db_clickhouse::persist::rows::SorobanContractMetadataRow;
use serde::Deserialize;
use stellar_xdr::{
    ContractId, Hash, LedgerEntry, LedgerEntryData, LedgerEntryExt, LedgerKey,
    LedgerKeyContractCode, LedgerKeyContractData, Limits, ReadXdr, ScAddress, ScVal,
};
use tracing::{info, warn};

use crate::contract_instance_backfill::instance_key;
use crate::error::BackfillError;
use crate::sink::Sink;
use crate::util::insert_rows;

/// A run that keeps asking for entries is given up after this many rounds.
const MAX_ROUNDS: usize = 5;

/// Contracts per instance query and rows per insert.
const CHUNK: usize = 1_000;

/// The functions read, in the order they are run.
const FUNCTIONS: [&str; 3] = ["decimals", "name", "symbol"];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContractMetadataBackfillStats {
    /// Token contracts considered (program declares `decimals`).
    pub tokens: u64,
    /// No instance row yet (`contract-instance-backfill` not run for it).
    pub no_instance: u64,
    /// The program's bytes are not stored (`wasm-code-backfill` not run).
    pub no_program: u64,
    /// A function asked for persistent contract data we do not store (0633).
    pub needs_contract_data: u64,
    /// A function failed or returned a value of the wrong type.
    pub failed: u64,
    /// Answered, and equal to the stored row.
    pub same: u64,
    /// Answered, and different from the stored row.
    pub different: u64,
    /// Answered, and no row was stored before.
    pub new: u64,
    /// Rows written (0 on a dry run).
    pub written: u64,
    pub dry_run: bool,
}

#[derive(Row, Deserialize)]
struct TokenRow {
    contract_id: String,
    wasm_hash: [u8; 32],
    declares: Vec<String>,
}

#[derive(Row, Deserialize)]
struct ProgramRow {
    wasm_hash: [u8; 32],
    #[serde(with = "serde_bytes")]
    code: Vec<u8>,
}

#[derive(Row, Deserialize)]
struct InstanceRow {
    contract: [u8; 32],
    #[serde(with = "serde_bytes")]
    latest_xdr: Vec<u8>,
    latest_ledger: i64,
}

#[derive(Row, Deserialize)]
struct LatestLedger {
    sequence: i64,
    closed_at: u32,
    protocol_version: i32,
}

#[derive(Row, Deserialize, PartialEq)]
struct StoredMetadata {
    contract_id: String,
    name: Option<String>,
    symbol: Option<String>,
    decimals: Option<u32>,
}

/// What reading one contract's three values came to.
enum Answer {
    Values {
        name: Option<String>,
        symbol: Option<String>,
        decimals: Option<u32>,
    },
    NeedsContractData,
    Failed,
}

pub async fn execute(
    sink: &Sink,
    dry_run: bool,
) -> Result<ContractMetadataBackfillStats, BackfillError> {
    let client = sink.client();

    let latest = client
        .query(
            "SELECT sequence, toUInt32(toUnixTimestamp(closed_at)) AS closed_at, protocol_version \
             FROM ledgers ORDER BY sequence DESC LIMIT 1",
        )
        .fetch_one::<LatestLedger>()
        .await?;
    // The process's network (`STELLAR_NETWORK_PASSPHRASE`), mainnet when unset.
    let network_id = match xdr_parser::sac::net_id() {
        Some(id) => *id,
        None => xdr_parser::sac::network_id(xdr_parser::MAINNET_PASSPHRASE),
    };
    let ledger = Ledger {
        sequence: latest.sequence as u32,
        timestamp: u64::from(latest.closed_at),
        protocol_version: latest.protocol_version as u32,
        network_id,
    };

    // Every non-SAC contract whose program declares `decimals`, with the
    // functions its program declares among the three.
    let tokens = client
        .query(
            "SELECT sc.contract_id AS contract_id, \
                    assumeNotNull(sc.wasm_hash) AS wasm_hash, \
                    arrayIntersect(['decimals', 'name', 'symbol'], \
                        arrayMap(f -> JSONExtractString(f, 'name'), \
                                 JSONExtractArrayRaw(wp.metadata, 'functions'))) AS declares \
             FROM soroban_contracts AS sc FINAL \
             INNER JOIN (SELECT wasm_hash, metadata FROM wasm_programs FINAL) AS wp \
                 ON wp.wasm_hash = assumeNotNull(sc.wasm_hash) \
             WHERE NOT sc.is_sac AND sc.wasm_hash IS NOT NULL AND has(declares, 'decimals') \
             ORDER BY contract_id",
        )
        .fetch_all::<TokenRow>()
        .await?;

    // The token programs' bytes, once each. `toFixedString`: see
    // `load_instances`.
    let mut token_programs: Vec<String> = tokens.iter().map(|t| hex::encode(t.wasm_hash)).collect();
    token_programs.sort();
    token_programs.dedup();
    let mut programs: HashMap<[u8; 32], Option<Vec<u8>>> = HashMap::new();
    for row in client
        .query(
            "SELECT wasm_hash, code FROM wasm_programs \
             WHERE code != '' \
               AND wasm_hash IN (SELECT toFixedString(unhex(arrayJoin(?)), 32)) \
             LIMIT 1 BY wasm_hash",
        )
        .bind(token_programs)
        .fetch_all::<ProgramRow>()
        .await?
    {
        programs.insert(row.wasm_hash, Some(row.code));
    }

    let mut instances: HashMap<[u8; 32], Option<(Vec<u8>, i64)>> = HashMap::new();
    let ids: Vec<[u8; 32]> = tokens
        .iter()
        .filter_map(|t| stellar_strkey::Contract::from_string(&t.contract_id).ok())
        .map(|c| c.0)
        .collect();
    for chunk in ids.chunks(CHUNK) {
        load_instances(client, chunk, &mut instances).await?;
    }

    let stored: HashMap<String, StoredMetadata> = client
        .query(
            "SELECT contract_id, name, symbol, decimals \
             FROM soroban_contract_metadata FINAL",
        )
        .fetch_all::<StoredMetadata>()
        .await?
        .into_iter()
        .map(|m| (m.contract_id.clone(), m))
        .collect();

    let mut stats = ContractMetadataBackfillStats {
        tokens: tokens.len() as u64,
        dry_run,
        ..Default::default()
    };
    info!(
        tokens = stats.tokens,
        ledger = ledger.sequence,
        "contract_metadata_backfill: start"
    );

    let mut rows = Vec::new();
    for token in &tokens {
        let Ok(id) = stellar_strkey::Contract::from_string(&token.contract_id) else {
            warn!(contract_id = %token.contract_id, "not a contract StrKey, skipped");
            continue;
        };
        let Some(Some((_, instance_ledger))) = instances.get(&id.0).cloned() else {
            stats.no_instance += 1;
            continue;
        };
        if !matches!(programs.get(&token.wasm_hash), Some(Some(_))) {
            stats.no_program += 1;
            continue;
        }
        let answer = read_metadata(
            client,
            &ledger,
            id.0,
            token.wasm_hash,
            &token.declares,
            &mut programs,
            &mut instances,
        )
        .await?;
        let (name, symbol, decimals) = match answer {
            Answer::Values {
                name,
                symbol,
                decimals,
            } => (name, symbol, decimals),
            Answer::NeedsContractData => {
                stats.needs_contract_data += 1;
                continue;
            }
            Answer::Failed => {
                stats.failed += 1;
                continue;
            }
        };
        let computed = StoredMetadata {
            contract_id: token.contract_id.clone(),
            name: name.clone(),
            symbol: symbol.clone(),
            decimals,
        };
        match stored.get(&token.contract_id) {
            None => stats.new += 1,
            Some(before) if *before == computed => stats.same += 1,
            Some(before) => {
                stats.different += 1;
                info!(
                    contract_id = %token.contract_id,
                    stored = ?(&before.name, &before.symbol, before.decimals),
                    computed = ?(&name, &symbol, decimals),
                    "contract_metadata_backfill: differs from the stored row"
                );
            }
        }
        rows.push(SorobanContractMetadataRow {
            contract_id: token.contract_id.clone(),
            name,
            symbol,
            decimals,
            version: instance_ledger,
        });
    }

    if !dry_run {
        for chunk in rows.chunks(CHUNK) {
            insert_rows(client, "soroban_contract_metadata", chunk).await?;
            stats.written += chunk.len() as u64;
        }
    }
    Ok(stats)
}

/// Run the declared functions among `decimals`, `name`, `symbol` for one
/// contract, adding the programs and instances a run asks for.
async fn read_metadata(
    client: &clickhouse::Client,
    ledger: &Ledger,
    contract: [u8; 32],
    wasm_hash: [u8; 32],
    declares: &[String],
    programs: &mut HashMap<[u8; 32], Option<Vec<u8>>>,
    instances: &mut HashMap<[u8; 32], Option<(Vec<u8>, i64)>>,
) -> Result<Answer, BackfillError> {
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
) -> Result<Option<Option<LedgerEntry>>, BackfillError> {
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

async fn load_instances(
    client: &clickhouse::Client,
    contracts: &[[u8; 32]],
    instances: &mut HashMap<[u8; 32], Option<(Vec<u8>, i64)>>,
) -> Result<(), BackfillError> {
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
