//! Task 0424 — fill the history of `nft_ownership_changes{,_pending}` from what
//! ClickHouse already holds, no S3.
//!
//! **Temporary** (thread 314 A): it reads the old ownership tables, so it
//! cannot run after their drop; task 0424's PR 4 deletes it with them.
//!
//! The contract events of every collection in the old ownership tables are
//! read back from `soroban_events` — topics and data are stored as the
//! parser's own JSON — and run through the indexer's own NFT extraction
//! (`detect_nft_events` → `extract_nft_ownership_events`). Each change is
//! located as the live writer locates it: the transaction position from the
//! row (`soroban_events.application_order`), the operation and the event from
//! the rpc id. Routing follows each contract's current verdict, as
//! `nft-reclassify` does: `Nft` → hot, `Token` / `Fungible` → dropped,
//! anything else → pending.
//!
//! Re-running is a no-op: the rows are deterministic and the
//! ReplacingMergeTree collapses them. Ledgers the new indexer already wrote
//! collapse the same way.

use std::collections::HashMap;

use clickhouse::Row;
use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::NftOwnershipChangeRow;
use domain::ContractEventType;
use serde::Deserialize;
use tracing::{info, warn};
use xdr_parser::types::{EventSource, ExtractedEvent};
use xdr_parser::{EventId, detect_nft_events, extract_nft_ownership_events};

use crate::error::BackfillError;
use crate::sink::Sink;

/// `ContractType` discriminants (`crates/domain/src/enums/contract_type.rs`).
const CONTRACT_TYPE_TOKEN: i16 = 0;
const CONTRACT_TYPE_NFT: i16 = 2;
const CONTRACT_TYPE_FUNGIBLE: i16 = 3;

const HOT: &str = "nft_ownership_changes";
const PENDING: &str = "nft_ownership_changes_pending";

/// One contract event of a collection, with its contract's StrKey and verdict.
#[derive(Debug, Clone, Row, Deserialize)]
pub(crate) struct EventRow {
    pub contract: String,
    pub contract_type: Option<i16>,
    pub ledger_sequence: i64,
    pub transaction_index: u32,
    pub operation_index: u16,
    pub event_index: u32,
    pub application_order: i16,
    pub topics_xdr: String,
    pub data_xdr: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NftOwnershipFillStats {
    pub events: usize,
    pub hot: usize,
    pub pending: usize,
    pub dropped: usize,
    /// The gate against the old tables, position aside: changes only they
    /// hold, and changes only the fill found.
    pub only_old: usize,
    pub only_new: usize,
    pub dry_run: bool,
}

/// A change without its location — what the old tables can be compared on.
type Change = (i64, String, i64, Option<i64>, i16);

#[derive(Debug, Row, Deserialize)]
struct OldRow {
    contract_id: i64,
    token_id: String,
    ledger_sequence: i64,
    owner_id: Option<i64>,
    event_type: i16,
}

/// Multiset difference, hot and pending together (routing follows today's
/// verdicts, the old tables the verdicts at write time): `(only_old, only_new)`.
pub(crate) fn diff(old: Vec<Change>, new: Vec<Change>) -> (Vec<Change>, Vec<Change>) {
    let mut count: HashMap<Change, i64> = HashMap::new();
    for c in old {
        *count.entry(c).or_default() -= 1;
    }
    for c in new {
        *count.entry(c).or_default() += 1;
    }
    let (mut only_old, mut only_new) = (Vec::new(), Vec::new());
    for (c, n) in count {
        let side = if n < 0 { &mut only_old } else { &mut only_new };
        side.extend(std::iter::repeat_n(c, n.unsigned_abs() as usize));
    }
    only_old.sort();
    only_new.sort();
    (only_old, only_new)
}

pub async fn execute(sink: &Sink, dry_run: bool) -> Result<NftOwnershipFillStats, BackfillError> {
    let client = sink.client();
    // Every collection the old tables hold; `LIMIT 1 BY` drops unmerged
    // duplicates of an event.
    let events: Vec<EventRow> = client
        .query(
            "WITH targets AS (
                 SELECT contract_id FROM nft_ownership
                 UNION DISTINCT SELECT contract_id FROM nft_ownership_pending)
             SELECT c.contract_id AS contract, c.contract_type AS contract_type,
                    e.ledger_sequence AS ledger_sequence,
                    e.transaction_index AS transaction_index,
                    e.operation_index AS operation_index,
                    e.event_index AS event_index,
                    e.application_order AS application_order,
                    e.topics_xdr AS topics_xdr, e.data_xdr AS data_xdr
             FROM soroban_events AS e
             INNER JOIN (SELECT id, contract_id, contract_type FROM soroban_contracts FINAL
                         WHERE id IN (SELECT contract_id FROM targets)) AS c
               ON c.id = e.contract_id
             WHERE e.event_type = 1 AND e.contract_id IN (SELECT contract_id FROM targets)
             ORDER BY e.contract_id, e.ledger_sequence, e.transaction_index,
                      e.operation_index, e.event_index
             LIMIT 1 BY e.contract_id, e.ledger_sequence, e.transaction_index,
                        e.operation_index, e.event_index",
        )
        .fetch_all()
        .await?;

    let (hot, pending, dropped) = changes(&events)?;

