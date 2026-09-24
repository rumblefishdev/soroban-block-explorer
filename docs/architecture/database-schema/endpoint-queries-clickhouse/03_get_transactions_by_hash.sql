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
--   $1  :hash     FixedString(32)  raw 32-byte transaction hash
-- Indexes:      transaction_hash_prefix_index (ORDER BY (hash_prefix,
--                 ledger_sequence) — the hash's first 8 bytes → candidate
--                 ledgers, task 0580).
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 + PARTITION BY intDiv. Once we have ledger_sequence from
--                 the index, the planner uses one partition + sparse-PK granule.
--               operations_appearances, transaction_participants, soroban_events,
--                 soroban_invocations_appearances — all PARTITION BY intDiv
--                 + ORDER BY with transaction_id in the prefix.
--               accounts, soroban_contracts — Replacing state, FINAL.
-- CH Engine:    All Replacing — FINAL on every read. ledgers MergeTree only.
-- CH Pattern:   6 statements like PG. A seeks the hash index; subsequent
--                 statements use the resolved `(ledger_sequence, transaction_id)`
--                 for partition prune + sparse-PK granule seek.
-- ADR 0044 §:   §4.9, §4.2/§4.3 (Replacing
--                 partitioned), §4.4 (soroban_events full payload — §5.1
--                 divergence: E reads full payload not just appearance index),
--                 §4.5 (state Replacing), §5.2 (closed_at via JOIN ledgers
--                 if needed; the header carries it via JOIN to ledgers).
-- Notes:
--   • Six statements. The API runs them sequentially, threading
--     `(transaction_id, ledger_sequence)` from statement B into C-F.
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
    t.id                                    AS transaction_id,
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
WHERE t.hash = $1
  AND intDiv(t.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)));

-- @@ split @@

-- ============================================================================
-- C. Operations (appearance rows for this tx).
--    Inputs: $1 = hash (used for partition pruning via the index lookup).
--    PR #175 dropped `operations_appearances.id` surrogate; rows are
--    identified by (transaction_id, application_order). Frontend gets
--    a stable per-op id by combining the natural key.
-- ============================================================================
SELECT
    oa.application_order                    AS appearance_id, -- natural-key replacement for dropped surrogate
    oa.type                                 AS type,
    src.account_id                          AS source_account,
    dst.account_id                          AS destination_account,
    sc.contract_id                          AS contract_id,
    oa.asset_code,
    iss.account_id                          AS asset_issuer,
    arrayMap(x -> lower(hex(x)), oa.pool_ids) AS pool_ids_hex,
    oa.application_order,
    oa.ledger_sequence,
    l.closed_at                             AS created_at
    -- not in DB: per-op stroop amount, raw operation parameters, return values
    --           — Archive XDR overlay. ADR 0029.
FROM operations_appearances oa FINAL
LEFT JOIN accounts          src FINAL ON src.id = oa.source_id          AND oa.source_id         IS NOT NULL
LEFT JOIN accounts          dst FINAL ON dst.id = oa.destination_id     AND oa.destination_id    IS NOT NULL
LEFT JOIN soroban_contracts sc  FINAL ON sc.id  = oa.contract_id        AND oa.contract_id       IS NOT NULL
LEFT JOIN accounts          iss FINAL ON iss.id = oa.asset_issuer_id    AND oa.asset_issuer_id   IS NOT NULL
JOIN      ledgers           l         ON l.sequence = oa.ledger_sequence
WHERE oa.transaction_id = (
    SELECT id FROM transactions FINAL WHERE hash = $1
      AND intDiv(ledger_sequence, 500000)
          IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
    LIMIT 1)
  AND intDiv(oa.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
-- ORDER BY natural shape: application_order is unique within (transaction_id, ledger_sequence)
-- per PR #175 schema, so this single column gives stable ordering — no oa.id tiebreaker needed.
ORDER BY oa.application_order ASC NULLS LAST;

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
    SELECT ledger_sequence, application_order FROM transactions FINAL WHERE hash = $1
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
    SELECT ledger_sequence, application_order FROM transactions FINAL WHERE hash = $1
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
SELECT
    sc.contract_id,
    caller.account_id                       AS caller_account,
    caller_contract.contract_id             AS caller_contract,
    sia.ledger_sequence,
    sia.amount,
    l.closed_at                             AS created_at
    -- not in DB: function_name, args, return_value — Archive XDR.
FROM soroban_invocations_appearances sia FINAL
JOIN soroban_contracts sc FINAL ON sc.id = sia.contract_id
LEFT JOIN accounts          caller          FINAL ON caller.id          = sia.caller_id
LEFT JOIN soroban_contracts caller_contract FINAL ON caller_contract.id = sia.caller_contract_id
JOIN      ledgers           l                     ON l.sequence = sia.ledger_sequence
WHERE sia.transaction_id = (
    SELECT id FROM transactions FINAL WHERE hash = $1
      AND intDiv(ledger_sequence, 500000)
          IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
    LIMIT 1)
  AND intDiv(sia.ledger_sequence, 500000)
      IN (SELECT intDiv(ledger_sequence, 500000) FROM transaction_hash_prefix_index
          WHERE hash_prefix = reinterpretAsUInt64(substring($1, 1, 8)))
ORDER BY sia.ledger_sequence, sc.contract_id;
