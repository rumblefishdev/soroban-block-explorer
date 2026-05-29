//! ClickHouse implementation of `GET /v1/network/stats`.
//!
//! Mirrors `crates/api/src/network/queries.rs` (PG) one-for-one — same
//! response shape, same empty-cluster handling. Reference SQL lives at
//! `docs/architecture/database-schema/endpoint-queries-clickhouse/01_get_network_stats.sql`;
//! the PG semantics for TPS, accounts/contracts estimates, and the
//! `generated_at` ↔ cache-staleness split carry over verbatim.

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
) -> Result<NetworkStats, clickhouse::error::Error> {
    let row_opt = client
        .query(
            "SELECT \
                latest.sequence AS latest_ledger_sequence, \
                latest.closed_at AS latest_ledger_closed_at, \
                now64() AS generated_at, \
                toFloat64(ifNull( \
                    (SELECT sum(transaction_count) \
                            / nullIf(dateDiff('second', min(closed_at), max(closed_at)), 0) \
                     FROM ledgers \
                     WHERE closed_at >= now64() - INTERVAL 60 SECOND), \
                    0 \
                )) AS tps_60s, \
                (SELECT total_rows FROM system.tables \
                    WHERE database = currentDatabase() AND name = 'accounts') \
                    AS total_accounts, \
                (SELECT total_rows FROM system.tables \
                    WHERE database = currentDatabase() AND name = 'soroban_contracts') \
                    AS total_contracts \
             FROM ( \
                 SELECT sequence, closed_at \
                 FROM ledgers \
                 ORDER BY closed_at DESC \
                 LIMIT 1 \
             ) AS latest",
        )
        .fetch_optional::<StatsRow>()
        .await?;

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
        // CH `system.tables.total_rows` is UInt64 but real chain-scale
        // totals (~1e8) fit Int64 with eight orders of magnitude to spare,
        // and the wire shape is `i64` (matches the PG path).
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
