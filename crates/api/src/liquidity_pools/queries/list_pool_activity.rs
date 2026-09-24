//! `GET /v1/liquidity-pools/:id/activity` — what moved through the pool.

use chrono::{DateTime, Utc};
use clickhouse::Row;
use serde::Deserialize;
use std::collections::HashMap;

use crate::common::ch::{millis_to_utc, resolve_accounts};
use crate::common::cursor::{Direction, keyset_sql_desc};

use crate::liquidity_pools::dto::{PoolActivityCursor, PoolEvent};

/// One activity row after enrichment — the handler maps this straight into
/// `PoolActivityItem` (task 0491).
#[derive(Debug, Clone)]
pub struct PoolActivityRow {
    pub transaction_hash: String,
    pub ledger_sequence: i64,
    /// Surrogate `transactions.id`. Not on the wire — it is the cursor's
    /// middle component, the same tie-break the sort key uses.
    pub transaction_id: i64,
    pub application_order: i16,
    pub event: Option<PoolEvent>,
    /// One per leg, in `legs` order — see `PoolActivityItem::amounts`.
    pub amounts: Vec<Option<String>>,
    pub source_account: String,
    /// How many pools the whole operation crossed (`length(pool_ids)` off the
    /// same appearance seek that resolves the op source). `None` = unknowable
    /// (no appearance row), never guessed to `1`.
    pub pools_crossed: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Row, Deserialize)]
struct PoolLegsChRow {
    legs: Vec<i64>,
}

/// The pool's leg surrogates — the key `lp_operation_amounts.asset_id` is
/// written with (task 0279), so an amount row maps onto the legs the page
/// renders. `None` when the pool does not exist, which is also this seek's
/// existence check (it replaces a separate `pool_exists` round-trip).
///
/// This used to RECOMPUTE the surrogates in Rust from the pair columns, with a
/// comment explaining that SQL could not: our surrogate is `cityhash_102_128`'s
/// lower half and ClickHouse's builtin `cityHash64` is a different algorithm.
/// All of that is still true and no longer relevant — the writer now stores the
/// surrogates it computed, so this reads the column instead of reproducing it.
/// Verified against production before the change: 171,268 of 171,268
/// `lp_operation_amounts.asset_id` values match a leg in `legs`, zero orphans.
pub async fn fetch_pool_asset_ids(
    client: &clickhouse::Client,
    pool_id_hex: &str,
) -> Result<Option<Vec<i64>>, clickhouse::error::Error> {
    let rows = client
        .query(
            "SELECT legs FROM liquidity_pools WHERE pool_id = unhex(?) \
             ORDER BY last_updated_ledger DESC LIMIT 1",
        )
        .bind(pool_id_hex)
        .fetch_all::<PoolLegsChRow>()
        .await?;
    Ok(rows.into_iter().next().map(|r| r.legs))
}

/// One raw leg from `lp_operation_amounts` — the table's own grain, read in
/// sort-key order and paired in Rust. No `GROUP BY`: see
/// [`fetch_pool_activity`] for the measurement that removed it.
#[derive(Debug, Row, Deserialize)]
struct PoolLegChRow {
    ls: i64,
    tid: i64,
    ao: i16,
    asset_id: i64,
    amount: i64,
}

/// Transaction-level enrichment for the activity page's DISTINCT tx keys.
#[derive(Debug, Row, Deserialize)]
struct ActivityTxRow {
    id: i64,
    hash: String,
    source_id: i64,
    created_at_ms: i64,
}

/// The operation's own source account, `None` when it declares none (the XDR
/// default: the transaction's source).
#[derive(Debug, Row, Deserialize)]
struct OpSourceChRow {
    ls: i64,
    tid: i64,
    ao: i16,
    source_id: Option<i64>,
    /// `length(pool_ids)` — how many pools the whole operation crossed.
    pools_crossed: u64,
}

/// One operation's leg amounts, grouped out of the key-ordered leg stream.
/// `amounts[i]` belongs to the pool's `legs[i]`.
struct PairedOp {
    ls: i64,
    tid: i64,
    ao: i16,
    amounts: Vec<Option<i64>>,
}

