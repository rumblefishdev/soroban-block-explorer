-- ============================================================================
-- Endpoint:     GET /assets/:id/transactions
-- Purpose:      Paginated transactions touching a given asset, newest first.
--               Driver is the `operation_asset_appearances` fan-out (task 0359) —
--               a per-(asset, transaction) presence index keyed asset-first,
--               replacing the old two-variant identity predicate over
--               `operations_appearances` (`(asset_code, asset_issuer_id)` /
--               `contract_id`) that could not represent multi-asset ops and
--               modelled native XLM as absence.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.9
-- Schema:       task 0359 (`operation_asset_appearances`); ADR 0044 (transactions).
-- Data sources: DB-only.
-- Inputs:
--   $1  :asset_id       Int64   ids::asset_id surrogate. Native is a FIRST-CLASS
--                               non-zero key (ids::asset_id(0,'',0,0)); the API
--                               guards the unresolved id==0 sentinel with an empty
--                               page (no `WHERE asset_id = 0`).
--   $2  :limit          Int     page size (already the +1 finalize_page peek row)
--   $3  :cursor_ledger  Int64   NULL on first page
--   $4  :cursor_tx_id   Int64   NULL on first page
-- Indexes:      operation_asset_appearances ORDER BY (asset_id, ledger_sequence,
--                 transaction_id) — `asset_id` IS the leading PK, so the driver
--                 seek is a bounded PK-prefix range read (no bloom, no scan; the
--                 leading-key seek is itself the perf fix vs the old NON-leading
--                 density-scan). The `max(sequence)` commit fence keeps the seek
--                 behind the ledgers marker; `LIMIT 1 BY (ledger, tx)` collapses
--                 the multi-op-per-tx fan-out to one row per tx.
--               transactions ORDER BY (ledger_sequence, application_order)
--                 + PARTITION BY intDiv — headers fetched by (ledger, id) IN keys.
--               accounts ORDER BY (account_id) — source join via id.
-- CH Engine:    All ReplacingMergeTree. The driver dedups via `LIMIT 1 BY` (no
--                 FINAL on the seek); the header/aggregate step is the shared
--                 non-correlated two-step (crates/api/src/common/ch.rs).
-- Notes:
--   • Native (asset_type=0) is FIRST-CLASS: asset_id = ids::asset_id(0,'',0,0),
--     a stable non-zero surrogate, so /assets/native/transactions returns real
--     native activity — no longer "out of scope" as on the old path.
--   • `operation_types` is the per-tx aggregate over `operations_appearances`,
--     fetched NON-correlated (page keys → GROUP BY transaction_id) via
--     `fetch_tx_list_aggregates` — CH 26.3+ rejects the correlated scalar
--     subquery (`… WHERE oa.transaction_id = t.id`) with Code 48 NOT_IMPLEMENTED.
--   • Cursor tuple (ledger_sequence, transaction_id) matches the fan-out PK order.
--   • ⚠️ Backfill dependency: the fan-out is populated by the Soroban-era XDR
--     re-parse; until it runs, this query returns only post-deploy classic
--     history. Run the backfill in the SAME rollout as this read swap.
-- ============================================================================

-- Step 1 — asset-leading seek (one row per tx) behind the commit fence.
SELECT p.ledger_sequence, p.transaction_id
FROM operation_asset_appearances p
WHERE p.asset_id = $1
  AND p.ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  AND ($3 IS NULL OR (p.ledger_sequence, p.transaction_id) < ($3, $4))
ORDER BY p.ledger_sequence DESC, p.transaction_id DESC
LIMIT 1 BY p.ledger_sequence, p.transaction_id
LIMIT $2;

-- @@ split @@

-- Step 2 — transaction headers + `operation_types` aggregate for the page keys
--          from Step 1 (non-correlated two-step; `keys` = the (ledger, tx)
--          tuples the seek returned, inlined as an IN-tuple list).
SELECT
    lower(hex(t.hash))  AS hash_hex,
    t.ledger_sequence,
    src.account_id      AS source_account,
    t.fee_charged,
    t.successful,
    t.operation_count,
    t.has_soroban,
    t.id                AS cursor_tx_id
FROM transactions t FINAL
JOIN accounts src FINAL ON src.id = t.source_id
WHERE (t.ledger_sequence, t.id) IN ( /* page keys from Step 1 */ )
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $2;