    // The gate: the same changes as the old tables, located or not. `FINAL`
    // collapses their unmerged copies; both tables are small.
    let old: Vec<OldRow> = client
        .query(
            "SELECT contract_id, token_id, ledger_sequence, owner_id, event_type
             FROM nft_ownership FINAL
             UNION ALL
             SELECT contract_id, token_id, ledger_sequence, owner_id, event_type
             FROM nft_ownership_pending FINAL",
        )
        .fetch_all()
        .await?;
    let key = |r: &NftOwnershipChangeRow| -> Change {
        (
            r.contract_id,
            r.token_id.clone(),
            r.ledger_sequence,
            r.owner_id,
            r.event_type,
        )
    };
    let (only_old, only_new) = diff(
        old.into_iter()
            .map(|r| {
                (
                    r.contract_id,
                    r.token_id,
                    r.ledger_sequence,
                    r.owner_id,
                    r.event_type,
                )
            })
            .collect(),
        hot.iter().chain(&pending).map(key).collect(),
    );
    for c in only_old.iter().take(10) {
        warn!(change = ?c, "nft_ownership_fill: only in the old tables");
    }
    for c in only_new.iter().take(10) {
        warn!(change = ?c, "nft_ownership_fill: only in the fill");
    }

    let stats = NftOwnershipFillStats {
        events: events.len(),
        hot: hot.len(),
        pending: pending.len(),
        dropped,
        only_old: only_old.len(),
        only_new: only_new.len(),
        dry_run,
    };
    if !dry_run {
        for (table, rows) in [(HOT, &hot), (PENDING, &pending)] {
            let mut insert = client.insert::<NftOwnershipChangeRow>(table).await?;
            for row in rows {
                insert.write(row).await?;
            }
            insert.end().await?;
            info!(table, rows = rows.len(), "nft_ownership_fill: written");
        }
    }
    Ok(stats)
}

/// The ownership changes the indexer's extraction finds in `events`, split
/// hot / pending by each contract's verdict, plus how many were dropped.
pub(crate) fn changes(
    events: &[EventRow],
) -> Result<
    (
        Vec<NftOwnershipChangeRow>,
        Vec<NftOwnershipChangeRow>,
        usize,
    ),
    BackfillError,
> {
    let mut position: HashMap<EventId, i16> = HashMap::with_capacity(events.len());
    let mut verdict: HashMap<&str, Option<i16>> = HashMap::new();
    let mut extracted = Vec::with_capacity(events.len());
    for e in events {
        let ledger_sequence = u32::try_from(e.ledger_sequence)
            .map_err(|_| BackfillError::Incomplete(format!("ledger {}", e.ledger_sequence)))?;
        let id = EventId {
            ledger_sequence,
            transaction_index: e.transaction_index,
            operation_index: e.operation_index,
            event_index: e.event_index,
        };
        position.insert(id, e.application_order);
        verdict.insert(&e.contract, e.contract_type);
        let json = |s: &str| {
            serde_json::from_str(s).map_err(|err| {
                BackfillError::Incomplete(format!("stored event JSON at {id:?}: {err}"))
            })
        };
        extracted.push(ExtractedEvent {
            transaction_hash: String::new(),
            event_type: ContractEventType::Contract,
            source: EventSource::PerOp,
            contract_id: Some(e.contract.clone()),
            topics: json(&e.topics_xdr)?,
            data: json(&e.data_xdr)?,
            position_in_tx: 0,
            op_index: Some(u32::from(e.operation_index)),
            event_pos_in_op: Some(e.event_index),
            stage: None,
            event_id: Some(id),
            ledger_sequence,
            created_at: 0,
        });
    }

    // ponytail: the whole population (31,088 events, 7 MiB, 2026-09-28) goes
    // through one call; batch per contract if the collections grow by orders
    // of magnitude.
    let net_id = xdr_parser::sac::network_id(xdr_parser::sac::MAINNET_PASSPHRASE);
    let nft_events = detect_nft_events(&extracted, &net_id);
    let (mut hot, mut pending, mut dropped) = (Vec::new(), Vec::new(), 0);
    for ev in extract_nft_ownership_events(&nft_events) {
        // Every event above was built with its id, so both lookups hold.
        let Some((id, &application_order)) = ev
            .event_id
            .and_then(|id| position.get(&id).map(|p| (id, p)))
        else {
            return Err(BackfillError::Incomplete(format!(
                "NFT change without its source event: {ev:?}"
            )));
        };
        let row = NftOwnershipChangeRow {
            contract_id: ids::contract_id(&ev.contract_id),
            token_id: ev.token_id,
            ledger_sequence: i64::from(ev.ledger_sequence),
            application_order,
            operation_index: id.operation_index,
            event_index: id.event_index,
            owner_id: ev.owner_account.as_deref().map(ids::account_id),
            event_type: ev.event_type as i16,
        };
        match verdict.get(ev.contract_id.as_str()).copied().flatten() {
            Some(CONTRACT_TYPE_NFT) => hot.push(row),
            Some(CONTRACT_TYPE_TOKEN | CONTRACT_TYPE_FUNGIBLE) => dropped += 1,
            _ => pending.push(row),
        }
    }
    // Deterministic output, and one row per key as the RMT would keep it.
    for rows in [&mut hot, &mut pending] {
        rows.sort();
        rows.dedup();
    }
    Ok((hot, pending, dropped))
}

#[cfg(test)]
mod tests;