impl PairedOp {
    /// `None` unless EVERY leg landed — the read stays total rather than
    /// classifying a half-row. `anyIf`-style defaulting would have made a
    /// missing leg read as `0` and turn a half-row into a "trade".
    fn event(&self) -> Option<PoolEvent> {
        let amounts: Vec<i64> = self.amounts.iter().copied().collect::<Option<_>>()?;
        Some(PoolEvent::from_signs(&amounts))
    }
}

/// Fold the key-ordered leg stream into operations.
///
/// The legs of one operation are ADJACENT by construction: `asset_id` is
/// the last component of the sort key, so rows sharing
/// `(ledger_sequence, transaction_id, application_order)` are neighbours. That
/// is the whole reason this can be a fold instead of an aggregation.
///
/// `truncated` means the read hit its row cap, so the final group may be
/// missing a leg that simply did not fit — it is dropped and re-read from the
/// previous complete key on the next window.
fn pair_legs(rows: Vec<PoolLegChRow>, legs: &[i64], truncated: bool) -> Vec<PairedOp> {
    let mut out: Vec<PairedOp> = Vec::new();
    for r in rows {
        let same_op = out
            .last()
            .is_some_and(|last| (last.ls, last.tid, last.ao) == (r.ls, r.tid, r.ao));
        if !same_op {
            out.push(PairedOp {
                ls: r.ls,
                tid: r.tid,
                ao: r.ao,
                amounts: vec![None; legs.len()],
            });
        }
        // An asset that is not one of the pool's legs has no slot to land in.
        if let (Some(op), Some(i)) = (out.last_mut(), legs.iter().position(|&l| l == r.asset_id)) {
            op.amounts[i] = Some(r.amount);
        }
    }
    if truncated {
        out.pop();
    }
    out
}

