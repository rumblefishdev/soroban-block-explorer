//! Task 0620 — one-shot fill of `contract_instances` for every contract whose
//! instance last changed before the indexer started storing instances.
//!
//! Reads each contract's instance entry from Soroban RPC `getLedgerEntries`
//! (archived instances are returned too) and writes it versioned by the
//! entry's own last-modified ledger — never later than the ledger the indexer
//! stamps a change with — so a fill that races the live indexer never
//! replaces a newer instance.
//!
//! Run it only after the indexer that writes `contract_instances` is
//! deployed: an instance changed between this read and that deploy would be
//! written by neither.
//!
//! Idempotent: a re-run fetches only contracts still without a row.
//! `--dry-run` fetches without writing.

use std::collections::HashSet;
use std::time::Duration;

use clickhouse::Row;
use db_clickhouse::persist::rows::ContractInstanceRow;
use serde::Deserialize;
use stellar_xdr::{
    ContractDataDurability, ContractId, Hash, LedgerEntryData, LedgerKey, LedgerKeyContractData,
    Limits, ScAddress, ScVal, WriteXdr,
};
use tracing::{info, warn};

use crate::error::BackfillError;
use crate::rpc_snapshot::{LedgerEntryRecord, RpcClient, RpcError};
use crate::sink::Sink;
use crate::util::insert_rows;

/// Instances per `getLedgerEntries` call. An instance is ~400 B (p99 ~560 B,
/// max seen ~2.4 KB), so 100 keep one response small.
const INSTANCES_PER_CALL: usize = 100;

/// Seconds to wait before each retry when the RPC answers 429 (rate limit).
const RETRY_WAITS_SECS: [u64; 5] = [5, 10, 15, 20, 25];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContractInstanceBackfillStats {
    /// Known contracts without a `contract_instances` row.
    pub missing: u64,
    /// Instances the RPC returned.
    pub fetched: u64,
    /// Asked for but not returned — still missing after the run.
    pub not_returned: u64,
    /// Rows written (0 on a dry run).
    pub written: u64,
    pub dry_run: bool,
}

#[derive(Row, Deserialize)]
struct KnownContract {
    contract_id: String,
}

#[derive(Row, Deserialize)]
struct StoredInstance {
    contract: [u8; 32],
}

pub async fn execute(
    sink: &Sink,
    rpc_url: Option<&str>,
    dry_run: bool,
) -> Result<ContractInstanceBackfillStats, BackfillError> {
    let client = sink.client();
    let rpc_url = rpc_url.ok_or_else(|| {
        BackfillError::Incomplete(
            "contract_instance_backfill requires --soroban-rpc-url (or SOROBAN_RPC_URL)"
                .to_string(),
        )
    })?;

    let stored: HashSet<[u8; 32]> = client
        .query("SELECT DISTINCT contract FROM contract_instances")
        .fetch_all::<StoredInstance>()
        .await?
        .into_iter()
        .map(|r| r.contract)
        .collect();
    let mut missing: Vec<[u8; 32]> = Vec::new();
    for row in client
        .query("SELECT DISTINCT contract_id FROM soroban_contracts ORDER BY contract_id")
        .fetch_all::<KnownContract>()
        .await?
    {
        match stellar_strkey::Contract::from_string(&row.contract_id) {
            Ok(id) if !stored.contains(&id.0) => missing.push(id.0),
            Ok(_) => {}
            Err(e) => warn!(contract_id = %row.contract_id, "not a contract StrKey, skipped: {e}"),
        }
    }

    let mut stats = ContractInstanceBackfillStats {
        missing: missing.len() as u64,
        dry_run,
        ..Default::default()
    };
    info!(
        missing = stats.missing,
        "contract_instance_backfill: contracts without an instance row"
    );

    let rpc = RpcClient::new(rpc_url)?;
    for chunk in missing.chunks(INSTANCES_PER_CALL) {
        let keys: Vec<LedgerKey> = chunk.iter().map(|c| instance_key(*c)).collect();

        let mut rows = Vec::with_capacity(chunk.len());
        for record in fetch_with_retry(&rpc, &keys).await? {
            let LedgerEntryData::ContractData(ref data) = record.data else {
                continue;
            };
            let ScAddress::Contract(ContractId(Hash(contract))) = data.contract else {
                continue;
            };
            stats.fetched += 1;
            rows.push(ContractInstanceRow {
                contract,
                data_xdr: record.data.to_xdr(Limits::none()).map_err(|e| {
                    BackfillError::Incomplete(format!("instance does not re-encode: {e}"))
                })?,
                ledger: i64::from(record.last_modified_ledger),
            });
        }

        if !dry_run {
            insert_rows(client, "contract_instances", &rows).await?;
            stats.written += rows.len() as u64;
        }
        info!(
            fetched = stats.fetched,
            written = stats.written,
            "contract_instance_backfill: progress"
        );
    }

    stats.not_returned = stats.missing - stats.fetched;
    if stats.not_returned > 0 {
        warn!(
            not_returned = stats.not_returned,
            "contract_instance_backfill: some instances were not returned by the RPC — re-run to retry them"
        );
    }
    Ok(stats)
}

/// The ledger key of a contract's instance: its persistent contract-data
/// entry under the instance key.
fn instance_key(contract: [u8; 32]) -> LedgerKey {
    LedgerKey::ContractData(LedgerKeyContractData {
        contract: ScAddress::Contract(ContractId(Hash(contract))),
        key: ScVal::LedgerKeyContractInstance,
        durability: ContractDataDurability::Persistent,
    })
}

/// One `getLedgerEntries` call, retried on 429 after each wait in
/// [`RETRY_WAITS_SECS`]; any other error ends the run (a re-run resumes).
async fn fetch_with_retry(
    rpc: &RpcClient,
    keys: &[LedgerKey],
) -> Result<Vec<LedgerEntryRecord>, BackfillError> {
    for wait in RETRY_WAITS_SECS {
        match rpc.get_ledger_entries(keys).await {
            Err(RpcError::HttpStatus { status: 429, .. }) => {
                warn!(
                    wait_secs = wait,
                    "contract_instance_backfill: RPC rate limit, waiting"
                );
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
            result => return Ok(result?),
        }
    }
    Ok(rpc.get_ledger_entries(keys).await?)
}
