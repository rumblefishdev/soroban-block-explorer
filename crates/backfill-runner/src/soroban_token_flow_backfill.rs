//! Task 0383 — one-shot backfill of Soroban token-event flow into the two
//! presence indexes (`transaction_participants` + `operation_asset_appearances`),
//! re-derived from already-ingested `soroban_events`.
//!
//! ## Why this exists
//!
//! The live ingest hook (`stage.rs`) registers SEP-41 / CAP-67 token-event
//! participants (`from`/`to`) and SAC-classic asset presence going forward, but
//! historical events predate parts of it: asset presence from events never
//! existed for ANY verb, and mint/burn/clawback participants were added by 0383.
//! The signal is **already in `soroban_events`** — `topics_xdr` holds the
//! decoded typed-JSON topics — so recovery needs **no raw-S3 re-ingest**.
//!
//! ## Mechanism
//!
//! Scan `soroban_events` for the four token signatures in ledger windows —
//! restricted via PREWHERE to **Soroban-context txs** (`has_soroban`; see
//! `read_events`) — and decode each survivor's `topics_xdr` with the SAME
//! [`derive_token_event`] the live indexer runs. The surrogate hashing is
//! `cityhash_102_128` (see
//! [`db_clickhouse::persist::ids`]), deliberately NOT CH SQL's `cityHash64()`, so
//! the decode MUST run in Rust to produce rows that dedup against live data — a
//! pure `INSERT … SELECT` could not.
//!
//! The `has_soroban` restriction is not an optimisation-of-convenience: Protocol
//! 23 makes every classic payment emit a SAC transfer event, but those txs are
//! already covered by the 0359 op path (measured 99.4% of transfer events,
//! 670/670 participant coverage). Scoping to `has_soroban` keeps exactly the
//! net-new contract-internal flows and drops the redundant classic firehose.
//!
//! ## Idempotency
//!
//! Both targets are `ReplacingMergeTree`; re-inserting identically-keyed rows
//! collapses on merge. Runs the whole ingested history (range auto-detected, no
//! args); safe to re-run and to overlap with live ingest. `--dry-run` counts the
//! would-be rows without writing.

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, info};

use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{OperationAssetAppearanceRow, TransactionParticipantRow};
use db_clickhouse::persist::stage::derive_token_event;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::util::insert_rows;

/// Ledgers per read batch — the whole window's survivors are `fetch_all`ed.
/// `read_events` uses a PREWHERE `has_soroban` filter, so only the ~1-in-5
/// Soroban-context rows materialize `topics_xdr`; memory scales with survivors,
/// not the raw transfer firehose, so the window can be generous. Tunable per
/// range density / read-quota budget.
const LEDGER_BATCH: i64 = 5_000;

#[derive(Debug, Default, Clone, Copy)]
pub struct SorobanTokenFlowStats {
    /// `soroban_events` rows scanned (the four token signatures in range).
    pub events_scanned: u64,
    /// `transaction_participants` rows derived (would-be inserts on `--dry-run`).
    pub participant_rows: u64,
    /// `operation_asset_appearances` rows derived (SAC-classic/native only).
    pub asset_rows: u64,
    pub dry_run: bool,
}

/// One `soroban_events` row projected to what the presence derivation needs.
/// `data_xdr` is intentionally not read — the amount it carries is not stored
/// under the presence model (task 0383), so skipping it cuts read I/O on the
/// (large, ZSTD) payload columns.
#[derive(Debug, Row, Deserialize)]
struct EventRow {
    transaction_id: i64,
    ledger_sequence: i64,
    topics_xdr: String,
}