/// `GET /v1/liquidity-pools/:id/activity` — one row per (operation, pool),
/// task 0491.
///
/// **The driver table is the design.** `operation_pools` is keyed
/// `(pool_id, ledger_sequence, transaction_id)` with no `application_order`,
/// so it cannot page per operation. `lp_operation_amounts` is keyed
/// `(pool_id, ledger_sequence, transaction_id, application_order, asset_id)`
/// — the page's exact grain, reached by one PK-prefix seek.
///
/// **No `GROUP BY`, and that is measured, not stylistic.** The first cut of
/// this function pivoted the legs with `countIf`/`anyIf` and grouped by the
/// key triple. On prod's busiest pool (1.68M leg rows) that read **2.60M rows
/// / 109 ms / 182 MiB** to return 21 operations — a `GROUP BY` has to consume
/// the pool's whole slice before `ORDER BY … LIMIT` can pick the newest 21.
/// `optimize_aggregation_in_order` did not help (same rows, 253 ms). Reading
/// the same rows in sort-key order and pairing them here is **115k rows /
/// 9 ms / ~11 MiB** (median of 3), against **159k / 11 ms** for the
/// per-transaction endpoint this replaces: 22× off the first cut, and
/// slightly under the shape it supersedes.
///
/// Take the medians, not single runs — a cold run of EITHER shape reads
/// 0.7–1.0M rows, so one measurement each can invert the comparison. Measured
/// 2026-08-18 on prod; `log_comment` `lore0491-*` / `rep-*` in
/// `system.query_log`.
///
/// For the record: `FINAL` was never the cost — it added 22% (2.60M → 3.17M),
/// not an order of magnitude. It stays off because the producer is
/// deterministic (schema header's single-writer argument), so an unmerged
/// duplicate is byte-identical to its twin.
///
/// **Known consequence: an operation with no amount rows is not listed.** The
/// indexer writes `operation_pools` for an op that *declares* a pool whether
/// or not the transaction succeeded; amounts are written only for value that
/// actually moved. A failed explicit LP op therefore had a row under
/// `/transactions` and has none here — the page answers "what moved through
/// this pool", and a failed op moved nothing.
pub async fn fetch_pool_activity(
    client: &clickhouse::Client,
    pool_id_hex: &str,
    legs: &[i64],
    limit: i64,
    cursor: Option<&PoolActivityCursor>,
    direction: Direction,
    event: Option<PoolEvent>,
) -> Result<Vec<PoolActivityRow>, clickhouse::error::Error> {
    let (op, order) = keyset_sql_desc(direction);

    // Where the next window resumes. `<` on the whole triple skips the last
    // kept operation outright — both its legs share that triple, so there is
    // no half-operation to step over.
    let mut after: Option<(i64, i64, i16)> =
        cursor.map(|c| (c.ledger_sequence, c.transaction_id, c.application_order));

    // One row per leg per operation, plus slack so the cap rarely lands mid-op.
    let legs_per_op = legs.len() as i64;
    let mut window = (limit * legs_per_op + legs_per_op).max(64);
    let mut ops: Vec<PairedOp> = Vec::new();

    // One pass when unfiltered (the common case). With `filter[event]` the
    // matching rate is unknown up front — the event is only knowable once both
    // legs are in hand — so the window doubles until the page fills or the
    // pool runs out. Geometric growth keeps this O(log) round trips and never
    // reads more than ~2× the span it had to cover; a linear re-poll would be
    // the slow version of the same idea.
    loop {
        let keyset = match after {
            Some((ls, tid, ao)) => format!(
                " AND (ledger_sequence, transaction_id, application_order) {op} ({ls}, {tid}, {ao})"
            ),
            None => String::new(),
        };
        let sql = format!(
            "SELECT \
                ledger_sequence   AS ls, \
                transaction_id    AS tid, \
                application_order AS ao, \
                asset_id          AS asset_id, \
                amount            AS amount \
             FROM lp_operation_amounts \
             WHERE pool_id = toFixedString(unhex(?), 32) \
               AND ledger_sequence <= (SELECT max(sequence) FROM ledgers) {keyset} \
             ORDER BY ls {order}, tid {order}, ao {order} \
             LIMIT {window}"
        );
        let rows = client
            .query(&sql)
            .bind(pool_id_hex)
            .fetch_all::<PoolLegChRow>()
            .await?;

        let exhausted = (rows.len() as i64) < window;
        let batch = pair_legs(rows, legs, !exhausted);
        if let Some(last) = batch.last() {
            after = Some((last.ls, last.tid, last.ao));
        }

        match event {
            Some(want) => ops.extend(batch.into_iter().filter(|o| o.event() == Some(want))),
            None => ops.extend(batch),
        }

        if exhausted || (ops.len() as i64) >= limit {
            break;
        }
        window *= 2;
    }
    ops.truncate(limit as usize);
    if ops.is_empty() {
        return Ok(Vec::new());
    }

    // Enrich the page's DISTINCT transactions — several operations of one
    // transaction share a row here, so this set is smaller than the page.
    // Keys inlined (i64) with the partition prune that turns the
    // `(ledger_sequence, id) IN (…)` filter into a tight PK seek, same shape
    // as `common::ch::fetch_tx_list_aggregates`.
    let tx_keys: std::collections::BTreeSet<(i64, i64)> =
        ops.iter().map(|o| (o.ls, o.tid)).collect();
    let in_tuples = tx_keys
        .iter()
        .map(|(ls, tid)| format!("({ls},{tid})"))
        .collect::<Vec<_>>()
        .join(",");
    let partitions = tx_keys
        .iter()
        .map(|(ls, _)| ls / 500_000)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let detail_sql = format!(
        "SELECT \
            t.id                                 AS id, \
            lower(hex(t.hash))                   AS hash, \
            t.source_id                          AS source_id, \
            toUnixTimestamp64Milli(l.closed_at)  AS created_at_ms \
         FROM transactions t \
         INNER JOIN ledgers l ON l.sequence = t.ledger_sequence \
         WHERE (t.ledger_sequence, t.id) IN ({in_tuples}) \
           AND intDiv(t.ledger_sequence, 500000) IN ({partitions}) \
         LIMIT 1 BY t.id"
    );
    let txs = client
        .query(&detail_sql)
        .fetch_all::<ActivityTxRow>()
        .await?;
    let by_tx: HashMap<i64, &ActivityTxRow> = txs.iter().map(|t| (t.id, t)).collect();

    // The OPERATION's own source account. A Stellar operation may declare one,
    // and then it — not the transaction's source — is who performed this
    // operation; `operations_appearances.source_id` is NULL when it does not,
    // which per the XDR means "same as the transaction's". Showing the
    // transaction's source on a per-operation row names the wrong account
    // whenever they differ (measured on prod: 41% of ops in a recent ledger
    // window declare their own, and stellar.expert shows that one).
    //
    // `(ledger_sequence, transaction_id, application_order)` IS this table's
    // sort key, so the page's bounded IN-list is a PK seek with the same
    // partition prune. `max()` rather than `LIMIT 1 BY`: the table holds one
    // row per APPEARANCE, so an operation has several, and aggregation skips
    // the NULLs instead of picking one arbitrarily.
    //
    // `pools_crossed` rides the same seek for free: `pool_ids` is the op's
    // sorted+deduped crossing list, written identically on every appearance
    // row (stage.rs fans the one list out), so `max(length(...))` is just
    // "the length". It is what lets a row say "this trade was one hop of an
    // N-pool route" without carrying the route itself — the route lives on
    // the op's detail page, which the row already links to.
    let op_sources_sql = format!(
        "SELECT \
            ledger_sequence   AS ls, \
            transaction_id    AS tid, \
            application_order AS ao, \
            max(source_id)    AS source_id, \
            max(length(pool_ids)) AS pools_crossed \
         FROM operations_appearances \
         WHERE (ledger_sequence, transaction_id) IN ({in_tuples}) \
           AND intDiv(ledger_sequence, 500000) IN ({partitions}) \
         GROUP BY ls, tid, ao"
    );
    let op_sources = client
        .query(&op_sources_sql)
        .fetch_all::<OpSourceChRow>()
        .await?;
    let by_op: HashMap<(i64, i64, i16), (Option<i64>, u64)> = op_sources
        .iter()
        .map(|r| ((r.ls, r.tid, r.ao), (r.source_id, r.pools_crossed)))
        .collect();

    // Source StrKeys by surrogate id (bloom seek) rather than a whole-
    // `accounts` INNER JOIN — task 0354. One resolve for both kinds of source.
    let account_ids = txs
        .iter()
        .map(|t| t.source_id)
        .chain(by_op.values().filter_map(|(src, _)| *src))
        .collect();
    let accounts = resolve_accounts(client, account_ids).await?;

    // A page row whose transaction did not resolve is DROPPED, not rendered
    // half-blank: it would have no hash to link and no timestamp to sort by.
    // Unreachable unless the tx tables lag the amounts table, and the
    // `max(sequence)` fence above already keeps the seek behind the commit
    // marker.
    Ok(ops
        .into_iter()
        .filter_map(|o| {
            let tx = by_tx.get(&o.tid)?;
            // The operation's own source, falling back to the transaction's —
            // which is what the XDR's absent `sourceAccount` means.
            let (op_source, pools_crossed) = by_op
                .get(&(o.ls, o.tid, o.ao))
                .copied()
                .map_or((None, None), |(src, n)| (src, Some(n as i64)));
            let source_id = op_source.unwrap_or(tx.source_id);
            let source_account = accounts.get(&source_id)?.clone();
            let event = o.event();
            Some(PoolActivityRow {
                transaction_hash: tx.hash.clone(),
                ledger_sequence: o.ls,
                transaction_id: o.tid,
                application_order: o.ao,
                event,
                amounts: o
                    .amounts
                    .iter()
                    .map(|a| event.and(*a).map(|v| v.to_string()))
                    .collect(),
                source_account,
                pools_crossed,
                created_at: millis_to_utc(tx.created_at_ms),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests;
