//! Database queries backing `GET /v1/network/stats`.
//!
//! Implementation of the canonical SQL in
//! `docs/architecture/database-schema/endpoint-queries/01_get_network_stats.sql`
//! (deliverable of task 0167) — one statement, one round-trip.
//!
//! Cost profile per cache miss:
//!   * latest ledger:    index lookup on `idx_ledgers_closed_at DESC`, 1 row.
//!   * tps_60s:          index range on the same index, ~12 rows (5s × 60s).
//!   * total_accounts:   `pg_class.reltuples` catalog read, microseconds.
//!   * total_contracts:  `pg_class.reltuples` catalog read, microseconds.
//!
//! No `count(*)` over user tables — at chain scale (10s of millions of
//! accounts) an exact count is a full heap scan that would dominate this
//! hot dashboard path. Reltuples is refreshed by autovacuum / ANALYZE
//! and is well within the accuracy a "total accounts" UI cell needs.
//!
//! Repeat-call traffic is absorbed by the version-keyed in-process cache (see
//! `cache.rs`); the DB sees one statement per chain head (~1 / ledger).

use sqlx::{PgPool, Row};

use super::dto::NetworkStats;

/// Run the canonical network-stats statement against `pool` and assemble
/// the response DTO. `head` is the chain head the caller version-keys the
/// cache on (`crate::common::head`); the statement **pins** the latest-ledger
/// row to `WHERE sequence = head` rather than re-deriving "latest" itself, so
/// the response's `latest_ledger_sequence` always equals the cache key (no
/// TOCTOU between the head read and this query, and no divergence between
/// `max(sequence)` and `ORDER BY closed_at DESC`).
///
/// Returns `sqlx::Error` on any DB failure; the handler turns this into
/// the canonical `db_error` envelope. Written in raw SQL (not the
/// `query!` macro) so it does not require `DATABASE_URL` at build time
/// — consistent with `crates/api/src/transactions/queries.rs`.
///
/// Field naming and semantics match canonical SQL one-for-one:
/// `latest_ledger_closed_at` is the raw close-time of the head
/// ledger; `generated_at` is `NOW()` at SELECT time. Frontend uses
/// the pair to derive two distinct signals — indexer-health lag
/// (`generated_at − latest_ledger_closed_at`) and cache staleness
/// (`Date.now() − generated_at`) — without confusing them when the
/// version-keyed cache replays a stored response.
///
/// `::float8` matches canonical SQL — TPS is a 0–1000 display metric
/// with FE-side rounding, f64 has 14-digit headroom, and avoiding the
/// `rust_decimal` dep keeps the cache-miss decode path native.
///
/// Empty-`ledgers` case (cold-bootstrap cluster, no rows ingested yet):
/// the caller passes `head = 0`, `WHERE sequence = 0` matches no row, the
/// SELECT yields zero rows, and we map that to a zero-valued response with
/// `latest_ledger_closed_at = None` and `generated_at = Utc::now()`.
pub async fn fetch_stats(pool: &PgPool, head: i64) -> Result<NetworkStats, sqlx::Error> {
    let row_opt = sqlx::query(
        "SELECT \
            latest.sequence AS latest_ledger_sequence, \
            latest.closed_at AS latest_ledger_closed_at, \
            now() AS generated_at, \
            ( \
                SELECT COALESCE( \
                    SUM(transaction_count)::float8 \
                        / NULLIF(EXTRACT(EPOCH FROM (MAX(closed_at) - MIN(closed_at))), 0), \
                    0 \
                )::float8 \
                FROM ledgers \
                WHERE closed_at >= now() - INTERVAL '60 seconds' \
            ) AS tps_60s, \
            (SELECT reltuples::bigint FROM pg_class \
                WHERE oid = 'public.accounts'::regclass) AS total_accounts, \
            (SELECT reltuples::bigint FROM pg_class \
                WHERE oid = 'public.soroban_contracts'::regclass) AS total_contracts \
         FROM ( \
             SELECT sequence, closed_at \
             FROM ledgers \
             WHERE sequence = $1 \
         ) latest",
    )
    .bind(head)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row_opt else {
        // `head = 0` (empty cluster) or the head row vanished — no latest
        // ledger to report. Sequence 0 is a safe sentinel (Stellar genesis is
        // ledger 1); close-time is undefined. `generated_at` falls back to
        // wall-clock now (microsecond-equivalent to the DB's `now()` call).
        return Ok(NetworkStats {
            tps_60s: 0.0,
            total_accounts: 0,
            total_contracts: 0,
            latest_ledger_sequence: 0,
            latest_ledger_closed_at: None,
            generated_at: chrono::Utc::now(),
        });
    };

    Ok(NetworkStats {
        tps_60s: row.get("tps_60s"),
        total_accounts: row.get("total_accounts"),
        total_contracts: row.get("total_contracts"),
        latest_ledger_sequence: row.get("latest_ledger_sequence"),
        latest_ledger_closed_at: row.get("latest_ledger_closed_at"),
        generated_at: row.get("generated_at"),
    })
}
