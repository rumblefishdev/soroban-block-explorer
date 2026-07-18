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
//! ## Idempotency — and why it is now conditional
//!
//! Both targets are `ReplacingMergeTree`; re-inserting identically-keyed rows
//! collapses on merge. Runs the whole ingested history (range auto-detected, no
//! args), so it DOES overlap live ingest. `--dry-run` counts the would-be rows
//! without writing.
//!
//! That overlap used to be free for presence-only rows: both writers emitted the
//! same row and the version-less RMT collapsed the duplicate. Task 0393 added
//! `net_settled`. If this backfill wrote a placeholder (e.g. `net_settled: None`
//! "to fill in later") while live ingest had computed a real value for the same
//! key, the merge would keep whichever it happened to, and the read's
//! `HAVING net_settled IS NOT NULL` could blank the column across the overlap.
//!
//! So this backfill reduces the value with [`token_events_net_settled`] — the
//! SAME function the live indexer runs, over the same inputs — and therefore
//! emits rows identical to live's. The duplicate collapses cleanly *because* the
//! two agree, not by luck. **Do not add a column to this row struct without
//! computing it as the live path would.**

use std::collections::BTreeMap;

use clickhouse::Client as ClickhouseClient;
use clickhouse::Row;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, info};

use db_clickhouse::persist::ids;
use db_clickhouse::persist::rows::{OperationAssetAppearanceRow, TransactionParticipantRow};
use db_clickhouse::persist::stage::{derive_token_event, token_events_net_settled};

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
    /// `operation_asset_appearances` rows derived (native, SAC-classic, and
    /// bespoke Soroban tokens since 0393).
    pub asset_rows: u64,
    pub dry_run: bool,
}

