//! Search's transaction bucket: an exact hash, outer or fee-bump inner.

use clickhouse::Row;
use serde::Deserialize;

use super::IncludeFlags;
use crate::common::ch::millis_to_utc;
use crate::search::classifier::Classified;
use crate::search::dto::{EntityType, SearchHit};
use crate::transactions::lookup_hash_ledgers;

/// Mainnet ledger-partition width (`PARTITION BY intDiv(ledger_sequence,
/// 500000)` on `transactions`). Used to prune the `transactions` seek to the
/// single partition each hash-index candidate names.
const LEDGER_PARTITION_SIZE: i64 = 500_000;

// ---------------------------------------------------------------------------
// Transactions — exact hash → transaction_hash_prefix_index seek
// ---------------------------------------------------------------------------

#[derive(Debug, Row, Deserialize)]
struct TxMetaRow {
    successful: bool,
    /// `ledgers.closed_at` (`DateTime64(3)`) decoded as raw i64 millis.
    created_at_ms: i64,
}

/// Fires only for a hash-shaped query. Step 1 takes the candidate ledgers off
/// `transaction_hash_prefix_index` through the transaction page's own
/// [`lookup_hash_ledgers`] (the hash's first 8 bytes — more than one only when
/// two hashes share the prefix, task 0580). The index maps a fee-bump's inner hash too (task 0375), so
/// step 2, per candidate until one matches, checks the hash as the
/// transaction's own or as its inner hash, as the transaction page does: it
/// reads `successful` + the ledger `closed_at` via a single-partition,
/// single-row seek on `transactions` (`ledger_sequence` leading PK — one
/// ledger is one granule) joined to `ledgers` — the PG `tx_hits` enrichment,
/// at the cost of two point-seeks.
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

    for ledger in lookup_hash_ledgers(client, &hash_hex).await? {
        if let Some(meta) = fetch_tx_meta(client, ledger, &hash_hex).await? {
            return Ok(vec![(
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
            )]);
        }
    }
    // No candidate, or an index row whose transaction row is missing — no hit
    // rather than a half-populated redirect target.
    Ok(Vec::new())
}

/// `successful` + the ledger's `closed_at` of the transaction in `ledger` whose
/// own or fee-bump inner hash is `hash_hex`, if there is one.
async fn fetch_tx_meta(
    client: &clickhouse::Client,
    ledger: i64,
    hash_hex: &str,
) -> Result<Option<TxMetaRow>, clickhouse::error::Error> {
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
    client
        .query(&sql)
        .bind(hash_hex)
        .bind(hash_hex)
        .fetch_optional::<TxMetaRow>()
        .await
}
