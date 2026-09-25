-- ============================================================================
-- ⚠️  API-SHAPE CORRECTION (task 0386) — the per-row `contract_ids` array is
--     REMOVED from the transaction-list API entirely. It was PG-parity
--     scaffolding no frontend rendered; computing it forced a whole-table
--     `JOIN soroban_contracts FINAL` (~200k rows/page). The live list response
--     now carries `operation_types` only. The `contract_surrogate_ids` /
--     `contract_ids` projections the statements below once carried were
--     removed when this file was rewritten to the live shape (task 0372).
--     Supersedes the ops-only note below.
-- ============================================================================
-- ⚠️  CH 26.3 CORRECTION (task 0243) — do NOT compute `operation_types` with a
--     correlated scalar subquery in the projection (`SELECT
--     groupUniqArray(oa.type) … WHERE oa.transaction_id = t.id`, the shape this
--     file once showed). ClickHouse 26.3 rejects it:
--       Code: 48 NOT_IMPLEMENTED: can't find correlated column …
--     The live read path uses a NON-correlated two-step (validated on prod
--     CH 26.3): fetch the page, then aggregate per
--     `(ledger_sequence, application_order) IN (…)` grouped by that position
--     (statement D below; keyed by `transaction_id` until task 0372).
--     Reuse the shared Rust helper
--     `crates/api/src/common/ch.rs::fetch_tx_list_aggregates` for any new
--     transaction-list module.
-- ============================================================================
-- NOTE: the "value moved" / `values` addition (task 0393) was REMOVED on
--     2026-09-04 — the response no longer carries it and the helper runs the
--     op-types aggregate only.
--     NB the table is `asset_id`-leading, so that filter scans the pruned
--     partition (not a seek). Read-path optimisation is an OPEN 0393 follow-up:
--     no index/projection is baked in — the mechanism must come from a concrete
--     load measurement of this endpoint. Amounts are RAW Int128 (client scales;
--     classic/SAC = 7). Not shown in the statements below.
-- ============================================================================
-- ⚠️  CH READ-COST CORRECTION (task 0243) — `contract_ids` is OPS-ONLY in the
--     live read path. The `arrayConcat`/UNION over operations_appearances +
--     soroban_invocations_appearances + soroban_events that statements B/C
--     once showed was NOT what ran. Both soroban_* tables are ORDER BY
--     (contract_id, …), so the per-page `(ledger_sequence, transaction_id) IN
--     (…)` key filter is a PARTITION SCAN on them, not a key seek. In
--     production a single /transactions page read ~1e8 rows and a handful of
--     requests exhausted the api_reader read_rows hourly quota
--     (CH Code: 201 QUOTA_EXCEEDED), 500-ing every CH endpoint.
--     The live helper sources `contract_ids` from operations_appearances ONLY
--     (primary-key seek). PARITY COST: a contract touched solely via a nested
--     sub-invocation or an emitted event (never a root-op contract_id) is not
--     listed; for the vast majority of Soroban tx the invoked contract IS the
--     root-op contract_id, so list-row contract_ids match PG in practice.
--     A cheap full-parity path (skip-index on transaction_id, or a precomputed
--     per-tx contract_ids column) is a deferred follow-up.
-- ============================================================================
-- ⚠️  CH FINAL READ-COST CORRECTION (task 0243) — `... FINAL ... ORDER BY ...
--     LIMIT` over a partition reads the WHOLE partition: FINAL must merge it
--     before the limit applies (~1.2e8 transactions on the mainnet head).
--     Live read path:
--     • Statement A (no filter, polled) DROPS FINAL and orders/keys on the
--       physical sort key `(ledger_sequence, application_order)` so CH reads in
--       primary-key order and stops at the limit (~2e5 rows/page, validated).
--       Cursor tie-break is `application_order` (also the correct in-ledger
--       order — the `id` hash tie-break it replaced did NOT preserve it).
--     • Statements B/C (filtered) do not join their driver to `transactions`:
--       the driver returns up to `$1 * 4` positions and `transactions` is
--       sought by `(ledger_sequence, application_order) IN (…)` (tasks 0541,
--       0372). The join they replaced, to an unpruned `transactions t FINAL`,
--       merged the whole 3.6B-row table per request (measured; blew the
--       read_rows quota, Code: 201). The contract driver is one seek on the
--       `contract_transactions` presence index (task 0541; before it the
--       `idx_oa_contract_id` bloom skip-index on `operations_appearances`,
--       task 0333 — box-measured 13.18 M read_rows / 1609 granules → 245 K /
--       32 granules). The `type` driver (Statement C) scans one partition of
--       `transaction_operations` — `type` is not a key prefix.
--     • ledgers (04) + network (01) are ORDER BY `sequence`, NOT `closed_at`;
--       drive their reads off `sequence` (monotonic with closed_at) to stay on
--       the primary key.
--     FINAL is kept only for single-key reads (detail, embedded ledger tx)
--       where it is cheap. The `operation_types` aggregate (statement D) runs
--       without it since task 0372: `groupUniqArray` collapses an unmerged
--       duplicate on its own.
-- ============================================================================
-- Endpoint:     GET /transactions
-- Purpose:      Paginated list of transactions. Optional filters:
--               source_account, contract_id, operation_type.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.3
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment.
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                Int     page size (the page reads `$1 + 1`, the
--                                     peek row that detects a next page)
--   $2  :cursor_ledger        Int64   NULL on first page (drives partition prune)
--   $3  :cursor_app_order     Int16   NULL on first page — the cursor is the
--                                     transaction's position under every filter
--                                     (tasks 0541, 0372); a surrogate cursor
--                                     answers 400 `invalid_cursor`
--   $4  :source_account_id    Int64   NULL = no filter (cityhash64 of source StrKey)
--   $5  :contract_id          Int64   NULL = no filter (cityhash64 of C-StrKey)
--   $6  :op_type              Int16   NULL = no filter
--   $7  :latest_partition     Int64   API computes `intDiv(latest_ledger, 500000)`
--                                     and passes as scan-window upper bound;
--                                     first page only — drops the full-table-FINAL
--                                     scan that previously blew CH memory limit.
-- Indexes:      transactions ORDER BY (ledger_sequence, application_order)
--                 + PARTITION BY intDiv(ledger_sequence, 500000).
--               contract_transactions ORDER BY (contract_id, ledger_sequence,
--                 application_order) — Statement B's driver (task 0541).
--               transaction_operations ORDER BY (ledger_sequence,
--                 application_order, operation_index) — Statement C's driver
--                 (a partition scan by `type`), B's operation-type filter and
--                 the `operation_types[]` aggregate (both position seeks)
--                 (task 0372).
--               accounts `idx_acc_id` bloom + ledgers PK (sequence) — the
--                 source StrKey and `created_at` resolve by key seek.
-- CH Engine:    All ReplacingMergeTree. No FINAL on any statement here: A drops
--               an unmerged duplicate in Rust, B/C with `LIMIT 1 BY`, D with
--               `groupUniqArray`.
-- CH Pattern:   A (no filter) reads `transactions` directly; B (contract) and C
--               (op_type) collect positions from a driver, then seek
--               `transactions` at those positions; D aggregates
--               `operation_types` for the page. A and C: partition prune ALWAYS
--               applied — `$2 IS NULL` first-page path uses `$7`
--               (caller-supplied latest_partition) to bound the scan to one
--               partition. Without this, full-table FINAL on 20M+ tx rows blows
--               the 5.6 GB default memory limit.
-- ADR 0044 §:   §4.2 (transactions), §4.3 (operations_appearances et al.),
--               §4.4 (soroban_events — full payload §5.1), §4.5 (accounts).
--   **PR #175 amendment:** memory blowup observed on first-page no-filter
--   path. Fix: caller passes `$7 = intDiv(max_ledger_sequence, 500000)`
--   so we partition-prune from the start. Frontend can derive `$7` from
--   the latest_ledger_sequence in E01's response or cache it for ~5s.
-- Notes:
--   • A and C: partition prune via `intDiv(ledger_sequence, 500000) = ifNull($2_part, $7)`
--     where `$2_part = intDiv($2, 500000)` when cursor set, else `$7`.
--     Limits scan to ONE partition (~500k ledgers worst case).
--   • The page keys (the positions B and C collect, and the page rows D
--     aggregates for) are integers, inlined as literal `IN (…)` lists with the
--     touched partitions — the literals below are examples. The API inlines
--     every value of B and C: the bound-parameter path returned empty pages.
--   • `source_account` and `created_at` are not joined: the API resolves them
--     for the page by key seeks (`accounts WHERE id IN (…)` on the
--     `idx_acc_id` bloom, `ledgers WHERE sequence IN (…)`), task 0290.