/// Re-derive token-flow presence rows for the ENTIRE ingested history and write
/// them to the two indexes. The ledger range is auto-detected from
/// `soroban_events` (no operator args) and processed in internal `LEDGER_BATCH`
/// windows. CH-only. Idempotent.
pub async fn execute(sink: &Sink, dry_run: bool) -> Result<SorobanTokenFlowStats, BackfillError> {
    let client = sink.client();

    let mut stats = SorobanTokenFlowStats {
        dry_run,
        ..Default::default()
    };

    let Some((from, to)) = ledger_bounds(client).await? else {
        info!("soroban_token_flow: soroban_events is empty — nothing to do");
        return Ok(stats);
    };
    let mut lo = from;
    while lo <= to {
        let hi = (lo + LEDGER_BATCH - 1).min(to);

        let rows = read_events(client, lo, hi).await?;
        stats.events_scanned += rows.len() as u64;

        let (part_rows, asset_rows) = build_rows(&rows);
        stats.participant_rows += part_rows.len() as u64;
        stats.asset_rows += asset_rows.len() as u64;

        if !dry_run {
            insert_rows(client, "transaction_participants", &part_rows).await?;
            insert_rows(client, "operation_asset_appearances", &asset_rows).await?;
        }

        debug!(
            lo,
            hi,
            scanned = rows.len(),
            participants = part_rows.len(),
            assets = asset_rows.len(),
            "soroban_token_flow: batch done"
        );
        lo = hi + 1;
    }

    info!(
        events_scanned = stats.events_scanned,
        participant_rows = stats.participant_rows,
        asset_rows = stats.asset_rows,
        dry_run,
        "soroban_token_flow: completed"
    );
    Ok(stats)
}

/// Min/max `ledger_sequence` across `soroban_events`, or `None` when empty.
/// Cheap even full-table: `ledger_sequence` is a sort-key column, so CH answers
/// min/max from per-part metadata without reading the heavy payload columns.
async fn ledger_bounds(client: &ClickhouseClient) -> Result<Option<(i64, i64)>, BackfillError> {
    #[derive(Row, Deserialize)]
    struct Bounds {
        lo: i64,
        hi: i64,
        n: u64,
    }
    let b = client
        .query(
            "SELECT min(ledger_sequence) AS lo, max(ledger_sequence) AS hi, count() AS n \
             FROM soroban_events",
        )
        .fetch_one::<Bounds>()
        .await
        .map_err(BackfillError::Ch)?;
    Ok((b.n > 0).then_some((b.lo, b.hi)))
}

/// Read the Soroban-context token events in a ledger window (see the module
/// header for why the scope is `has_soroban`).
///
/// The filter lives in **PREWHERE** so ClickHouse reads only the small columns
/// (`signature`, `ledger_sequence`, `transaction_id`) to decide membership, and
/// materializes the heavy ZSTD `topics_xdr` **only for survivors** — the
/// quota-dominant cost. Measured on a 500-ledger head window: 236,257 sig-matched
/// rows but only 43,504 (18%) are `has_soroban`, so PREWHERE cuts the topics read
/// ~5×. `transaction_id IN (subquery)` is set-membership (not a JOIN), so an
/// unmerged-RMT duplicate `id` in `transactions` cannot fan out a row.
/// `lowerUTF8(signature)` mirrors the parser's case-insensitive verb match.
async fn read_events(
    client: &ClickhouseClient,
    lo: i64,
    hi: i64,
) -> Result<Vec<EventRow>, BackfillError> {
    client
        .query(
            "SELECT transaction_id, ledger_sequence, topics_xdr \
             FROM soroban_events \
             PREWHERE lowerUTF8(signature) IN ('transfer', 'mint', 'burn', 'clawback') \
               AND ledger_sequence BETWEEN ? AND ? \
               AND transaction_id IN ( \
                   SELECT id FROM transactions \
                   WHERE ledger_sequence BETWEEN ? AND ? AND has_soroban = true \
               )",
        )
        .bind(lo)
        .bind(hi)
        .bind(lo)
        .bind(hi)
        .fetch_all::<EventRow>()
        .await
        .map_err(BackfillError::Ch)
}

