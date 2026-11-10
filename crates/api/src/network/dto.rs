//! Response DTO for `GET /v1/network/stats`.
//!
//! Wire shape per task 0045; the query is `network::queries::fetch_stats`.
//! The frontend consumes this endpoint on every Home dashboard load —
//! see `docs/architecture/frontend/frontend-overview.md` §6.2.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Top-level chain overview returned by `GET /v1/network/stats`.
///
/// `total_accounts` and `total_contracts` are deduped read-time counts —
/// see the per-field docs for the exactness each one carries. (They were
/// `system.tables.total_rows` planner estimates until 0420, which counted
/// unmerged ReplacingMergeTree duplicates and over-reported; that source is
/// gone.) `latest_ledger_closed_at` is `None` only on a cold-bootstrap
/// cluster where no ledger has been indexed yet.
///
/// `generated_at` is the wall-clock time the underlying SELECT ran on
/// the DB. Cache hits keep the original value, so frontend can derive
/// two distinct signals without confusing them with cache age:
///
/// * indexer-health lag = `generated_at - latest_ledger_closed_at`
/// * data staleness ("info from N seconds ago") = `Date.now() - generated_at`
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkStats {
    /// Operations per second over the 720 ledgers (~1 h) ending at head,
    /// including the operations of failed transactions — the figure
    /// Chainspect reports for Stellar. Computed as the window's
    /// `SUM(transactions.operation_count)` divided by the span between its
    /// MIN/MAX `closed_at`; `0` when the span is zero (a single-ledger
    /// cluster).
    pub tps: f64,
    /// Indexed account count: `count()` over `accounts_recent`, the deduped
    /// MV the `/accounts` list also pages from — so this KPI and that list
    /// agree. Exact to ±1 vs `accounts FINAL`, modulo the ~2-minute MV
    /// refresh. Safe to render as a total.
    pub total_accounts: i64,
    /// Indexed Soroban contract count: `count() FROM soroban_contracts FINAL`.
    /// Exact — `FINAL` is affordable on a table this size.
    pub total_contracts: i64,
    /// Sequence of the newest ledger in the database (ordered by
    /// `closed_at DESC`). `0` sentinel indicates an empty cluster (no
    /// ledger 0 exists in Stellar).
    pub latest_ledger_sequence: i64,
    /// Close time of the newest ledger in the database. `null` only
    /// when no ledgers are indexed (cold-bootstrap cluster).
    pub latest_ledger_closed_at: Option<DateTime<Utc>>,
    /// Wall-clock time the underlying SELECT ran on the DB (`NOW()` at
    /// fetch time). Preserved across cache hits — frontend uses this to
    /// distinguish indexer-health lag from cache-age staleness.
    pub generated_at: DateTime<Utc>,
}
