-- Endpoint:     GET /v1/liquidity-pools/{pool_id}/activity
-- Purpose:      The pool's activity as a list of OPERATIONS, with an
--               `filter[event]` over trade / deposit / withdrawal. Supersedes
--               20 (`/transactions`), whose row was a transaction.
-- Source:       backend-overview.md §6.2; issue #371; task 0491
-- Schema:       lp_operation_amounts, transactions, ledgers, accounts; ADR 0044
-- Data sources: DB-only
-- Inputs:
--   $1 :pool_id           TEXT   64-char lowercase hex (from the L… strkey)
--   {a} {b}               INT64  the pool's two asset_id surrogates
--   $ls $tid $ao          INT    keyset bound (absent on the first page)
--   :event                TEXT   optional 'trade' | 'deposit' | 'withdrawal'
-- Indexes:      lp_operation_amounts PK prefix (pool_id, ledger_sequence,
--               transaction_id, application_order); transactions PK
--               (ledger_sequence, id) + partition prune; accounts bloom on id
-- CH Engine:    ReplacingMergeTree (lp_operation_amounts, transactions)
-- CH Pattern:   PK-prefix seek → GROUP BY pivot + multiIf classification →
--               bounded IN-list enrichment → surrogate StrKey resolve
-- ADR 0044 §:   §4.1 (RMT), §5.2 (ledgers holds the timestamp)
-- Notes:
--   • NO FINAL — see the ⚠️ below; the producer is deterministic, so the
--     duplicate a merge has not yet collapsed is byte-identical.
--   • The event has NO type column: it is the sign pair of the two legs.
--   • An operation with no amount rows is NOT listed (failed LP ops) — the
--     ⚠️ below states why that is deliberate.
-- ============================================================================
--
-- ONE ROW PER OPERATION against the pool. Supersedes
-- `20_get_liquidity_pools_transactions.sql`, whose row was a transaction —
-- a unit that could not carry an honest Event chip (a bundled deposit + trade
-- collapses to one label), forced the Amount cell to stack figures that must
-- not be summed, and made a trades filter inexpressible: "trades only" has no
-- truthful answer for a transaction that both deposits and trades.
--
-- ⚠️  THE DRIVER TABLE IS THE WHOLE DESIGN.
--     `operation_pools` is keyed (pool_id, ledger_sequence, transaction_id)
--     with NO application_order, so it cannot page per operation.
--     `lp_operation_amounts` is keyed
--     (pool_id, ledger_sequence, transaction_id, application_order, asset_id)
--     — exactly this page's grain, reached by one PK-prefix seek.
--
-- ⚠️  KNOWN CONSEQUENCE: an operation with NO amount rows is not listed.
--     The indexer writes `operation_pools` for an op that DECLARES a pool
--     whether or not the transaction succeeded, but writes amounts only for
--     value that actually moved (claim atoms for trades, the op's own
--     LedgerEntryChanges for deposits/withdrawals). A FAILED explicit LP op
--     therefore appeared under `/transactions` and does not appear here.
--     Deliberate: the page answers "what moved through this pool", a failed
--     op moved nothing, and this narrows a known CH-vs-Horizon breadth
--     difference rather than widening it. Reversing it means carrying
--     `application_order` into `operation_pools` — schema change plus a
--     re-key backfill, for rows that would then render with empty amounts.
--
-- ⚠️  NO `FINAL`. `lp_operation_amounts` is a ReplacingMergeTree whose
--     producer is deterministic (see the schema header's single-writer
--     argument), so an unmerged duplicate is byte-identical to its twin and
--     `anyIf` picks the same value either way; GROUP BY collapses them free.
--
-- The event is named by the SIGN PAIR and nothing else — `amount` is signed
-- from the pool's perspective, so `+/+` is a deposit, `-/-` a withdrawal and
-- `+/-` a trade. There is no operation-type column to read and no join to
-- `operations`. Classified HERE rather than in Rust because this same
-- expression is the `filter[event]` predicate: two classifiers would
-- eventually disagree, and the label the user sees must be the one the filter
-- admitted the row on.
--
-- `countIf` is not decoration. `anyIf` over a non-nullable Int64 yields 0 for
-- "no such row", indistinguishable from a real zero amount, which would
-- classify a half-row as a trade. The counts are what make a missing leg
-- detectable instead of silently wrong; such a row travels as
-- `event: null` + `amount_*: null` and matches NO filter value.
--
-- Shipped module: `crates/api/src/liquidity_pools/queries.rs::fetch_pool_activity`
-- ============================================================================

-- STEP 1 — page driver + value pivot + classification, one seek.
-- $1 = pool_id (64-char lowercase hex), {a}/{b} = the pool's asset_id
-- surrogates from `fetch_pool_asset_ids`, which also serves as the
-- pool-exists gate.
--
-- The keyset lives in WHERE, not in a post-GROUP BY HAVING: it is the sort-key
-- prefix, so it prunes the seek. Bounded after grouping would read the pool's
-- whole history first.
SELECT
    ledger_sequence   AS ls,
    transaction_id    AS tid,
    application_order AS ao,
    countIf(asset_id = {a}) AS n_a,
    countIf(asset_id = {b}) AS n_b,
    anyIf(amount, asset_id = {a}) AS raw_a,
    anyIf(amount, asset_id = {b}) AS raw_b,
    multiIf(n_a = 0 OR n_b = 0, '',
            raw_a > 0 AND raw_b > 0, 'deposit',
            raw_a < 0 AND raw_b < 0, 'withdrawal',
            'trade') AS event
FROM lp_operation_amounts
WHERE pool_id = toFixedString(unhex($1), 32)
  AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  -- AND (ledger_sequence, transaction_id, application_order) < ($ls, $tid, $ao)
GROUP BY ls, tid, ao
-- HAVING event = 'trade'          -- filter[event], same expression
ORDER BY ls DESC, tid DESC, ao DESC
LIMIT 21;                          -- page limit + 1

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

-- STEP 3 — source StrKeys by surrogate id (bloom seek), NOT a whole-`accounts`
-- INNER JOIN (task 0354). `common::ch::resolve_accounts`.
