//! Task 0331 step 7 — one-shot RPC-snapshot seed of per-holder Soroban token
//! balances for bespoke type-3 tokens.
//!
//! ## Why this exists
//!
//! The live parser writes the unified `balances` table only when it observes a
//! `ContractData` `Balance(Address)` change. Holders that have not moved their
//! tokens since the parser was deployed are never seen, so `balances`
//! under-counts both supply (`sum(amount)`) and holders (`countIf(amount > 0)`).
//! This pass captures the CURRENT on-chain state once, directly from mainnet RPC,
//! making the table complete without reprocessing all historical ledgers.
//!
//! ## Mechanism
//!
//! 1. Per type-3 token, enumerate holder candidates = every `G…`/`C…` StrKey in
//!    the token's `soroban_events` topics + data (the event SET — the value comes
//!    from ledger STATE via RPC, never an event-fold; see task README DECISION
//!    2026-06-29).
//! 2. Build `Balance(Address)` persistent ledger keys and fetch them via
//!    `getLedgerEntries` (batched by the shared [`RpcClient`]); decode the
//!    bare-`i128` value with the same contract the live parser uses.
//! 3. Write `balances` rows (`ReplacingMergeTree`, keyed `(holder_id, asset_id)`)
//!    with version = the entry's last-modified ledger, so the live writer cleanly
//!    supersedes the seed once ingest catches up.
//!
//! Reads CURRENT state, so it is **freshness-immune to the indexer lag** — the
//! seed is correct at run time no matter how far behind live ingest is.
//! Idempotent: a re-run re-reads + re-upserts. `--dry-run` reports counts without
//! writing. CH-only — the unified `balances` model lives in ClickHouse, so a
//! Postgres target no-ops.

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::SorobanTokenSupplyRow;
use db_clickhouse::persist::stage::build_balance_rows;
use serde::Deserialize;
use tracing::info;
use xdr_parser::ExtractedSorobanBalance;

use crate::error::BackfillError;
use crate::util::insert_rows;
use crate::rpc_snapshot::{
    RpcClient, balance_ledger_key, decode_balance_entry, decode_total_supply, instance_ledger_key,
};
use crate::sink::Sink;

#[derive(Debug, Default, Clone, Copy)]
pub struct BalanceSeedStats {
    /// type-3 tokens with at least one holder candidate in their event stream.
    pub tokens: u64,
    /// `(token, holder)` balance keys requested from RPC.
    pub keys_requested: u64,
    /// Live `Balance` entries returned by RPC and decoded (bare-`i128` only).
    pub balances_decoded: u64,
    /// Tokens whose authoritative instance `TotalSupply` key was read.
    pub supply_read: u64,
    pub dry_run: bool,
}

/// One row of the candidate query: a token's C-StrKey + every holder StrKey
/// seen in its event stream.
#[derive(Row, Deserialize)]
struct SeedCandidate {
    token_strkey: String,
    holders: Vec<String>,
}

