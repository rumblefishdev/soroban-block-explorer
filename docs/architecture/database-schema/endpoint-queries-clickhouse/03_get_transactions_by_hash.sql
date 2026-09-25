-- Endpoint:     GET /transactions/:hash
-- Purpose:      Full transaction detail. Header from DB; raw envelope/result/
--               result_meta XDR + parsed invocation tree fetched from the
--               public Stellar ledger archive at request time per ADR 0029
--               (NOT in DB on either PG or CH side).
--               Operation list, participants, soroban events, and invocation
--               appearances are all DB-side.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.4
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037 + ADR 0029/0033/0034
-- Data sources: DB + Archive XDR (ADR 0029).
--               DB returns: header, operations[], participants[],
--                           soroban_events[] (**full payload inline per
--                           §5.1, no Archive overlay needed for events**),
--                           soroban_invocations[] (appearance rows).
--               Archive returns: envelope_xdr, result_xdr, result_meta_xdr,
--                           parsed operation_tree, full invocation args+returns.
--                           NOTE: PG E03 also fetches events from Archive;
--                           CH side skips that since `soroban_events` is
--                           the full-content table (§5.1, §5.5 win).
-- Inputs:
--   $1  :hash     FixedString(32)  raw 32-byte transaction hash — the
--                                  transaction's own, or a fee-bump's inner
--                                  hash (`hash OR inner_tx_hash`)
-- Indexes:      transaction_hash_prefix_index (ORDER BY (hash_prefix,
--                 ledger_sequence) — the hash's first 8 bytes → candidate
--                 ledgers, task 0580).
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 + PARTITION BY intDiv. Once we have ledger_sequence from
--                 the index, the planner uses one partition + sparse-PK granule.
--               transaction_operations ORDER BY (ledger_sequence,
--                 application_order, operation_index) — statement C is a
--                 primary-key seek on the transaction's position (task 0372).
--               transaction_participants, soroban_events — filtered by the
--                 position too; contract_activity (F) — by the position,
--                 behind a leading contract_id (task 0586). All PARTITION BY
--                 intDiv(ledger_sequence, 500000).
--               accounts, soroban_contracts — Replacing state, FINAL.
-- CH Engine:    All Replacing — FINAL on every read. ledgers MergeTree only.
-- CH Pattern:   6 statements like PG. A seeks the hash index; subsequent
--                 statements use the resolved position `(ledger_sequence,
--                 application_order)` (C–F) for partition prune + sparse-PK
--                 seek.
-- ADR 0044 §:   §4.9, §4.2/§4.3 (Replacing
--                 partitioned), §4.4 (soroban_events full payload — §5.1
--                 divergence: E reads full payload not just appearance index),
--                 §4.5 (state Replacing), §5.2 (closed_at via JOIN ledgers
--                 if needed; the header carries it via JOIN to ledgers).
-- Notes:
--   • Six statements. The API runs them sequentially, threading the
--     position `(ledger_sequence, application_order)` from statement B into
--     C–F.
--   • Statement A is the partition-pruning shortcut: hash → ledger_sequence
--     via a `transaction_hash_prefix_index` seek on the hash's first 8 bytes —
--     every candidate ledger; `transactions` decides by the full hash (more
--     than one candidate only when two hashes share a prefix). (A `transaction_hash_dict`
--     Dictionary over it was never called by the API and was removed in
--     task 0396.)
--   • Statement E (events) is the major §5.1 divergence: returns FULL
--     event payload (`topics_xdr`, `data_xdr`, `event_type`, `signature`)
--     inline. PG E03 statement E only returns the appearance index +
--     the Archive bridge — the API has to fetch + parse the .xdr.zst.
--     CH-side, the API can return events directly without Archive fetch.
--   • Statement F (invocations) still needs Archive overlay for
--     function_name / args / return_value (those are not in CH either —
--     they're in the Archive XDR per ADR 0034). The DB-side rows tell
--     the API which contracts were touched and by whom.
--   • `closed_at` for the header comes via JOIN to `ledgers` (§5.2).

-- ============================================================================
-- A. Candidate ledgers of the hash (transaction_hash_prefix_index seek).
-- ============================================================================
SELECT DISTINCT ledger_sequence FROM transaction_hash_prefix_index
WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8))
ORDER BY ledger_sequence DESC;

-- @@ split @@

-- ============================================================================
-- B. Transaction header.
--    Inputs: $1 = hash (FixedString(32)).
--    Resolves the row using the candidate ledgers for partition prune,
--    then sparse-PK seek on (ledger_sequence, application_order, id).
-- ============================================================================
SELECT
    lower(hex(t.hash))                      AS hash_hex,
    t.ledger_sequence,
    t.application_order,
    src.account_id                          AS source_account,
    t.fee_charged,
    lower(hex(t.inner_tx_hash))             AS inner_tx_hash_hex,
    t.successful,
    t.operation_count,
    t.has_soroban,
    t.parse_error,
    l.closed_at                             AS created_at
    -- not in DB: envelope_xdr, result_xdr, result_meta_xdr, operation_tree
    --           — Archive (.xdr.zst), parsed at request time. ADR 0029.
    -- not in DB: memo_type, memo_content, signatures[] — Archive overlay.
FROM transactions t FINAL
JOIN accounts src FINAL ON src.id = t.source_id
JOIN ledgers   l        ON l.sequence = t.ledger_sequence
WHERE (t.hash = $1 OR t.inner_tx_hash = $1)
  AND intDiv(t.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)));

-- @@ split @@

