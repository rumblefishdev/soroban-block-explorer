-- Endpoint:     GET /v1/liquidity-pools/{pool_id}/activity
-- Purpose:      The pool's activity as a list of OPERATIONS, with a
--               `filter[event]` over trade / deposit / withdrawal. Supersedes
--               20 (`/transactions`), whose row was a transaction.
-- Source:       backend-overview.md §6.2; issue #371; task 0491
-- Schema:       pool_operation_amounts, transactions, transaction_operations,
--               ledgers, accounts; ADR 0044, ADR 0059 (task 0372)
-- Data sources: DB-only
-- Inputs:
--   $1 :pool_id           TEXT   64-char lowercase hex (from the L… strkey)
--   $ls $ao $oi           INT    keyset bound (absent on the first page): the
--                                transaction's position (ledger_sequence,
--                                application_order) and the operation's
--                                0-based operation_index — the cursor
--                                `{ledger_sequence, application_order,
--                                operation_index}`. A cursor minted before
--                                task 0372 (`transaction_id`) fails to decode
--                                and answers 400 `invalid_cursor`.
--   :window               INT    rows to read — 2 legs per operation + slack
-- Indexes:      pool_operation_amounts PK prefix (pool_id, ledger_sequence,
--               application_order, operation_index); transactions PK
--               (ledger_sequence, application_order) + partition prune;
--               transaction_operations PK prefix (ledger_sequence,
--               application_order) + partition prune; accounts bloom on id
-- CH Engine:    ReplacingMergeTree (pool_operation_amounts, transactions,
--               transaction_operations)
-- CH Pattern:   read-in-order top-N → pair legs in Rust → bounded IN-list
--               enrichment → surrogate StrKey resolve
-- ADR 0044 §:   §4.1 (RMT), §5.2 (ledgers holds the timestamp)
-- Notes:
--   • NO GROUP BY and NO FINAL — both measured, see the ⚠️ blocks below.
--   • The event has NO type column: it is the signs of the op's leg amounts,
--     and it is decided in Rust (`PoolEvent::from_signs`), not here. Amounts
--     are grouped per operation into one slot per pool leg (`legs` order).
--   • An operation with no amount rows is NOT listed (failed LP ops).
--   • The wire's `application_order` is the OPERATION's 1-based position
--     (`operation_index + 1`, the `#op-N` anchor); the tables store the
--     0-based index (ADR 0059).
-- ============================================================================
--
-- ⚠️  THE DRIVER TABLE IS THE WHOLE DESIGN.
--     `operation_pools` (dropped in task 0372) was keyed (pool_id,
--     ledger_sequence, transaction_id) with NO application_order, so it could
--     not page per operation.
--     `pool_operation_amounts` is keyed
--     (pool_id, ledger_sequence, application_order, operation_index, asset_id)
--     — exactly this page's grain, reached by one PK-prefix seek. Since task
--     0372 the transaction is located by its position; the predecessor
--     `lp_operation_amounts` (same rows, same grain) located it by the
--     `transaction_id` surrogate and is still written until it is dropped.
--
-- ⚠️  DO NOT REINTRODUCE THE `GROUP BY`. The first implementation pivoted the
--     two legs with countIf/anyIf and grouped by the key triple. Measured on
--     prod's busiest pool (1.68M leg rows), returning 21 operations:
--
--       GROUP BY pivot            2.60M rows   109 ms   182 MiB
--       + optimize_aggregation_in_order
--                                 2.60M rows   253 ms   230 MiB   (no help)
--       + FINAL                   3.17M rows   110 ms   195 MiB
--       read-in-order + pair      0.115M rows    9 ms    11 MiB   ← shipped
--       (per-transaction ep. 20)  0.159M rows   11 ms    11 MiB
--
--     A GROUP BY must consume the pool's WHOLE slice before ORDER BY … LIMIT
--     can pick the newest 21. Reading in sort-key order stops at the window.
--     Figures are medians of 3 — a COLD run of either shape reads 0.7–1.0M
--     rows, so single runs can invert the comparison. Measured on
--     `lp_operation_amounts`, before task 0372 renamed the key; the shape of
--     the read is unchanged.
--
-- ⚠️  NO `FINAL`, and it is not the cost anyway (+22%, not an order of
--     magnitude). `pool_operation_amounts` is a ReplacingMergeTree whose
--     producer is deterministic (see the schema header's single-writer
--     argument), so an unmerged duplicate is byte-identical to its twin and
--     picking either leg row gives the same answer.
--
-- ⚠️  KNOWN CONSEQUENCE: an operation with NO amount rows is not listed.
--     An op DECLARES a pool (its `pool_ids`) whether or not the transaction
--     succeeded, but the indexer writes amounts only for
--     value that actually moved (claim atoms for trades, the op's own
--     LedgerEntryChanges for deposits/withdrawals). A FAILED explicit LP op
--     therefore appeared under `/transactions` and does not appear here.
--     Deliberate: the page answers "what moved through this pool", a failed
--     op moved nothing, and this narrows a known CH-vs-Horizon breadth
--     difference rather than widening it.
--
-- Shipped module: `crates/api/src/liquidity_pools/queries/list_pool_activity.rs::fetch_pool_activity`
-- ============================================================================