pub async fn execute(
    sink: &Sink,
    rpc_url: Option<&str>,
    dry_run: bool,
) -> Result<BalanceSeedStats, BackfillError> {
    // ClickHouse-only — the unified balances model is CH; PG retired.
    let Sink::Clickhouse(client) = sink else {
        return Err(BackfillError::Incomplete(
            "balance-seed is ClickHouse-only (unified balances model; PG retired)".to_string(),
        ));
    };

    let rpc_url = rpc_url.ok_or_else(|| {
        BackfillError::Incomplete(
            "balance_seed requires --soroban-rpc-url (or SOROBAN_RPC_URL)".to_string(),
        )
    })?;

    let candidates = read_seed_candidates(client).await?;
    let mut stats = BalanceSeedStats {
        tokens: candidates.len() as u64,
        dry_run,
        ..Default::default()
    };
    if candidates.is_empty() {
        info!("balance_seed: no type-3 token holder candidates found — nothing to do");
        return Ok(stats);
    }

    // Build every (token, holder) balance key. `decode_balance_entry` recovers
    // the token + holder from the returned entry itself, so request→response
    // order need not be tracked.
    let mut keys = Vec::new();
    for cand in &candidates {
        for holder in &cand.holders {
            if let Some(key) = balance_ledger_key(&cand.token_strkey, holder) {
                keys.push(key);
            }
        }
        // One instance key per token → its authoritative `TotalSupply` (where the
        // token stores it). Mixed into the same batch; the decoders route each
        // record by shape (Balance entry vs ContractInstance).
        if let Some(key) = instance_ledger_key(&cand.token_strkey) {
            keys.push(key);
        }
    }
    stats.keys_requested = keys.len() as u64;

    let rpc = RpcClient::new(rpc_url)?;
    let records = rpc.get_ledger_entries(&keys).await?;

    let mut balances: Vec<ExtractedSorobanBalance> = Vec::with_capacity(records.len());
    let mut supply_rows: Vec<SorobanTokenSupplyRow> = Vec::new();
    for rec in records {
        if let Some((contract_id, holder, balance)) = decode_balance_entry(&rec.data) {
            balances.push(ExtractedSorobanBalance {
                contract_id,
                holder,
                balance,
                ledger: rec.last_modified_ledger,
            });
        } else if let Some((token, total_supply)) = decode_total_supply(&rec.data) {
            // Authoritative supply for tokens that store the key; tokens without it
            // are simply absent here → the assets read falls back to Σ balances.
            supply_rows.push(SorobanTokenSupplyRow {
                asset_id: ids::asset_id(3, "", 0, ids::contract_id(&token)),
                total_supply,
                last_updated_ledger: i64::from(rec.last_modified_ledger),
            });
        }
    }
    stats.balances_decoded = balances.len() as u64;
    stats.supply_read = supply_rows.len() as u64;

    let balance_rows = build_balance_rows(&balances);

    if dry_run {
        info!(
            tokens = stats.tokens,
            keys_requested = stats.keys_requested,
            balances_decoded = stats.balances_decoded,
            supply_read = stats.supply_read,
            "balance_seed: dry-run, no rows written"
        );
        return Ok(stats);
    }

    insert_rows(client, "balances", &balance_rows).await?;
    insert_rows(client, "soroban_token_supply", &supply_rows).await?;
    info!(
        balances = balance_rows.len(),
        supply = supply_rows.len(),
        "balance_seed: wrote seed rows (RMT supersede; live ingest takes over on catch-up)"
    );
    Ok(stats)
}

/// Candidate query: for every type-3 token, its C-StrKey + the distinct set of
/// `G…`/`C…` holder StrKeys appearing in its event stream. StrKeys are matched
/// with a base32 regex (`[GC]` + 55 base32 chars) over the tagged-JSON
/// `topics_xdr`/`data_xdr`; over-matching is harmless (a non-holder key just
/// returns an absent entry, dropped). Scoped to the type-3 set so the scan reads
/// ~the bespoke-token event subset, not the full firehose.
async fn read_seed_candidates(
    client: &ClickhouseClient,
) -> Result<Vec<SeedCandidate>, clickhouse::error::Error> {
    client
        .query(
            "SELECT sc.contract_id AS token_strkey, \
                    arrayDistinct(arrayConcat( \
                        groupArrayArray(extractAll(e.topics_xdr, '([GC][A-Z2-7]{55})')), \
                        groupArrayArray(extractAll(e.data_xdr, '([GC][A-Z2-7]{55})')) \
                    )) AS holders \
             FROM soroban_events e \
             INNER JOIN soroban_contracts sc FINAL ON sc.id = e.contract_id \
             WHERE e.contract_id IN ( \
                 SELECT contract_id FROM assets WHERE asset_type = 3 AND contract_id != 0 \
             ) \
             GROUP BY sc.contract_id",
        )
        .fetch_all::<SeedCandidate>()
        .await
}
