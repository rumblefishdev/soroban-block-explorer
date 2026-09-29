//! Task 0374 (W1) — fill `soroban_pool_event_amounts` from the events already
//! in `soroban_events`, for every pool in the registry.
//!
//! No archive re-parse: the events are stored decoded, and the live writer
//! derives its rows from exactly those staged rows through the same decoder
//! ([`stage::soroban_pool_amounts::soroban_pool_amount_rows`]), so a backfilled row is byte-identical
//! to the one the live writer would have produced. Re-running is harmless
//! (ReplacingMergeTree keyed by the event).
//!
//! Run AFTER the writer is deployed: events arriving meanwhile are covered by
//! the writer, events before it by this pass, and the overlap collapses.
//! A one-shot catch-up: delete it once it has run on production (README
//! clause 4) — the live writer owns the table from then on.

use std::collections::HashMap;

use db_clickhouse::persist::rows::SorobanEventRow;
use db_clickhouse::persist::stage;

use crate::error::BackfillError;
use crate::sink::Sink;
use crate::util::insert_rows;

/// Events buffered before decoding. An operation never spans ledgers, so the
/// buffer is cut only at a ledger boundary; this bounds memory on the busiest
/// pool (~2.4M events), not correctness.
const CHUNK_EVENTS: usize = 100_000;

#[derive(Debug, Default)]
pub struct Stats {
    pub dry_run: bool,
    pub pools: usize,
    pub events_read: u64,
    pub rows: u64,
}

pub async fn execute(sink: &Sink, dry_run: bool) -> Result<Stats, BackfillError> {
    let client = sink.client();
    let pools = db_clickhouse::persist::fetch_soroban_pools(client, true).await?;
    let sac_classic = db_clickhouse::persist::fetch_sac_classic_map(client, true).await?;
    let mut stats = Stats {
        dry_run,
        pools: pools.len(),
        ..Default::default()
    };

    for (contract_id, pool) in &pools {
        let rows_before = stats.rows;
        let one = HashMap::from([(*contract_id, pool.clone())]);
        // Key order of the table itself, so the read streams in order; `LIMIT 1
        // BY` drops unmerged ReplacingMergeTree duplicates of an event.
        let mut cursor = client
            .query(
                "SELECT contract_id, ledger_sequence, transaction_index, operation_index, \
                        event_index, application_order, event_type, signature, \
                        topics_xdr, data_xdr \
                 FROM soroban_events WHERE contract_id = ? \
                 ORDER BY ledger_sequence, transaction_index, operation_index, event_index \
                 LIMIT 1 BY ledger_sequence, transaction_index, operation_index, event_index",
            )
            .bind(*contract_id)
            .fetch::<SorobanEventRow>()?;
        let mut buf: Vec<SorobanEventRow> = Vec::new();
        loop {
            let next = cursor.next().await?;
            let ledger_ends = match (&next, buf.last()) {
                (Some(n), Some(last)) => n.ledger_sequence != last.ledger_sequence,
                (None, _) => true,
                (Some(_), None) => false,
            };
            if ledger_ends && (next.is_none() || buf.len() >= CHUNK_EVENTS) {
                let rows =
                    stage::soroban_pool_amounts::soroban_pool_amount_rows(&buf, &one, &sac_classic);
                stats.events_read += buf.len() as u64;
                stats.rows += rows.len() as u64;
                if !dry_run {
                    insert_rows(client, "soroban_pool_event_amounts", &rows).await?;
                }
                buf.clear();
            }
            match next {
                Some(ev) => buf.push(ev),
                None => break,
            }
        }
        tracing::info!(
            contract_id,
            rows = stats.rows - rows_before,
            "soroban pool amounts: pool done"
        );
    }
    Ok(stats)
}
