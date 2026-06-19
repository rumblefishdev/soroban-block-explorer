//! NFT re-parse from `soroban_events` (task 0296 recovery, CH-direct).
//!
//! ## Why this exists
//!
//! Task 0296 taught `detect_nft_events` to recognise the **map**, **packed-vec**
//! and **consecutive_mint** NFT event shapes the old parser silently dropped.
//! Deploying the fixed parser only fixes *new* ledgers — the historical events
//! that were dropped never produced `nfts_pending` rows (e.g. `CARTUL5A…`:
//! 1,875 events in `soroban_events`, **0** rows in `nfts_pending`).
//!
//! Crucially the dropped events are **still in `soroban_events`** —
//! `topics_xdr` / `data_xdr` are stored as decoded typed-JSON, not raw XDR. So
//! recovery needs **no raw-S3 re-ingest**: re-run `detect_nft_events` over
//! `soroban_events` and write the candidates to the pending tables. The
//! downstream `contract_type_rebuild` + `nft_reclassify` then promote `Nft`
//! pending → hot and drop `Fungible`/`Token` — so this pass writes **PENDING
//! only**, never hot.
//!
//! ## Scope
//!
//! Re-parses **every** `mint`/`transfer`/`burn`/`consecutive_mint` event in the
//! range (case-insensitive) and lets `detect_nft_events` + its `is_fungible_map`
//! guard decide what is an NFT candidate — same authority as live ingest. This
//! re-derives Shape A (scalar `token_id`) too; those rows are already in
//! `nfts_pending`, so that work is redundant but idempotent (see below). A shape
//! pre-filter (`data_xdr LIKE '%token_id%'`) could cut the scan ~640,000× by
//! skipping the fungible-transfer firehose, but is intentionally omitted so the
//! pass is a complete superset of ingest and never relies on a heuristic.
//!
//! ## Parity with the indexer
//!
//! `event_order` and the `nfts_pending` owner/mint columns are produced by the
//! same folds the live indexer runs (see `parse_batch`), over events sorted into
//! application order, so re-parsed rows are byte-equivalent to ingest and dedup
//! cleanly against any rows already present.
//!
//! ## Idempotency
//!
//! `nfts_pending` / `nft_ownership_pending` are `ReplacingMergeTree`; re-running
//! over the same ledger range yields identically-keyed rows, collapsed on
//! merge. Safe to re-run / resume by range.

use std::collections::HashMap;

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use domain::ContractEventType;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, info};

use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{NftOwnershipPendingRow, NftPendingRow};
use xdr_parser::detect_nft_events;
use xdr_parser::state::{detect_nfts, extract_nft_ownership_events};
use xdr_parser::types::{EventSource, ExtractedEvent};

use crate::error::BackfillError;
use crate::sink::Sink;

/// Ledgers per read batch — bounds memory: the whole window is loaded as
/// `EventRow`s AND built into `ExtractedEvent`s for the batched parse. With no
/// shape pre-filter every mint/transfer/burn is pulled (~hundreds per ledger on
/// busy mainnet), so the window is deliberately small; raise/lower per range
/// density and available memory.
const LEDGER_BATCH: i64 = 2_000;

