//! Task 0620 — `decimals`, `name` and `symbol` of every token and NFT
//! contract, from its own functions, into `soroban_contract_metadata`.
//!
//! For each non-SAC contract whose program `has_metadata`, the functions
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

use std::collections::HashMap;

use clickhouse::Row;
use contract_executor::Ledger;
use db_clickhouse::persist::rows::SorobanContractMetadataRow;
use serde::Deserialize;
use tracing::{info, warn};

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::util::insert_rows;
use indexer::contract_metadata::{Answer, has_metadata, read_metadata};
use indexer::contract_state::{Entries, code_entry, code_key, instance_key, load_instances};

/// Contracts per instance query and rows per insert.
const CHUNK: usize = 1_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContractMetadataBackfillStats {
    /// Contracts considered (program `has_metadata`).
    pub contracts: u64,
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
struct ContractRow {
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

    // Every non-SAC contract whose program `has_metadata`, with the functions
    // its program declares among the three.
    let mut contracts = client
        .query(
            "SELECT sc.contract_id AS contract_id, \
                    assumeNotNull(sc.wasm_hash) AS wasm_hash, \
                    arrayIntersect(['decimals', 'name', 'symbol'], \
                        arrayMap(f -> JSONExtractString(f, 'name'), \
                                 JSONExtractArrayRaw(wp.metadata, 'functions'))) AS declares \
             FROM soroban_contracts AS sc FINAL \
             INNER JOIN (SELECT wasm_hash, metadata FROM wasm_programs FINAL) AS wp \
                 ON wp.wasm_hash = assumeNotNull(sc.wasm_hash) \
             WHERE NOT sc.is_sac AND sc.wasm_hash IS NOT NULL AND notEmpty(declares) \
             ORDER BY contract_id",
        )
        .fetch_all::<ContractRow>()
        .await?;
    contracts.retain(|c| has_metadata(&c.declares));

    // Their programs' bytes, once each. `toFixedString`: see
    // `load_instances`.
    let mut programs: Vec<String> = contracts.iter().map(|c| hex::encode(c.wasm_hash)).collect();
    programs.sort();
    programs.dedup();
    // Every contract's program and instance, loaded once; a run that asks for
    // another contract's adds it here.
    let mut entries = Entries::new();
    for row in client
        .query(
            "SELECT wasm_hash, code FROM wasm_programs \
             WHERE code != '' \
               AND wasm_hash IN (SELECT toFixedString(unhex(arrayJoin(?)), 32)) \
             LIMIT 1 BY wasm_hash",
        )
        .bind(programs)
        .fetch_all::<ProgramRow>()
        .await?
    {
        entries.insert(code_key(row.wasm_hash), code_entry(row.wasm_hash, row.code));
    }
    let ids: Vec<[u8; 32]> = contracts
        .iter()
        .filter_map(|c| stellar_strkey::Contract::from_string(&c.contract_id).ok())
        .map(|c| c.0)
        .collect();
    for chunk in ids.chunks(CHUNK) {
        load_instances(client, chunk, &mut entries).await?;
    }
    let mut entries = std::rc::Rc::new(entries);

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
        contracts: contracts.len() as u64,
        dry_run,
        ..Default::default()
    };
    info!(
        contracts = stats.contracts,
        ledger = ledger.sequence,
        "contract_metadata_backfill: start"
    );

    let mut rows = Vec::new();
    for contract in &contracts {
        let Ok(id) = stellar_strkey::Contract::from_string(&contract.contract_id) else {
            warn!(contract_id = %contract.contract_id, "not a contract StrKey, skipped");
            continue;
        };
        let Some(Some(instance)) = entries.get(&instance_key(id.0)) else {
            stats.no_instance += 1;
            continue;
        };
        let instance_ledger = i64::from(instance.last_modified_ledger_seq);
        if !matches!(entries.get(&code_key(contract.wasm_hash)), Some(Some(_))) {
            stats.no_program += 1;
            continue;
        }
        let metadata =
            match read_metadata(client, &ledger, id.0, &contract.declares, &mut entries).await? {
                Answer::Metadata(metadata) => metadata,
                Answer::NeedsContractData => {
                    stats.needs_contract_data += 1;
                    continue;
                }
                Answer::Failed => {
                    stats.failed += 1;
                    continue;
                }
            };
        let (name, symbol, decimals) = (metadata.name, metadata.symbol, metadata.decimals);
        let computed = StoredMetadata {
            contract_id: contract.contract_id.clone(),
            name: name.clone(),
            symbol: symbol.clone(),
            decimals,
        };
        match stored.get(&contract.contract_id) {
            None => stats.new += 1,
            Some(before) if *before == computed => stats.same += 1,
            Some(before) => {
                stats.different += 1;
                info!(
                    contract_id = %contract.contract_id,
                    stored = ?(&before.name, &before.symbol, before.decimals),
                    computed = ?(&name, &symbol, decimals),
                    "contract_metadata_backfill: differs from the stored row"
                );
            }
        }
        rows.push(SorobanContractMetadataRow {
            contract_id: contract.contract_id.clone(),
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
