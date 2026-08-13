-- Endpoint:     GET /network/stats
-- Purpose:      Top-level chain summary for the home dashboard:
--               latest ledger sequence + close-time, TPS over a 60s window,
--               total accounts, total contracts, plus the wall-clock time
--               the SELECT was executed (for cache-aware freshness on the
--               client). Cacheable with a short TTL (5–15 s).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.2 + §7
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :head  Int64  the chain head (max ledger sequence) the API
--               version-keys its cache on (crate::common::head). The Rust
--               query inlines it as a trusted i64 via `format!`; documented
--               here as a positional parameter so this file is standalone
--               SQL the Tier-1 gate can parse. The latest row is PINNED to it
--               (WHERE sequence = $1) and the TPS window prunes on
--               sequence > $1 - 200, so latest_ledger_sequence always equals
--               the cache key and the previous inner
--               (SELECT max(sequence) ...) subquery is dropped.
--               $1 = 0 (empty cluster) matches no row -> zero response.
-- Indexes:      ledgers PK on (sequence); system.tables.total_rows for the
--               accounts / soroban_contracts estimates.
-- CH Engine:    ledgers — MergeTree (no FINAL needed).
--               accounts / soroban_contracts — totals come from
--               system.tables.total_rows so engine FINAL semantics are
--               irrelevant here (we want the planner estimate).
-- CH Pattern:   scalar subselects; no FINAL (ledgers immutable lookup);
--               toStartOfSecond-free time math via dateDiff('second', ...).
-- ADR 0044 §:   §4.1 (ledgers plain MergeTree).
-- Notes:
--   • `latest_ledger_closed_at` powers the §7 "polling indicator — when
--     data was last refreshed" UI element. Same semantics as PG.
--   • `total_accounts` and `total_contracts` use `system.tables.total_rows`
--     — ClickHouse's O(1) row-count estimate maintained by the storage
--     layer. Mirrors the PG pattern of `pg_class.reltuples` (planner
--     estimate, not exact). On an explorer-scale DB an exact `count()`
--     touches every part of every partition (still seconds even in CH)
--     and would dominate this hot dashboard query. If exact ever needed,
--     spawn a counter table — do NOT add count() here.
--   • TPS is `sum(transaction_count) / window_seconds` over the closed
--     ledgers in the trailing 60s, computed from the actual span between
--     min/max closed_at in the window. `nullIf(.., 0)` guards a 0 window
--     (single-ledger or empty range). Same numerical semantics as PG E01.
--   • `generated_at` is `now()` at SELECT time. The API caches the
--     assembled response in-process, version-keyed on {head} (one compute per
--     chain head; a 60 s backstop TTL bounds memory / a stalled head); cache
--     hits return the original `generated_at` so the frontend can split
--     indexer-health lag from data staleness.

SELECT
    latest.sequence                                                              AS latest_ledger_sequence,
    latest.closed_at                                                             AS latest_ledger_closed_at,
    now64()                                                                      AS generated_at,
    (
        SELECT toFloat64(
            ifNull(
                sum(transaction_count)
                    / nullIf(dateDiff('second', min(closed_at), max(closed_at)), 0),
                0
            )
        )
        FROM ledgers
        WHERE sequence > $1 - 200                  -- prune to recent ~200 ledgers
          AND closed_at >= now64() - INTERVAL 60 SECOND
    )                                                                            AS tps_60s,
    (SELECT total_rows FROM system.tables WHERE database = currentDatabase() AND name = 'accounts')          AS total_accounts,
    (SELECT total_rows FROM system.tables WHERE database = currentDatabase() AND name = 'soroban_contracts') AS total_contracts
FROM (
    SELECT sequence, closed_at
    FROM ledgers
    WHERE sequence = $1   -- pinned to the cache-key head (PK point read)
) AS latest;
