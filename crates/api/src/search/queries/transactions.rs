//! Search's transaction bucket: an exact hash, outer or fee-bump inner.

use clickhouse::Row;
use serde::Deserialize;

use super::super::classifier::Classified;
use super::super::dto::{EntityType, SearchHit};
use super::IncludeFlags;
use crate::common::ch::millis_to_utc;

/// Mainnet ledger-partition width (`PARTITION BY intDiv(ledger_sequence,
/// 500000)` on `transactions`). Used to prune the `transactions` seek to the
/// single partition the `transaction_hash_index` lookup resolved.
const LEDGER_PARTITION_SIZE: i64 = 500_000;

// ---------------------------------------------------------------------------
// Transactions — exact hash → transaction_hash_index PK seek
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct LedgerSeqRow {
    ledger_sequence: i64,
}

#[derive(Debug, Row, Deserialize)]
struct TxMetaRow {
    successful: bool,
    /// `ledgers.closed_at` (`DateTime64(3)`) decoded as raw i64 millis.
    created_at_ms: i64,
}

/// Fires only for a hash-shaped query. Step 1 resolves `hash → ledger_sequence`
/// off `transaction_hash_index` (ORDER BY `hash`; immutable mapping, no FINAL).
/// The index maps a fee-bump's inner hash too (task 0375), so step 2 matches
/// the hash as the transaction's own or as its inner hash, as the transaction
/// page does. Step 2 reads `successful` + the ledger `closed_at` via a
/// single-partition, single-row seek on `transactions` (`ledger_sequence` leading PK — one
/// ledger is one granule) joined to `ledgers` — the PG `tx_hits`
/// enrichment, at the cost of two point-seeks.
pub(super) async fn search_transactions(
    client: &clickhouse::Client,
    classified: &Classified,
    include: &IncludeFlags,
) -> Result<Vec<(String, SearchHit)>, clickhouse::error::Error> {
    if !include.transaction {
        return Ok(Vec::new());
    }
    let Some(bytes) = classified.hash_bytes.as_deref() else {
        return Ok(Vec::new());
    };
    let hash_hex = hex::encode(bytes);

    let ledger = client
        .query(
            "SELECT ledger_sequence FROM transaction_hash_index \
             WHERE hash = unhex(?) LIMIT 1",
        )
        .bind(&hash_hex)
        .fetch_optional::<LedgerSeqRow>()
        .await?
        .map(|r| r.ledger_sequence);
    let Some(ledger) = ledger else {
        return Ok(Vec::new());
    };

    // `ledger` is from our own index seek (i64), inlined for partition pruning —
    // no injection surface. `hash` is bound. `successful` is immutable across
    // re-ingest, so `LIMIT 1` (no FINAL) is correct. `closed_at` is resolved via
    // a BOUNDED `ledgers WHERE sequence = {ledger}` sub-select (PK point seek, ~1
    // granule) — NOT a plain `INNER JOIN ledgers`, which builds its hash side
    // from the whole ~3.6M-row `ledgers` table on prod (measured: a scalar
    // correlated form also de-optimised to a full scan). INNER (not LEFT): under
    // the `api_reader` `join_use_nulls = 0` RBAC a LEFT-JOIN miss yields the
    // column DEFAULT on a NON-nullable column (not NULL), so LEFT buys no real
    // "missing ledger ⇒ None" semantics — it would only mask a 0-epoch. A real
    // indexed tx always has its ledger row, so INNER is correct and simpler.
    let partition = ledger / LEDGER_PARTITION_SIZE;
    let sql = format!(
        "SELECT t.successful AS successful, l.closed_at AS created_at_ms \
         FROM transactions t \
         INNER JOIN (SELECT sequence, closed_at FROM ledgers WHERE sequence = {ledger}) l \
                 ON l.sequence = t.ledger_sequence \
         WHERE t.ledger_sequence = {ledger} \
           AND intDiv(t.ledger_sequence, {LEDGER_PARTITION_SIZE}) = {partition} \
           AND (t.hash = unhex(?) OR t.inner_tx_hash = unhex(?)) \
         ORDER BY t.application_order \
         LIMIT 1"
    );
    let Some(meta) = client
        .query(&sql)
        .bind(&hash_hex)
        .bind(&hash_hex)
        .fetch_optional::<TxMetaRow>()
        .await?
    else {
        // Index row present but the transaction row is missing — treat as no
        // hit rather than emit a half-populated redirect target.
        return Ok(Vec::new());
    };

    Ok(vec![(
        "transaction".to_string(),
        SearchHit {
            entity_type: EntityType::Transaction,
            identifier: hash_hex,
            label: String::new(),
            route_token: None,
            successful: Some(meta.successful),
            last_activity_at: Some(millis_to_utc(meta.created_at_ms)),
            contract_id: None,
            token_id: None,
        },
    )])
}