-- STEP 1 — the page window, read in sort-key order. One row per LEG; the two
-- legs of an operation are ADJACENT because `asset_id` is the last key
-- component, so Rust folds them into operations without an aggregation.
--
-- The keyset compares the whole triple (transaction position + operation
-- index), and both legs of an operation share
-- it, so `<` steps over the previous operation entirely — there is never half
-- an operation to skip.
--
-- `:window` is 2×page + slack when unfiltered, which is one round trip. With
-- `filter[event]` the matching rate is unknown before the legs are paired, so
-- the caller doubles the window and re-reads until the page fills — geometric,
-- so O(log) round trips.
SELECT
    ledger_sequence   AS ls,
    application_order AS ao,
    operation_index   AS oi,
    asset_id          AS asset_id,
    amount            AS amount
FROM pool_operation_amounts
WHERE pool_id = toFixedString(unhex($1), 32)
  AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  -- AND (ledger_sequence, application_order, operation_index) < ($ls, $ao, $oi)
ORDER BY ls DESC, ao DESC, oi DESC
LIMIT 44;                          -- 2 × (page 20 + 1) + 2

-- @@ split @@
-- STEP 2 — transaction-level enrichment for the page's DISTINCT tx keys.
-- Smaller than the page: several operations of one transaction share it.
-- Keyed by the transaction's position (task 0372), inlined (integers) with
-- the partition prune that turns the IN filter into a tight PK seek, same
-- shape as `common::ch::fetch_tx_list_aggregates`.
SELECT
    t.ledger_sequence                   AS ls,
    t.application_order                 AS ao,
    lower(hex(t.hash))                  AS hash,
    t.source_id                         AS source_id,
    toUnixTimestamp64Milli(l.closed_at) AS created_at_ms
FROM transactions t
INNER JOIN ledgers l ON l.sequence = t.ledger_sequence
WHERE (t.ledger_sequence, t.application_order) IN ((58123456, 14), (58123455, 3))
  AND intDiv(t.ledger_sequence, 500000) IN (116)
LIMIT 1 BY t.ledger_sequence, t.application_order;

-- @@ split @@
-- STEP 3 — the OPERATION's own source account.
--
-- ⚠️  NOT the transaction's. A Stellar operation may declare its own source,
--     and then it is who performed this operation; `source_id` is NULL when it
--     declares none, which per the XDR means "the transaction's". On a
--     per-operation row the transaction's source is simply the wrong account
--     whenever the two differ — measured on prod, 41% of operations in a
--     recent ledger window declare their own, and stellar.expert shows that
--     one. The retired endpoint 20 could only carry the transaction's, because
--     its row WAS a transaction.
--
-- `(ledger_sequence, application_order, operation_index)` IS the sort key of
-- `transaction_operations` (task 0372), so the bounded IN-list on the
-- transaction positions is a PK-prefix seek with the same partition prune.
-- `max()` rather than `LIMIT 1 BY`: aggregation folds rows the RMT has not
-- merged yet and skips the NULLs instead of picking a row arbitrarily.
-- The table holds one row per folded identity group, at the group's smallest
-- `operation_index`: an operation folded into an earlier identical one has no
-- row of its own, so its page row falls back to the transaction's source and
-- carries no `pools_crossed` (the same as on `operations_appearances`).
-- `pools_crossed` rides the same seek: `pool_ids` is the op's sorted+deduped
-- crossing list, so max(length()) is just "the length". It lets a row say
-- "one hop of an N-pool route" without carrying the route (that lives on the
-- op detail page the row links to).
-- Cost of the array column, measured on `operations_appearances` before task
-- 0372: 8.4k → 20.6k read_rows, same 7 ms, <1 MiB — noise against the 115k
-- page read.
SELECT
    ledger_sequence   AS ls,
    application_order AS ao,
    operation_index   AS oi,
    max(source_id)    AS source_id,
    max(length(pool_ids)) AS pools_crossed
FROM transaction_operations
WHERE (ledger_sequence, application_order) IN ((58123456, 14), (58123455, 3))
  AND intDiv(ledger_sequence, 500000) IN (116)
GROUP BY ls, ao, oi;

-- STEP 4 — source StrKeys by surrogate id (bloom seek), NOT a whole-`accounts`
-- INNER JOIN (task 0354). `common::ch::resolve_accounts`, one call for both
-- the operation sources above and the transaction sources from STEP 2.
