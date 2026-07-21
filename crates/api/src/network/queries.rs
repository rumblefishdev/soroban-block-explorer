//! ClickHouse implementation of `GET /v1/network/stats`.
//!
//! Reference SQL lives at
//! `docs/architecture/database-schema/endpoint-queries-clickhouse/01_get_network_stats.sql`,
//! which documents the semantics for TPS, accounts/contracts estimates, the
//! `generated_at` ↔ cache-staleness split, and empty-cluster handling.

use chrono::{DateTime, TimeZone, Utc};
use clickhouse::Row;
use serde::Deserialize;

use super::dto::NetworkStats;

/// Single-row projection of the canonical CH network-stats statement.
/// Field order matches the SELECT column order in `01_get_network_stats.sql`;
/// `clickhouse::Row` decodes by position, not by name.
#[derive(Debug, Row, Deserialize)]
struct StatsRow {
    latest_ledger_sequence: i64,
    /// `DateTime64(3, 'UTC')` decoded as ms since Unix epoch.
    latest_ledger_closed_at: i64,
    generated_at: i64,
    tps_60s: f64,
    total_accounts: u64,
    total_contracts: u64,
}

pub async fn fetch_stats(
    client: &clickhouse::Client,
    head: i64,
) -> Result<NetworkStats, clickhouse::error::Error> {
    // `ledgers` is ORDER BY `sequence`, NOT `closed_at`. Ordering or filtering
    // by `closed_at` forces a full 12M-row table scan (measured) — and this
    // endpoint is polled, so that scan recurs and eats the `read_rows` quota.
    // We drive both reads off `sequence`, pinned to the caller's `head` (the
    // chain head the cache version-keys on — `crate::common::head`):
    //   * the latest ledger via `WHERE sequence = head` (primary-key point
    //     read, one granule) — so `latest_ledger_sequence` always equals the
    //     cache key (no TOCTOU, no re-derivation of "latest" here);
    //   * the 60s TPS window via a `sequence > head - 200` prune that bounds
    //     the scan to the most recent ~200 ledgers (~20 min of headroom for a
    //     60s window) before the `closed_at` predicate refines it. That window
    //     is collapsed with `LIMIT 1 BY sequence` before `sum(transaction_count)`
    //     (lore-0420): `ledgers` is a ReplacingMergeTree, so a ledger that gets
    //     re-ingested before its parts merge appears twice and would be counted
    //     twice, inflating TPS. The tip is dup-free today, which is exactly why
    //     this must be structural rather than left to luck.
    // Both `ledgers` reads also carry an explicit row cap: the `latest` subquery
    // takes `LIMIT 1` because a duplicated head row would return two rows and
    // break `fetch_optional` with a 500 on the whole stats endpoint.
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
                (SELECT sum(transaction_count) \
                        / nullIf(dateDiff('second', min(closed_at), max(closed_at)), 0) \
                 FROM ( \
                     SELECT sequence, transaction_count, closed_at \
                     FROM ledgers \
                     WHERE sequence > {head} - 200 \
                       AND closed_at >= now64() - INTERVAL 60 SECOND \
                     LIMIT 1 BY sequence \
                 )), \
                0 \
            )) AS tps_60s, \
            ifNull((SELECT count() FROM accounts_recent), 0) \
                AS total_accounts, \
            ifNull((SELECT count() FROM soroban_contracts FINAL), 0) \
                AS total_contracts \
         FROM ( \
             SELECT sequence, closed_at \
             FROM ledgers \
             WHERE sequence = {head} \
             LIMIT 1 \
         ) AS latest"
    );
    let row_opt = client.query(&sql).fetch_optional::<StatsRow>().await?;

    let Some(row) = row_opt else {
        return Ok(NetworkStats {
            tps_60s: 0.0,
            total_accounts: 0,
            total_contracts: 0,
            latest_ledger_sequence: 0,
            latest_ledger_closed_at: None,
            generated_at: Utc::now(),
        });
    };

    Ok(NetworkStats {
        tps_60s: row.tps_60s,
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
