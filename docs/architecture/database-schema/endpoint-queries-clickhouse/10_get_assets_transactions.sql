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
--   $4  :cursor_app_order Int16 NULL on first page
-- Indexes:      operation_asset_appearances ORDER BY (asset_id, ledger_sequence,
--                 application_order) — `asset_id` IS the leading PK, so the driver
--                 seek is a bounded PK-prefix range read (no bloom, no scan; the
--                 leading-key seek is itself the perf fix vs the old NON-leading
--                 density-scan). The `max(sequence)` commit fence keeps the seek
--                 behind the ledgers marker; `LIMIT 1 BY (ledger, tx)` collapses
--                 the multi-op-per-tx fan-out to one row per tx.
--               transactions ORDER BY (ledger_sequence, application_order)
--                 + PARTITION BY intDiv — headers fetched by their full key.
--               accounts `idx_acc_id` bloom — source StrKey by id seek.
--               transaction_operations ORDER BY (ledger_sequence,
--                 application_order, operation_index) — the operation_types
--                 aggregate is a primary-key seek on the page positions
--                 (task 0372).
-- CH Engine:    All ReplacingMergeTree. The driver dedups via `LIMIT 1 BY` (no
--                 FINAL on the seek); the header/aggregate step is the shared
--                 non-correlated two-step (crates/api/src/common/ch.rs).
-- Notes:
--   • Native (asset_type=0) is FIRST-CLASS: asset_id = ids::asset_id(0,'',0,0),
--     a stable non-zero surrogate, so /assets/native/transactions returns real
--     native activity — no longer "out of scope" as on the old path.
--   • `operation_types` is the per-tx aggregate over `transaction_operations`,
--     fetched NON-correlated (page positions → GROUP BY ledger_sequence,
--     application_order; task 0372) via `fetch_tx_list_aggregates` — CH 26.3+
--     rejects the correlated scalar subquery (`… WHERE oa.transaction_id =
--     t.id`) with Code 48 NOT_IMPLEMENTED.
--   • The page keys are integers, inlined as literal `IN (…)` lists with the
--     touched partitions — the literals in steps 2 and 3 are examples.
--   • Cursor tuple (ledger_sequence, application_order) — the transaction's
--     position (task 0575) — matches the seek's key order, so a page is in
--     execution order inside a ledger. A surrogate cursor minted before 0575
--     answers 400 `invalid_cursor`.
--   • One source: a transaction is listed when an operation names the asset
--     or a token event (transfer / mint / burn / clawback, task 0383) moved it.
--     A second arm over the asset's contract (its own contract or its SAC) was
--     removed by task 0575: on 1,000 measured ledgers it added only failed
--     Soroban calls, reads that move nothing (`balance`) and the contract's own
--     non-token activity — 46% of a type-3 token's rows, 1.8% of a SAC's. The
--     contract page lists that activity.
--   • ⚠️ Backfill dependency: the fan-out is populated by the Soroban-era XDR
--     re-parse; until it runs, this query returns only post-deploy classic
--     history. Run the backfill in the SAME rollout as this read swap.
-- ============================================================================

-- Step 1 — asset-leading seek (one row per tx) behind the commit fence.
SELECT ledger_sequence, application_order
FROM operation_asset_appearances
WHERE asset_id = $1
  AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  AND ($3 IS NULL OR (ledger_sequence, application_order) < ($3, $4))
ORDER BY ledger_sequence DESC, application_order DESC
LIMIT 1 BY ledger_sequence, application_order
LIMIT $2;

-- @@ split @@

-- Step 2 — transaction headers for the page keys from Step 1 (the (ledger,
--          application_order) tuples the seek returned, inlined as an
--          IN-tuple list with the touched partitions). The source StrKey
--          resolves by surrogate-id key seek (`resolve_accounts`, task 0354),
--          not a JOIN to accounts.
SELECT
    lower(hex(t.hash))  AS hash,
    t.ledger_sequence   AS ledger_sequence,
    t.application_order AS application_order,
    t.source_id         AS source_id,
    t.fee_charged       AS fee_charged,
    t.successful        AS successful,
    t.operation_count   AS operation_count,
    t.has_soroban       AS has_soroban,
    l.closed_at         AS created_at
FROM transactions t
INNER JOIN ledgers l ON l.sequence = t.ledger_sequence
WHERE (t.ledger_sequence, t.application_order) IN ((64000123, 12), (63990001, 3))
  AND intDiv(t.ledger_sequence, 500000) IN (127, 128);

-- @@ split @@

-- Step 3 — `operation_types` for the same page positions (shared helper, see
--          02 statement D), keyed by position since task 0372. Rows are
--          emitted in the Step 1 keyset order, merged by position in Rust.
SELECT ledger_sequence, application_order, groupUniqArray(type) AS codes
FROM transaction_operations
WHERE (ledger_sequence, application_order) IN ((64000123, 12), (63990001, 3))
  AND intDiv(ledger_sequence, 500000) IN (127, 128)
GROUP BY ledger_sequence, application_order;
