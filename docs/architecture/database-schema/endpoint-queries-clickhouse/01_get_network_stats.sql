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
-- Indexes:      ledgers PK on (sequence); accounts_recent (plain MergeTree,
--               count() is a metadata read); soroban_contracts FINAL.
-- CH Engine:    ledgers — ReplacingMergeTree; deduped by `LIMIT 1` / `LIMIT 1
--                 BY sequence` rather than FINAL (lore-0420/0422).
--               accounts_recent — refreshable-MV copy of `accounts`, already one
--                 row per account.
--               soroban_contracts — ReplacingMergeTree, FINAL (≈146k rows).
-- CH Pattern:   scalar subselects; no FINAL on ledgers; time math via
--               dateDiff('second', ...).
-- ADR 0044 §:   §4.1 (ledgers), §4.5 (Replacing state).
-- Notes:
--   • `latest_ledger_closed_at` powers the §7 "polling indicator — when
--     data was last refreshed" UI element. Same semantics as PG.
--   • `total_accounts` / `total_contracts` do NOT read
--     `system.tables.total_rows` (lore-0420): that is the PHYSICAL part-row
--     count, so on a ReplacingMergeTree it counts unmerged duplicates and
--     reads too high (measured +4.3% accounts / +11.6% contracts, drifting
--     upward). accounts → `count()` over `accounts_recent` (exact to ±1 vs
--     `accounts FINAL`, modulo the 2-minute MV refresh); soroban_contracts →
--     `count()` over FINAL, affordable at its size.
--   • TPS is `sum(transaction_count) / window_seconds` over the closed
--     ledgers in the trailing 60s, computed from the actual span between
--     min/max closed_at in the window. `nullIf(.., 0)` guards a 0 window
--     (single-ledger or empty range). Same numerical semantics as PG E01.
--   • `generated_at` is `now()` at SELECT time. The API caches the
--     assembled response in-process, version-keyed on the head `$1` (one compute per
--     chain head; a 60 s backstop TTL bounds memory / a stalled head); cache
--     hits return the original `generated_at` so the frontend can split
--     indexer-health lag from data staleness.

SELECT
    latest.sequence                                                              AS latest_ledger_sequence,
    latest.closed_at                                                             AS latest_ledger_closed_at,
    now64()                                                                      AS generated_at,
    toFloat64(ifNull(
        (SELECT sum(transaction_count)
                / nullIf(dateDiff('second', min(closed_at), max(closed_at)), 0)
         FROM (
             SELECT sequence, transaction_count, closed_at
             FROM ledgers
             WHERE sequence > $1 - 200                  -- prune to recent ~200 ledgers
               AND closed_at >= now64() - INTERVAL 60 SECOND
             LIMIT 1 BY sequence                        -- dedup unmerged RMT rows
         )),
        0
    ))                                                                           AS tps_60s,
    ifNull((SELECT count() FROM accounts_recent), 0)                             AS total_accounts,
    ifNull((SELECT count() FROM soroban_contracts FINAL), 0)                     AS total_contracts
FROM (
    SELECT sequence, closed_at
    FROM ledgers
    WHERE sequence = $1   -- pinned to the cache-key head (PK point read)
    LIMIT 1               -- a duplicated head row would return two rows
) AS latest;
