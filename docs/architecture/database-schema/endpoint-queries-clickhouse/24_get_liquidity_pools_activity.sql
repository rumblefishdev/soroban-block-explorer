-- Endpoint:     GET /v1/liquidity-pools/{pool_id}/activity
-- Purpose:      The pool's activity as a list of OPERATIONS, with a
--               `filter[event]` over trade / deposit / withdrawal. Supersedes
--               20 (`/transactions`), whose row was a transaction.
-- Source:       backend-overview.md §6.2; issue #371; task 0491
-- Schema:       lp_operation_amounts, transactions, ledgers, accounts; ADR 0044
-- Data sources: DB-only
-- Inputs:
--   $1 :pool_id           TEXT   64-char lowercase hex (from the L… strkey)
--   $ls $tid $ao          INT    keyset bound (absent on the first page)
--   :window               INT    rows to read — 2 legs per operation + slack
-- Indexes:      lp_operation_amounts PK prefix (pool_id, ledger_sequence,
--               transaction_id, application_order); transactions PK
--               (ledger_sequence, application_order) + partition prune;
--               accounts bloom on id
-- CH Engine:    ReplacingMergeTree (lp_operation_amounts, transactions)
-- CH Pattern:   read-in-order top-N → pair legs in Rust → bounded IN-list
--               enrichment → surrogate StrKey resolve
-- ADR 0044 §:   §4.1 (RMT), §5.2 (ledgers holds the timestamp)
-- Notes:
--   • NO GROUP BY and NO FINAL — both measured, see the ⚠️ blocks below.
--   • The event has NO type column: it is the sign pair of the two legs, and
--     it is decided in Rust (`PoolEvent::from_signs`), not here.
--   • An operation with no amount rows is NOT listed (failed LP ops).
-- ============================================================================
--
-- ⚠️  THE DRIVER TABLE IS THE WHOLE DESIGN.
--     `operation_pools` is keyed (pool_id, ledger_sequence, transaction_id)
--     with NO application_order, so it cannot page per operation.
--     `lp_operation_amounts` is keyed
--     (pool_id, ledger_sequence, transaction_id, application_order, asset_id)
--     — exactly this page's grain, reached by one PK-prefix seek.
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
--     rows, so single runs can invert the comparison.
--
-- ⚠️  NO `FINAL`, and it is not the cost anyway (+22%, not an order of
--     magnitude). `lp_operation_amounts` is a ReplacingMergeTree whose
--     producer is deterministic (see the schema header's single-writer
--     argument), so an unmerged duplicate is byte-identical to its twin and
--     picking either leg row gives the same answer.
--
-- ⚠️  KNOWN CONSEQUENCE: an operation with NO amount rows is not listed.
--     The indexer writes `operation_pools` for an op that DECLARES a pool
--     whether or not the transaction succeeded, but writes amounts only for
--     value that actually moved (claim atoms for trades, the op's own
--     LedgerEntryChanges for deposits/withdrawals). A FAILED explicit LP op
--     therefore appeared under `/transactions` and does not appear here.
--     Deliberate: the page answers "what moved through this pool", a failed
--     op moved nothing, and this narrows a known CH-vs-Horizon breadth
--     difference rather than widening it.
--
-- Shipped module: `crates/api/src/liquidity_pools/queries.rs::fetch_pool_activity`
-- ============================================================================

-- STEP 1 — the page window, read in sort-key order. One row per LEG; the two
-- legs of an operation are ADJACENT because `asset_id` is the last key
-- component, so Rust folds them into operations without an aggregation.
--
-- The keyset compares the whole triple, and both legs of an operation share
-- it, so `<` steps over the previous operation entirely — there is never half
-- an operation to skip.
--
-- `:window` is 2×page + slack when unfiltered, which is one round trip. With
-- `filter[event]` the matching rate is unknown before the legs are paired, so
-- the caller doubles the window and re-reads until the page fills — geometric,
-- so O(log) round trips.
SELECT
    ledger_sequence   AS ls,
    transaction_id    AS tid,
    application_order AS ao,
    asset_id          AS asset_id,
    amount            AS amount
FROM lp_operation_amounts
WHERE pool_id = toFixedString(unhex($1), 32)
  AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  -- AND (ledger_sequence, transaction_id, application_order) < ($ls, $tid, $ao)
ORDER BY ls DESC, tid DESC, ao DESC
LIMIT 44;                          -- 2 × (page 20 + 1) + 2

-- STEP 2 — transaction-level enrichment for the page's DISTINCT tx keys.
-- Smaller than the page: several operations of one transaction share it.
-- Keys inlined (i64) with the partition prune that turns the IN filter into a
-- tight PK seek, same shape as `common::ch::fetch_tx_list_aggregates`.
SELECT
    t.id                                AS id,
    lower(hex(t.hash))                  AS hash,
    t.source_id                         AS source_id,
    toUnixTimestamp64Milli(l.closed_at) AS created_at_ms
FROM transactions t
INNER JOIN ledgers l ON l.sequence = t.ledger_sequence
WHERE (t.ledger_sequence, t.id) IN ((58123456, 991), (58123455, 990))
  AND intDiv(t.ledger_sequence, 500000) IN (116)
LIMIT 1 BY t.id;

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
-- `(ledger_sequence, transaction_id, application_order)` IS this table's sort
-- key, so the bounded IN-list is a PK seek with the same partition prune.
-- `max()` rather than `LIMIT 1 BY`: the table holds one row per APPEARANCE, so
-- an operation has several, and aggregation skips the NULLs instead of picking
-- a row arbitrarily.
SELECT
    ledger_sequence   AS ls,
    transaction_id    AS tid,
    application_order AS ao,
    max(source_id)    AS source_id
FROM operations_appearances
WHERE (ledger_sequence, transaction_id) IN ((58123456, 991), (58123455, 990))
  AND intDiv(ledger_sequence, 500000) IN (116)
GROUP BY ls, tid, ao;

-- STEP 4 — source StrKeys by surrogate id (bloom seek), NOT a whole-`accounts`
-- INNER JOIN (task 0354). `common::ch::resolve_accounts`, one call for both
-- the operation sources above and the transaction sources from STEP 2.
