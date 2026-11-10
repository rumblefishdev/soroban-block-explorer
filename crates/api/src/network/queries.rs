//! ClickHouse implementation of `GET /v1/network/stats`.
//!
//! The comment in [`fetch_stats`] documents the TPS window, the deduped
//! accounts/contracts counts and empty-cluster handling; the `generated_at` ↔
//! cache-staleness split is on [`NetworkStats`].

use chrono::{DateTime, TimeZone, Utc};
use clickhouse::Row;
use serde::Deserialize;

use super::dto::NetworkStats;

/// Single-row projection of the network-stats statement in [`fetch_stats`].
/// Field order matches its SELECT column order;
/// `clickhouse::Row` decodes by position, not by name.
#[derive(Debug, Row, Deserialize)]
struct StatsRow {
    latest_ledger_sequence: i64,
    /// `DateTime64(3, 'UTC')` decoded as ms since Unix epoch.
    latest_ledger_closed_at: i64,
    generated_at: i64,
    tps: f64,
    total_accounts: u64,
    total_contracts: u64,
}

/// Ledgers in the TPS window, ending at head (~1 h at ~5 s per ledger).
/// The same window Chainspect publishes for Stellar (task 0669).
const TPS_WINDOW_LEDGERS: i64 = 720;

pub async fn fetch_stats(
    client: &clickhouse::Client,
    head: i64,
) -> Result<NetworkStats, clickhouse::error::Error> {
    // `ledgers` is ORDER BY `sequence`, NOT `closed_at`. Ordering or filtering
    // by `closed_at` forces a full 12M-row table scan (measured) — and this
    // endpoint is polled, so that scan recurs and eats the `read_rows` quota.
    // Every read is a primary-key range pinned to the caller's `head` (the
    // chain head the cache version-keys on — `crate::common::head`), with no
    // `closed_at` filter and no `now64()`:
    //   * the latest ledger via `WHERE sequence = head` (primary-key point
    //     read, one granule) — so `latest_ledger_sequence` always equals the
    //     cache key (no TOCTOU, no re-derivation of "latest" here). It takes
    //     `LIMIT 1` because a duplicated head row would return two rows and
    //     break `fetch_optional` with a 500 on the whole stats endpoint;
    //   * TPS (task 0669) = operations per second over the
    //     `TPS_WINDOW_LEDGERS` ledgers ending at head: `sum(operation_count)`
    //     over `transactions` divided by the `closed_at` span of the same
    //     ledgers. `operation_count` is the envelope's count (the inner tx's
    //     for a fee bump) and is set for failed transactions too, so the sum
    //     is Horizon's `tx_set_operation_count` and Chainspect's `txns`
    //     (checked equal for two of its published ranges). Dividing by the
    //     span counts N ledgers over N-1 intervals, as Chainspect does; over
    //     720 ledgers that is +0.14%. `ledgers` is the indexer's commit
    //     marker, written after `transactions`, so the head ledger's
    //     operations are already in the sum. `min` / `max` of `closed_at` are
    //     not affected by duplicate ledger rows.
    //
    // Duplicates (lore-0420): both tables are `ReplacingMergeTree` with **no
    // version column**, so a row re-ingested before its parts merge is there
    // twice, and the copies are identical — measured over 308,930 duplicated
    // ledger sequences (ranges 60.0-60.2M and 63.3-63.5M), `closed_at`,
    // `transaction_count` and `hash` match across every pair, consistent with
    // a backfill re-walking history (0422). Picking one row per key is the
    // whole requirement:
    //   * `ledgers` reads take a row cap, not `FINAL`: `FINAL` would cost the
    //     19x read amplification measured on the ledgers list;
    //   * the `transactions` sum reads `FINAL`, which collapses rows by the
    //     sort key `(ledger_sequence, application_order)`. On this primary-key
    //     range it merges only parts whose ranges overlap, and at the head
    //     they do not (each indexer insert covers new ledgers). Measured on
    //     prod over 10 heads (2026-11-10): the whole statement took p50 25 ms
    //     with `FINAL` and 95 ms with `LIMIT 1 BY ledger_sequence,
    //     application_order`, with the same TPS every time. Overlapping parts
    //     (a backfill re-writing recent ledgers) raise the `FINAL` cost
    //     towards that of `LIMIT 1 BY`; it stays bounded by the 720 ledgers.
    // `head` is a trusted i64 from our own head probe, so it is inlined (no
    // injection surface) — matching the i64-inlining convention in
    // `crates/api/src/common/ch.rs`. This also drops the previous inner
    // `(SELECT max(sequence) FROM ledgers)` subquery — one fewer read.
    //
    // `total_accounts` / `total_contracts` do NOT read `system.tables.total_rows`
    // (lore-0420). That column is the PHYSICAL part-row count, so on a
    // ReplacingMergeTree it counts unmerged duplicates and reports too high —
    // measured +4.3% (accounts) / +11.6% (contracts), and drifting upward. This
    // is structural, not a one-off: RMT dedup is a best-effort background merge
    // and the indexer writes continuously (~8.46M account rows/day for ~136k
    // genuinely-active accounts), so fresh duplicates always exist there. A
    // one-time OPTIMIZE cannot fix it — the counts must dedup at READ time.
    // Both do so without an expensive scan:
    //   * accounts → `accounts_recent`, the refreshable-MV copy already deduped
    //     to one row per account (same source the /accounts list pages, so the
    //     KPI and the list agree). Plain MergeTree ⇒ `count()` is a metadata
    //     read, not a scan. Exact to ±1 vs `accounts FINAL`, modulo the 2-minute
    //     MV refresh — acceptable staleness for a headline total.
    //   * soroban_contracts → `FINAL`, affordable here because the table is
    //     ~146k rows (not 14M like accounts).
    let sql = format!(
        "SELECT \
            latest.sequence AS latest_ledger_sequence, \
            latest.closed_at AS latest_ledger_closed_at, \
            now64() AS generated_at, \
            toFloat64(ifNull( \
                (SELECT sum(operation_count) \
                 FROM transactions FINAL \
                 WHERE ledger_sequence > {head} - {window} \
                   AND ledger_sequence <= {head}) \
                / nullIf((SELECT dateDiff('second', min(closed_at), max(closed_at)) \
                          FROM ledgers \
                          WHERE sequence > {head} - {window} \
                            AND sequence <= {head}), 0), \
                0 \
            )) AS tps, \
            ifNull((SELECT count() FROM accounts_recent), 0) \
                AS total_accounts, \
            ifNull((SELECT count() FROM soroban_contracts FINAL), 0) \
                AS total_contracts \
         FROM ( \
             SELECT sequence, closed_at \
             FROM ledgers \
             WHERE sequence = {head} \
             LIMIT 1 \
         ) AS latest",
        window = TPS_WINDOW_LEDGERS,
    );
    let row_opt = client.query(&sql).fetch_optional::<StatsRow>().await?;

    let Some(row) = row_opt else {
        return Ok(NetworkStats {
            tps: 0.0,
            total_accounts: 0,
            total_contracts: 0,
            latest_ledger_sequence: 0,
            latest_ledger_closed_at: None,
            generated_at: Utc::now(),
        });
    };

    Ok(NetworkStats {
        tps: row.tps,
        // CH `count()` is UInt64 but real chain-scale totals (~1e8) fit Int64
        // with eight orders of magnitude to spare, and the wire shape is `i64`
        // (matches the PG path).
        total_accounts: row.total_accounts as i64,
        total_contracts: row.total_contracts as i64,
        latest_ledger_sequence: row.latest_ledger_sequence,
        latest_ledger_closed_at: Some(millis_to_utc(row.latest_ledger_closed_at)),
        generated_at: millis_to_utc(row.generated_at),
    })
}

fn millis_to_utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
}