-- ============================================================================
-- Statement A — no contract / op_type filter (default path)
-- ============================================================================
-- Wrap in subquery so the LIMIT applies BEFORE anything else touches the rows.
-- `ledger_sequence <= max(sequence)` keeps the page behind the newest ledger
-- row: a transaction can be visible slightly before its ledger, and the page
-- needs the ledger's `closed_at`.
SELECT
    lower(hex(t.hash))                              AS hash,
    t.ledger_sequence                               AS ledger_sequence,
    t.application_order                             AS application_order,
    t.source_id                                     AS source_id,
    t.fee_charged                                   AS fee_charged,
    lower(hex(t.inner_tx_hash))                     AS inner_tx_hash,
    t.successful                                    AS successful,
    t.operation_count                               AS operation_count,
    t.has_soroban                                   AS has_soroban
FROM (
    SELECT *
    FROM transactions
    WHERE intDiv(ledger_sequence, 500000) = ifNull(intDiv($2, 500000), $7)
      AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
      AND ($2 IS NULL OR (ledger_sequence, toInt64(application_order)) < ($2, $3))
      AND ($4 IS NULL OR source_id = $4)
    ORDER BY ledger_sequence DESC, application_order DESC
    LIMIT $1 + 1
) t
ORDER BY t.ledger_sequence DESC, t.application_order DESC;

-- @@ split @@