-- ============================================================================
-- C. Operations (appearance rows for this tx).
--    Inputs: $1 = hash (used for partition pruning via the index lookup).
--    Located by the transaction's POSITION (task 0372): the API binds
--    `(ledger_sequence, application_order)` from statement B; here the same
--    subquery as D and E derives it from the hash. `transaction_operations`
--    has no surrogate `id` (PR #175) and stores the operation's 0-based
--    `operation_index` (ADR 0059); the API sends `operation_index + 1` as both
--    `appearance_id` and `application_order`, the 1-based position the wire
--    has always carried (the `#op-N` anchor).
--    Surrogate ids (`source_id`, `destination_id`, `contract_id`,
--    `asset_issuer_id`) resolve to StrKeys by key seeks in the API
--    (`resolve_accounts` / `resolve_contracts`), not by joins.
-- ============================================================================
SELECT
    oa.type                                 AS op_type,
    oa.source_id,
    oa.destination_id,
    oa.contract_id,
    oa.asset_issuer_id,
    nullIf(oa.asset_code, '')               AS asset_code,
    arrayMap(x -> lower(hex(x)), oa.pool_ids) AS pool_ids,
    oa.operation_index,
    oa.ledger_sequence,
    l.closed_at                             AS created_at
    -- not in DB: per-op stroop amount, raw operation parameters, return values
    --           — Archive XDR overlay. ADR 0029.
FROM transaction_operations oa FINAL
-- ledgers FINAL explicit: ledgers is a ReplacingMergeTree with unmerged
-- duplicates; the join pins one sequence, so it is cheap (task 0420).
INNER JOIN ledgers l FINAL ON l.sequence = oa.ledger_sequence
WHERE (oa.ledger_sequence, oa.application_order) = (
    -- the transaction's position (task 0372)
    SELECT ledger_sequence, application_order FROM transactions FINAL WHERE (hash = $1 OR inner_tx_hash = $1)
      AND intDiv(ledger_sequence, 500000)
          IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
    LIMIT 1)
  AND intDiv(oa.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
-- `operation_index` is unique within the transaction: every operation belongs
-- to one folded identity group, and the group's row carries its smallest
-- index — so no tiebreaker is needed.
ORDER BY oa.operation_index;

-- @@ split @@

-- ============================================================================
-- D. Participants (accounts touched by this tx).
--    Inputs: $1 = hash (index lookup for partition prune).
-- ============================================================================
SELECT
    a.account_id
FROM transaction_participants tp FINAL
JOIN accounts a FINAL ON a.id = tp.account_id
WHERE (tp.ledger_sequence, tp.application_order) = (
    -- the transaction's position (task 0575)
    SELECT ledger_sequence, application_order FROM transactions FINAL WHERE (hash = $1 OR inner_tx_hash = $1)
      AND intDiv(ledger_sequence, 500000)
          IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
    LIMIT 1)
  AND intDiv(tp.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
ORDER BY a.account_id;

-- @@ split @@

-- ============================================================================
-- E. Soroban events (FULL PAYLOAD per §5.1 — major divergence vs PG).
--    Inputs: $1 = hash (index lookup for partition prune).
--    Unlike PG, CH `soroban_events` is the full table — `topics_xdr`,
--    `data_xdr`, `event_type`, `signature` are inlined. No Archive overlay
--    required for events on the CH-backed path.
-- ============================================================================
SELECT
    sc.contract_id,
    se.ledger_sequence,
    se.transaction_index,
    se.operation_index,
    se.event_index,
    se.event_type,
    se.signature,
    se.topics_xdr,
    se.data_xdr,
    l.closed_at                             AS created_at
FROM soroban_events se FINAL
JOIN soroban_contracts sc FINAL ON sc.id = se.contract_id
JOIN ledgers l ON l.sequence = se.ledger_sequence
WHERE (se.ledger_sequence, se.application_order) = (
    SELECT ledger_sequence, application_order FROM transactions FINAL WHERE (hash = $1 OR inner_tx_hash = $1)
      AND intDiv(ledger_sequence, 500000)
          IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
    LIMIT 1)
  AND intDiv(se.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
ORDER BY se.ledger_sequence, sc.contract_id, se.transaction_index, se.operation_index, se.event_index;

-- The filter is the transaction's POSITION (ADR 0059): a fee refund's rpc id
-- carries the end-of-ledger sentinel, so `application_order` is the only
-- column that says which transaction an event belongs to.

-- @@ split @@

-- ============================================================================
-- F. Soroban invocation appearances (per ADR 0034 + task 0183).
--    Inputs: $1 = hash (index lookup for partition prune).
--    function_name/args/return_value still come from Archive XDR — those are
--    not stored in CH either (ADR 0029 boundary applies to both stores).
-- ============================================================================
--    Located by the transaction's POSITION (task 0586): the API binds
--    `(ledger_sequence, application_order)` from statement B; here the same
--    subquery as D and E derives it from the hash. `contract_activity` leads
--    with `contract_id`, so this reads the ledger's granules of the partition,
--    as the old surrogate lookup did (2.1 M rows / 32 ms against 1.0 M /
--    25 ms, measured on the same key shape). The API resolves the surrogates
--    to StrKeys by key seeks, not joins, and reads `caller_id` only.
SELECT
    ca.contract_id                          AS contract_surrogate,
    ca.caller_id,
    ca.caller_contract_id,
    ca.invocation_count,
    ca.ledger_sequence,
    l.closed_at                             AS created_at
    -- not in DB: function_name, args, return_value — Archive XDR.
FROM contract_activity ca FINAL
INNER JOIN ledgers l FINAL ON l.sequence = ca.ledger_sequence
WHERE (ca.ledger_sequence, ca.application_order) = (
    SELECT ledger_sequence, application_order FROM transactions FINAL
    WHERE (hash = $1 OR inner_tx_hash = $1)
      AND intDiv(ledger_sequence, 500000)
          IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
    LIMIT 1)
  AND ca.invocation_count > 0;