/// One `soroban_events` row projected to what the derivation needs.
///
/// `data_xdr` WAS deliberately skipped under the presence-only model (0383) to
/// cut read I/O on the large ZSTD payload columns. Task 0393 stores the moved
/// amount, which only `data_xdr` carries, so it is read now — the saving is no
/// longer available, and omitting it would mean writing a value this backfill
/// cannot compute (see the module header on why that is destructive).
#[derive(Debug, Row, Deserialize)]
struct EventRow {
    transaction_id: i64,
    ledger_sequence: i64,
    topics_xdr: String,
    /// ScVal-decoded JSON payload (the column name is historical). Carries the
    /// moved amount; needed to reduce the net-settled value (task 0393 G1).
    data_xdr: String,
    /// Emitting contract surrogate — the asset identity for a bespoke token
    /// (task 0393); passed to `derive_token_event` so backfill + live match.
    contract_id: i64,
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
/// materializes the heavy ZSTD payload columns (`topics_xdr` + `data_xdr`)
/// **only for survivors** — the
/// quota-dominant cost. Measured on a 500-ledger head window: 236,257 sig-matched
/// rows but only 43,504 (18%) are `has_soroban`, so PREWHERE cuts the payload
/// read ~5×. (0393 added `data_xdr` to the projection, so the per-survivor cost
/// roughly doubles; the ~5x PREWHERE saving is unchanged.) `transaction_id IN (subquery)` is set-membership (not a JOIN), so an
/// unmerged-RMT duplicate `id` in `transactions` cannot fan out a row.
/// `lowerUTF8(signature)` mirrors the parser's case-insensitive verb match.
async fn read_events(
    client: &ClickhouseClient,
    lo: i64,
    hi: i64,
) -> Result<Vec<EventRow>, BackfillError> {
    client
        .query(
            "SELECT transaction_id, ledger_sequence, topics_xdr, data_xdr, contract_id \
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

/// One transaction's decoded token events: its ledger, and each event's
/// `(topics, data, emitting contract surrogate)` — the triple
/// [`token_events_net_settled`] reduces.
type TxEvents = (i64, Vec<(Value, Value, Option<i64>)>);

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

    // Presence is per-event, but the net-settled value is per (tx, asset) and
    // needs every event of the transaction at once — so decode first, group by
    // transaction, then reduce. A malformed payload is skipped for presence and
    // is simply absent from the reduction's input.
    let mut by_tx: BTreeMap<i64, TxEvents> = BTreeMap::new();
    for r in rows {
        let Ok(topics) = serde_json::from_str::<Value>(&r.topics_xdr) else {
            continue;
        };
        let data = serde_json::from_str::<Value>(&r.data_xdr).unwrap_or(Value::Null);
        let emitting = (r.contract_id != 0).then_some(r.contract_id);
        by_tx
            .entry(r.transaction_id)
            .or_insert_with(|| (r.ledger_sequence, Vec::new()))
            .1
            .push((topics, data, emitting));
    }

    for (transaction_id, (ledger_sequence, events)) in by_tx {
        // Same reduction the live indexer runs, over the same inputs, so the two
        // writers emit identical rows. Load-bearing: the version-less RMT keeps an
        // arbitrary row among duplicates on merge, so a disagreeing backfill row
        // could silently win over the live one. `soroban_events` holds no
        // diagnostic events (the writer filters them), which is exactly what the
        // live path drops before reducing.
        let value_by_asset: BTreeMap<i64, Option<i128>> =
            token_events_net_settled(events.iter().map(|(t, d, c)| (t, d, *c)))
                .into_iter()
                .map(|ns| (ns.asset_id, ns.amount))
                .collect();

        for (topics, _data, emitting) in &events {
            let Some(derived) = derive_token_event(topics, *emitting) else {
                continue;
            };
            for strkey in derived.participant_strkeys {
                part.push(TransactionParticipantRow {
                    account_id: ids::account_id(&strkey),
                    ledger_sequence,
                    transaction_id,
                });
            }
            if let Some(asset_id) = derived.asset_id {
                asset.push(OperationAssetAppearanceRow {
                    asset_id,
                    ledger_sequence,
                    transaction_id,
                    // `None` only when the reduction genuinely could not be made
                    // (unreadable amount / not representable) — never as a
                    // placeholder. The RMT collapses the duplicate (asset, tx)
                    // rows this per-event loop emits.
                    net_settled: value_by_asset.get(&asset_id).copied().flatten(),
                });
            }
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
    const G_THIRD: &str = "GB5WIXCUO5DWAJSVLVIJH5SBWGIRKGD27YYHLPOISGBO7MW2UH3EJXLM";

    fn ev(topics: String) -> EventRow {
        ev_with_amount(topics, "150")
    }

    fn ev_with_amount(topics: String, amount: &str) -> EventRow {
        EventRow {
            transaction_id: 777,
            ledger_sequence: 62_952_436,
            topics_xdr: topics,
            data_xdr: format!(r#"{{"type":"i128","value":"{amount}"}}"#),
            contract_id: 0,
        }
    }

    // A bespoke-token transfer: NO SEP-11 asset string, emitted by a real
    // contract. This is the shape that carries a VALUE through the H2 trust gate
    // (task 0393) — a classic-asset string claim is untrusted from events until
    // the SAC crypto-guard (task 0410), so the value tests use bespoke tokens.
    const CTOKEN: &str = "CBQFOPGGSP4VCDFXJ4YEPCQNLN6EFRC4M7OOLQOEEY7H4VPF6N4WEE2N";
    fn bespoke_topics_between(from: &str, to: &str) -> String {
        format!(
            r#"[{{"type":"sym","value":"transfer"}},{{"type":"address","value":"{from}"}},{{"type":"address","value":"{to}"}}]"#
        )
    }
    fn bespoke_ev(topics: String, amount: &str) -> EventRow {
        EventRow {
            transaction_id: 777,
            ledger_sequence: 62_952_436,
            topics_xdr: topics,
            data_xdr: format!(r#"{{"type":"i128","value":"{amount}"}}"#),
            contract_id: ids::contract_id(CTOKEN),
        }
    }

    /// The landmine this backfill must never re-arm (task 0393).
    ///
    /// `operation_asset_appearances` is a version-less `ReplacingMergeTree`, so a
    /// backfill row that disagreed with the live row for the same key could win
    /// the merge. If it emitted a placeholder instead of the real value, running
    /// it could silently blank a value live had computed — the read drops NULL
    /// rows. So the value must be present and must equal live's.
    #[test]
    fn backfill_writes_the_real_value_not_a_placeholder() {
        let (_part, asset) = build_rows(&[bespoke_ev(bespoke_topics_between(G_FROM, G_TO), "150")]);
        assert_eq!(asset.len(), 1);
        assert_eq!(
            asset[0].net_settled,
            Some(150),
            "a placeholder here erases live-computed values on the next run"
        );
    }

    /// The two writers must agree row-for-row, not merely both be non-null.
    #[test]
    fn backfill_value_equals_what_the_live_indexer_would_compute() {
        let topics: Value = serde_json::from_str(&bespoke_topics_between(G_FROM, G_TO)).unwrap();
        let data: Value = serde_json::from_str(r#"{"type":"i128","value":"150"}"#).unwrap();
        let emitting = Some(ids::contract_id(CTOKEN));

        // What live ingest reduces for the same event (same emitting contract).
        let live = token_events_net_settled(std::iter::once((&topics, &data, emitting)));

        // What the backfill writes.
        let (_part, asset) = build_rows(&[bespoke_ev(bespoke_topics_between(G_FROM, G_TO), "150")]);

        assert_eq!(live.len(), 1);
        assert_eq!(asset.len(), 1);
        assert_eq!(asset[0].asset_id, live[0].asset_id, "surrogate must match");
        assert_eq!(asset[0].net_settled, live[0].amount, "value must match");
    }

    /// A routing chain inside one transaction: the value is per (tx, asset), so
    /// the backfill must reduce across the transaction's events rather than
    /// per-event. Two 150 legs through one hub net to 150, not 300.
    #[test]
    fn backfill_reduces_across_the_transaction_not_per_event() {
        let (_part, asset) = build_rows(&[
            bespoke_ev(bespoke_topics_between(G_FROM, G_TO), "150"),
            bespoke_ev(bespoke_topics_between(G_TO, G_THIRD), "150"),
        ]);
        // Per-event rows for the same (asset, tx) — the RMT collapses them; every
        // one must carry the transaction-level figure.
        assert!(!asset.is_empty());
        for row in &asset {
            assert_eq!(
                row.net_settled,
                Some(150),
                "netted across the tx, not summed"
            );
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