-- ============================================================================
-- Statement B — contract filter set (with or without op_type)
-- ============================================================================
-- Step 1: driven by one seek on the `contract_transactions` presence index
-- (task 0541) — the shape `transaction_participants` gives the account list. It
-- replaced a UNION over `operations_appearances`,
-- `soroban_invocations_appearances` and `soroban_events` that hashed the whole
-- partition (6.04 GiB for native XLM, over the reader cap).
-- NOT bounded to one partition (task 0381): the index is keyed by contract, so
-- the seek crosses partitions cheaply, and bounded to the head's partition a
-- contract quiet there listed as empty — 93.3% of contracts on 2026-09-22.
-- One row per (contract, transaction); `LIMIT 1 BY` collapses rows the RMT has
-- not merged yet (no FINAL — it would merge the contract's rows across every
-- part).
SELECT ledger_sequence, application_order
FROM contract_transactions
WHERE contract_id = $5
  AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  AND ($2 IS NULL OR (ledger_sequence, application_order) < ($2, $3))
ORDER BY ledger_sequence DESC, application_order DESC
LIMIT 1 BY ledger_sequence, application_order
LIMIT $1 * 4;

-- @@ split @@

-- Step 2: the page — `transactions` at the positions from step 1, filtered by
-- source and operation type. Both are position seeks with the partition prune.
-- The operation-type filter is a NON-correlated `IN` over
-- `transaction_operations` bounded to the same positions (task 0372; it was a
-- correlated `count()` over `operations_appearances` keyed by
-- `transaction_id`).
SELECT
    lower(hex(t.hash))                              AS hash,
    t.ledger_sequence                               AS ledger_sequence,
    t.application_order                             AS application_order,
    t.source_id                                     AS source_id,
    t.fee_charged                                   AS fee_charged,
    lower(hex(t.inner_tx_hash))                     AS inner_tx_hash,
    t.successful                                    AS successful,
    t.operation_count                               AS operation_count,
    t.has_soroban                                   AS has_soroban
FROM transactions t
WHERE (t.ledger_sequence, t.application_order) IN ((64000123, 12), (64000120, 3))
  AND intDiv(t.ledger_sequence, 500000) IN (128)
  AND ($4 IS NULL OR t.source_id = $4)
  AND ($6 IS NULL OR (t.ledger_sequence, t.application_order) IN (
        SELECT ledger_sequence, application_order
        FROM transaction_operations
        WHERE (ledger_sequence, application_order) IN ((64000123, 12), (64000120, 3))
          AND intDiv(ledger_sequence, 500000) IN (128)
          AND type = $6))
ORDER BY t.ledger_sequence DESC, t.application_order DESC
LIMIT 1 BY t.ledger_sequence, t.application_order
LIMIT $1 + 1;

-- @@ split @@

-- ============================================================================
-- Statement C — op_type filter only (no contract filter)
-- ============================================================================
-- Step 1: up to `$1 * 4` positions of transactions carrying an operation of the
-- type, from `transaction_operations` pinned to ONE partition (the cursor's, or
-- the head's on the first page). `type` is not a key prefix, so this scans the
-- partition in key order until the limit. Measured 2026-09-25 on partition 115
-- (286M rows), LIMIT 80 below its last ledger: 63–147M rows read for types 1,
-- 19 and 24, against 24–147M for the same driver on `operations_appearances`;
-- the new table still held the history fill's 10 unmerged parts there (the old
-- one, 1). User-initiated, not polled.
-- `LIMIT 1 BY` folds a transaction's several operations of the type — and rows
-- the RMT has not merged yet — into one position.
-- Step 2 is statement B's step 2 with `$6` = NULL: the positions already match
-- the type, so the page filters by source only.
-- The cursor is the position (task 0372; it was the `transactions.id`
-- surrogate, and such a cursor now answers 400 `invalid_cursor` once).
SELECT ledger_sequence, application_order
FROM transaction_operations
WHERE type = $6
  AND intDiv(ledger_sequence, 500000) = ifNull(intDiv($2, 500000), $7)
  AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  AND ($2 IS NULL OR (ledger_sequence, application_order) < ($2, $3))
ORDER BY ledger_sequence DESC, application_order DESC
LIMIT 1 BY ledger_sequence, application_order
LIMIT $1 * 4;

-- @@ split @@

-- ============================================================================
-- Statement D — `operation_types[]` for the page (every statement)
-- ============================================================================
-- `crates/api/src/common/ch.rs::fetch_tx_list_aggregates`, shared with the
-- ledger, account and asset transaction lists (05, 07, 10). Keyed by the page's
-- positions, so it is a primary-key seek on `transaction_operations`; merged
-- onto the page rows in Rust by position. A transaction with no operations row
-- gets an empty array. No FINAL: `groupUniqArray` collapses an unmerged
-- duplicate, and `type` is the same on every version of a row.
SELECT ledger_sequence, application_order, groupUniqArray(type) AS codes
FROM transaction_operations
WHERE (ledger_sequence, application_order) IN ((64000123, 12), (64000120, 3))
  AND intDiv(ledger_sequence, 500000) IN (128)
GROUP BY ledger_sequence, application_order;