/// One `soroban_events` row, projected to what the NFT parser needs. The id
/// columns (`contract_id`, `transaction_id`) are already the hashed `i64`s used
/// by every other table; they ride through the parser as string carriers and
/// are decoded back in `parse_batch`.
#[derive(Debug, Row, Deserialize)]
struct EventRow {
    contract_id: i64,
    transaction_id: i64,
    ledger_sequence: i64,
    event_index: i16,
    topics_xdr: String,
    data_xdr: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NftReparseStats {
    /// `soroban_events` rows scanned (after the shape pre-filter).
    pub events_scanned: u64,
    /// NFT candidate rows recovered (would-be inserts on `--dry-run`).
    pub nft_pending_rows: u64,
    /// Ownership candidate rows recovered.
    pub ownership_pending_rows: u64,
    pub dry_run: bool,
}

/// Re-parse `soroban_events` in `[from_ledger, to_ledger]` and write recovered
/// NFT candidates to the pending tables. CH-only — PG target short-circuits.
pub async fn execute(
    sink: &Sink,
    from_ledger: u32,
    to_ledger: u32,
    dry_run: bool,
) -> Result<NftReparseStats, BackfillError> {
    let Sink::Clickhouse(client) = sink else {
        info!("nft_reparse: skipped (PG target — CH-direct recovery only)");
        return Ok(NftReparseStats::default());
    };

    let mut stats = NftReparseStats {
        dry_run,
        ..Default::default()
    };

    let (from, to) = (i64::from(from_ledger), i64::from(to_ledger));
    let mut lo = from;
    while lo <= to {
        let hi = (lo + LEDGER_BATCH - 1).min(to);

        let rows = read_events(client, lo, hi).await?;
        stats.events_scanned += rows.len() as u64;

        let (nft_rows, own_rows) = parse_batch(&rows)?;
        stats.nft_pending_rows += nft_rows.len() as u64;
        stats.ownership_pending_rows += own_rows.len() as u64;

        if !dry_run {
            insert_rows(client, "nfts_pending", &nft_rows).await?;
            insert_rows(client, "nft_ownership_pending", &own_rows).await?;
        }

        debug!(
            lo,
            hi,
            scanned = rows.len(),
            nft = nft_rows.len(),
            ownership = own_rows.len(),
            "nft_reparse: batch done"
        );
        lo = hi + 1;
    }

    info!(
        events_scanned = stats.events_scanned,
        nft_pending_rows = stats.nft_pending_rows,
        ownership_pending_rows = stats.ownership_pending_rows,
        dry_run,
        "nft_reparse: completed"
    );
    Ok(stats)
}

/// Read the NFT-candidate events in a ledger window, sorted into the indexer's
/// processing order so `parse_batch`'s `event_order` counter matches ingest.
///
/// - `lowerUTF8(signature) IN (...)` — `soroban_events.signature` is the raw
///   first-topic Symbol stored verbatim; real NFTs emit capitalised
///   `Mint`/`Transfer`/`Burn` (the parser lowercases before matching). A
///   case-sensitive `IN` would silently skip those.
/// - JOIN `transactions FINAL` for `application_order`: `transaction_id` is a
///   hash (`ids::transaction_id`), NOT monotonic with execution order, so it
///   can't order same-ledger events. `application_order` is the true intra-ledger
///   sequence; `FINAL` dedups the RMT so the join can't fan out.
/// - No shape pre-filter — every mint/transfer/burn/consecutive_mint is
///   re-parsed; `detect_nft_events` + the `is_fungible_map` guard decide what
///   becomes a candidate. (Re-parsing Shape-A scalars already in `nfts_pending`
///   is redundant but idempotent under RMT.)
async fn read_events(
    client: &ClickhouseClient,
    lo: i64,
    hi: i64,
) -> Result<Vec<EventRow>, BackfillError> {
    client
        .query(
            "SELECT e.contract_id, e.transaction_id, e.ledger_sequence, e.event_index, \
                    e.topics_xdr, e.data_xdr \
             FROM soroban_events AS e \
             INNER JOIN ( \
                 SELECT id, application_order FROM transactions FINAL \
                 WHERE ledger_sequence BETWEEN ? AND ? \
             ) AS t ON e.transaction_id = t.id \
             WHERE lowerUTF8(e.signature) IN ('mint', 'transfer', 'burn', 'consecutive_mint') \
               AND e.ledger_sequence BETWEEN ? AND ? \
             ORDER BY e.ledger_sequence, t.application_order, e.event_index",
        )
        .bind(lo)
        .bind(hi)
        .bind(lo)
        .bind(hi)
        .fetch_all::<EventRow>()
        .await
        .map_err(BackfillError::Ch)
}

/// Map a window of `soroban_events` rows (pre-sorted by `(ledger_sequence,
/// application_order, event_index)`) to pending rows by running the parser over
/// the **whole batch at once** — exactly how live ingest calls it. That makes
/// `extract_nft_ownership_events` assign the dense per-`(contract, token,
/// ledger)` `event_order` ordinal itself (no hand-rolled counter), and the
/// application-ordered input makes it match ingest, so re-parsed rows dedup
/// cleanly against any the indexer already wrote. `nfts_pending` is then folded
/// by `(contract, token)` exactly as `stage.rs` does (RMT version ties on a
/// same-ledger mint+transfer otherwise).
///
/// `transaction_id` / `contract_id` ride along as string carriers in the
/// parser's `transaction_hash` / `contract_id` fields so the batched output
/// maps back to the authoritative i64 ids without a second lookup.
fn parse_batch(
    rows: &[EventRow],
) -> Result<(Vec<NftPendingRow>, Vec<NftOwnershipPendingRow>), BackfillError> {
    let mut events = Vec::with_capacity(rows.len());
    for r in rows {
        let (Ok(topics), Ok(data)) = (
            serde_json::from_str::<Value>(&r.topics_xdr),
            serde_json::from_str::<Value>(&r.data_xdr),
        ) else {
            // A malformed JSON payload in CH is not worth aborting the whole
            // backfill for; skip and keep going (the row simply isn't recovered).
            continue;
        };
        events.push(ExtractedEvent {
            transaction_hash: r.transaction_id.to_string(),
            event_type: ContractEventType::Contract,
            source: EventSource::PerOp,
            contract_id: Some(r.contract_id.to_string()),
            topics,
            data,
            // `ledger_sequence` is the only field below that reaches an output
            // column (minted_at / current_owner_ledger / ownership ledger).
            // `event_index` is ignored by the parser, and `created_at` is neither
            // stored in `soroban_events` nor present on either pending row — so it
            // stays an inert 0.
            event_index: r.event_index.max(0) as u32,
            ledger_sequence: r.ledger_sequence.max(0) as u32,
            created_at: 0,
        });
    }

    // One batched parse — same call shape as the live indexer.
    let nft_events = detect_nft_events(&events);

    // Ownership rows: `event_order` straight from the parser; ids decoded from
    // the carriers.
    let mut own_out = Vec::new();
    for oe in extract_nft_ownership_events(&nft_events) {
        let (Ok(contract_id), Ok(transaction_id)) = (
            oe.contract_id.parse::<i64>(),
            oe.transaction_hash.parse::<i64>(),
        ) else {
            continue;
        };
        own_out.push(NftOwnershipPendingRow {
            contract_id,
            token_id: oe.token_id,
            ledger_sequence: i64::from(oe.ledger_sequence),
            event_order: oe.event_order as i16,
            transaction_id,
            owner_id: oe.owner_account.as_deref().map(ids::account_id),
            event_type: oe.event_type as i16,
        });
    }

    // State rows: fold by (contract, token) as stage.rs does.
    let mut nft_fold: HashMap<(i64, String), NftPendingRow> = HashMap::new();
    for nft in detect_nfts(&nft_events) {
        let Ok(contract_id) = nft.contract_id.parse::<i64>() else {
            continue;
        };
        let watermark = i64::from(nft.last_seen_ledger);
        let minted = nft.minted_at_ledger.map(i64::from);
        let owner = nft.owner_account.as_deref().map(ids::account_id);
        nft_fold
            .entry((contract_id, nft.token_id.clone()))
            .and_modify(|row| {
                // Latest event by watermark wins the owner (>= so a later
                // same-ledger event supersedes — matches stage.rs).
                if watermark >= row.current_owner_ledger {
                    row.current_owner_id = owner;
                    row.current_owner_ledger = watermark;
                }
                // Keep the earliest known mint ledger.
                row.minted_at_ledger = match (row.minted_at_ledger, minted) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, b) => b,
                };
            })
            .or_insert_with(|| NftPendingRow {
                contract_id,
                token_id: nft.token_id.clone(),
                collection_name: nft.collection_name.clone(),
                name: nft.name.clone(),
                media_url: nft.media_url.clone(),
                minted_at_ledger: minted,
                current_owner_id: owner,
                current_owner_ledger: watermark,
            });
    }

    Ok((nft_fold.into_values().collect(), own_out))
}