/// Decode a window of events into presence rows using the SAME
/// [`derive_token_event`] the live indexer runs, so the surrogates match and the
/// rows dedup against live data. A malformed JSON payload is skipped (never
/// aborts the whole backfill for one bad row).
fn build_rows(
    rows: &[EventRow],
) -> (
    Vec<TransactionParticipantRow>,
    Vec<OperationAssetAppearanceRow>,
) {
    let mut part = Vec::new();
    let mut asset = Vec::new();
    for r in rows {
        let Ok(topics) = serde_json::from_str::<Value>(&r.topics_xdr) else {
            continue;
        };
        // `None` emitting id: this presence backfill does not resolve bespoke
        // token identity (task 0393 live path does) — a bespoke `Contract` event
        // resolves to `asset_id = None` and is dropped, as under the original 0383.
        let Some(derived) = derive_token_event(&topics, None) else {
            continue;
        };
        for strkey in derived.participant_strkeys {
            part.push(TransactionParticipantRow {
                account_id: ids::account_id(&strkey),
                ledger_sequence: r.ledger_sequence,
                transaction_id: r.transaction_id,
            });
        }
        if let Some(asset_id) = derived.asset_id {
            asset.push(OperationAssetAppearanceRow {
                asset_id,
                ledger_sequence: r.ledger_sequence,
                transaction_id: r.transaction_id,
                // Presence-only backfill: value is computed by live ingest / the
                // full S3 re-ingest, not here. WARNING: on the version-less RMT a
                // NULL row can win the merge over a live-computed value and blank
                // the column — do NOT run this after `net_settled` lands on prod
                // (re-ingest is the history mechanism). See task 0393.
                net_settled: None,
            });
        }
    }
    (part, asset)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUER: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";
    const G_FROM: &str = "GBLVLKGRDU66WLWY4XRORJXCC4LDZ347AQTUYBEPBABIZTVITW2OAGIP";
    const G_TO: &str = "GADKLS7RS3OC2MXGEZXQA46JNF3FBVSTHTWLDPRF7TWI6GXVP4OUE3ZR";

    fn ev(topics: String) -> EventRow {
        EventRow {
            transaction_id: 777,
            ledger_sequence: 62_952_436,
            topics_xdr: topics,
        }
    }

    #[test]
    fn sac_transfer_yields_two_participants_and_one_classic_asset() {
        let rows = vec![ev(format!(
            r#"[{{"type":"sym","value":"transfer"}},{{"type":"address","value":"{G_FROM}"}},{{"type":"address","value":"{G_TO}"}},{{"type":"string","value":"USDC:{ISSUER}"}}]"#
        ))];
        let (part, asset) = build_rows(&rows);
        assert_eq!(part.len(), 2);
        assert_eq!(part[0].transaction_id, 777);
        assert_eq!(part[0].ledger_sequence, 62_952_436);
        assert_eq!(part[0].account_id, ids::account_id(G_FROM));
        assert_eq!(part[1].account_id, ids::account_id(G_TO));
        assert_eq!(asset.len(), 1);
        assert_eq!(
            asset[0].asset_id,
            ids::asset_id(1, "USDC", ids::account_id(ISSUER), 0)
        );
    }

    #[test]
    fn native_mint_yields_one_participant_and_native_asset() {
        let rows = vec![ev(format!(
            r#"[{{"type":"sym","value":"mint"}},{{"type":"address","value":"{G_TO}"}},{{"type":"string","value":"native"}}]"#
        ))];
        let (part, asset) = build_rows(&rows);
        assert_eq!(part.len(), 1);
        assert_eq!(part[0].account_id, ids::account_id(G_TO));
        assert_eq!(asset.len(), 1);
        assert_eq!(asset[0].asset_id, ids::NATIVE_ASSET_ID);
    }

    #[test]
    fn bespoke_transfer_registers_participants_but_no_asset() {
        // No SEP-11 asset string → out of asset scope; participants still counted.
        let rows = vec![ev(format!(
            r#"[{{"type":"sym","value":"transfer"}},{{"type":"address","value":"{G_FROM}"}},{{"type":"address","value":"{G_TO}"}}]"#
        ))];
        let (part, asset) = build_rows(&rows);
        assert_eq!(part.len(), 2);
        assert!(asset.is_empty());
    }

    #[test]
    fn non_token_event_skipped() {
        let rows = vec![ev(format!(
            r#"[{{"type":"sym","value":"set_admin"}},{{"type":"address","value":"{G_TO}"}}]"#
        ))];
        let (part, asset) = build_rows(&rows);
        assert!(part.is_empty());
        assert!(asset.is_empty());
    }

    #[test]
    fn malformed_topics_json_skipped_not_fatal() {
        let rows = vec![ev("{not valid json".to_string())];
        let (part, asset) = build_rows(&rows);
        assert!(part.is_empty());
        assert!(asset.is_empty());
    }
}