/// Batch-INSERT pending rows. No-op on an empty slice.
async fn insert_rows<T>(
    client: &ClickhouseClient,
    table: &str,
    rows: &[T],
) -> Result<(), BackfillError>
where
    T: clickhouse::Row + clickhouse::RowOwned + serde::Serialize,
{
    if rows.is_empty() {
        return Ok(());
    }
    let mut insert = client.insert::<T>(table).await.map_err(BackfillError::Ch)?;
    for row in rows {
        insert.write(row).await.map_err(BackfillError::Ch)?;
    }
    insert.end().await.map_err(BackfillError::Ch)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(contract_id: i64, topics: &str, data: &str) -> EventRow {
        EventRow {
            contract_id,
            transaction_id: 777,
            ledger_sequence: 62_952_436,
            event_index: 3,
            topics_xdr: topics.to_string(),
            data_xdr: data.to_string(),
        }
    }

    #[test]
    fn map_token_id_mint_maps_to_pending_rows() {
        // Real OZ/SEP-50 shape (CARTUL5A…): topics = [Symbol("mint"),
        // Address(to)], data = map{token_id: u32 133}. Old parser dropped this;
        // the re-parse must surface one pending + one ownership row.
        let rows = vec![row(
            12_345,
            r#"[{"type":"sym","value":"mint"},{"type":"address","value":"GAHWFCCYCDSSEYZRVV4446KBDP6X2JR2T56TG4OGANS3AS3NCILOCGU5"}]"#,
            r#"{"type":"map","value":[{"key":{"type":"sym","value":"token_id"},"value":{"type":"u32","value":133}}]}"#,
        )];

        let (nft, own) = parse_batch(&rows).expect("parse");
        assert_eq!(nft.len(), 1, "one nfts_pending row");
        assert_eq!(nft[0].contract_id, 12_345, "carried source contract_id");
        assert_eq!(nft[0].token_id, "133");
        assert!(nft[0].current_owner_id.is_some());

        assert_eq!(own.len(), 1, "one nft_ownership_pending row");
        assert_eq!(own[0].contract_id, 12_345);
        assert_eq!(own[0].token_id, "133");
        assert_eq!(own[0].transaction_id, 777, "carried source transaction_id");
        assert_eq!(
            own[0].event_order, 0,
            "first event of its (contract, token, ledger) triple"
        );
    }

    #[test]
    fn same_token_same_ledger_events_get_distinct_event_order() {
        // Regression for the dedup-collision data-loss bug: a mint then a
        // transfer of the SAME token in the SAME ledger (different txs) must get
        // event_order 0 and 1 — NOT both 0 (which would collapse on the
        // `(contract, token, ledger, event_order)` RMT key and lose one row).
        let mint = EventRow {
            contract_id: 12_345,
            transaction_id: 100,
            ledger_sequence: 999,
            event_index: 0,
            topics_xdr: r#"[{"type":"sym","value":"mint"},{"type":"address","value":"GAHWFCCYCDSSEYZRVV4446KBDP6X2JR2T56TG4OGANS3AS3NCILOCGU5"}]"#.into(),
            data_xdr: r#"{"type":"map","value":[{"key":{"type":"sym","value":"token_id"},"value":{"type":"u32","value":7}}]}"#.into(),
        };
        let transfer = EventRow {
            contract_id: 12_345,
            transaction_id: 200,
            ledger_sequence: 999,
            event_index: 0, // same per-tx index — would collide under the old code
            topics_xdr: r#"[{"type":"sym","value":"transfer"},{"type":"address","value":"GAHWFCCYCDSSEYZRVV4446KBDP6X2JR2T56TG4OGANS3AS3NCILOCGU5"},{"type":"address","value":"GBM3O32PEAYJGBUJN6TQENWBZCXE7RBQWWJYLLLTFG2QFQU2K5NOSMZI"}]"#.into(),
            data_xdr: r#"{"type":"map","value":[{"key":{"type":"sym","value":"token_id"},"value":{"type":"u32","value":7}}]}"#.into(),
        };

        let (nft, own) = parse_batch(&[mint, transfer]).expect("parse");
        assert_eq!(own.len(), 2, "both ownership events survive");
        let mut orders: Vec<i16> = own.iter().map(|r| r.event_order).collect();
        orders.sort();
        assert_eq!(orders, [0, 1], "distinct event_order — no dedup collision");

        // nfts_pending folds to one row; latest event (the transfer) owns it.
        assert_eq!(nft.len(), 1, "folded to one (contract, token) row");
        assert_eq!(nft[0].token_id, "7");
        assert_eq!(
            nft[0].current_owner_id,
            Some(ids::account_id(
                "GBM3O32PEAYJGBUJN6TQENWBZCXE7RBQWWJYLLLTFG2QFQU2K5NOSMZI"
            )),
            "transfer recipient is the current owner",
        );
        assert_eq!(nft[0].minted_at_ledger, Some(999), "mint ledger preserved");
    }

    #[test]
    fn consecutive_mint_range_expands() {
        // consecutive_mint vec[4, 6] → 3 mints (inclusive range), one pending +
        // one ownership row per token_id.
        let rows = vec![row(
            999,
            r#"[{"type":"sym","value":"consecutive_mint"},{"type":"address","value":"GBWHGYD5DFPQMJSUEEA77IT7YJ75PYQQFOCMP7HT5OIF2ULKJK22N4J4"}]"#,
            r#"{"type":"vec","value":[{"type":"u32","value":4},{"type":"u32","value":6}]}"#,
        )];

        let (nft, own) = parse_batch(&rows).expect("parse");
        assert_eq!(nft.len(), 3, "[4,6] inclusive = 3 mints");
        assert_eq!(own.len(), 3);
        let mut ids: Vec<&str> = nft.iter().map(|r| r.token_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["4", "5", "6"]);
    }

    #[test]
    fn fungible_map_transfer_produces_nothing() {
        // map{amount, to_muxed_id} with no token_id is a fungible (SAC) transfer
        // — the `is_fungible_map` guard suppresses it, so the re-parse emits
        // zero rows even though the SQL pre-filter let the map through.
        let rows = vec![row(
            555,
            r#"[{"type":"sym","value":"transfer"},{"type":"address","value":"GD2MH2SDYTIGBFBNH6ALKE7GNI5QBF2QAYKP7FLVCHLBVOMAQ65P5Z4A"},{"type":"address","value":"GBM3O32PEAYJGBUJN6TQENWBZCXE7RBQWWJYLLLTFG2QFQU2K5NOSMZI"}]"#,
            r#"{"type":"map","value":[{"key":{"type":"sym","value":"amount"},"value":{"type":"i128","value":"7943200000"}}]}"#,
        )];

        let (nft, own) = parse_batch(&rows).expect("parse");
        assert!(nft.is_empty(), "fungible map must not produce nft rows");
        assert!(own.is_empty());
    }
}
