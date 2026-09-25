-- ============================================================================
-- Endpoint:     GET /contracts/:contract_id/invocations
-- Purpose:      Paginated list of a contract's invocations, newest first, in
--               execution order inside a ledger.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.10
-- Schema:       ADR 0044 (CH pilot), ADR 0059 (position), task 0586
-- Data sources: DB-only. function_name + args + return_value live in the
--               archive XDR (ADR 0029/0034).
-- Inputs:
--   $1  :contract_id          Int64    soroban_contracts.id (resolved first)
--   $2  :limit                Int      page size (the API reads limit + 1)
--   $3  :cursor_ledger        Int64    NULL on first page
--   $4  :cursor_app_order     Int16    NULL on first page — the cursor is the
--                                      transaction's position (task 0586); a
--                                      surrogate cursor answers 400
-- Indexes:      contract_activity ORDER BY (contract_id, ledger_sequence,
--                 application_order) — contract-leading seek.
--               transactions ORDER BY (ledger_sequence, application_order).
-- CH Engine:    ReplacingMergeTree. No FINAL on the driver (with it CH merges
--               the contract's rows across every part, ~38× read
--               amplification measured on the invocations table it replaced);
--               `LIMIT 1 BY` collapses an unmerged duplicate.
-- CH Pattern:   two steps — (A) the driver seek collects the page's positions
--               and callers; (B) the page's transaction headers
--               (hash / successful / closed_at) by
--               `(ledger_sequence, application_order) IN (…)`. Never a join
--               of the driver to an unpruned `transactions FINAL` (the
--               read_rows-quota trap, CH Code: 201). Caller StrKeys resolve by
--               `accounts.id` bloom seek in the API.
-- Notes:
--   • `invocation_count > 0` keeps the invoked pairs: `contract_activity`
--     also holds pairs a transaction only touched (an operation event, an
--     operation naming the contract), which count 0.
--   • The caller is split across `caller_id` (an account) and
--     `caller_contract_id` (a contract), exactly one set on an invoked row.
--     The API reads `caller_id` only (task 0487 fixes that).
--   • Before task 0586 this read `soroban_invocations_appearances`, keyed by
--     the `transaction_id` surrogate — so a ledger's invocations came in hash
--     order.

-- A. Driver. The LIMIT sits inside the subquery so the read stops at it in
-- key order; `LIMIT 1 BY` beside the LIMIT disables that (a SAC with 10 M
-- weekly invocations: 22.3 M rows read flat, 4.4 M nested, 2026-09-25 —
-- 199 freshly filled parts; the invocations table read 0.6 M over 57).
SELECT m.ledger_sequence, m.application_order, m.caller_id
FROM (
    SELECT ledger_sequence, application_order, caller_id
    FROM contract_activity
    WHERE contract_id = $1
      AND invocation_count > 0
      AND ledger_sequence <= (SELECT max(sequence) FROM ledgers)
      AND ($3 IS NULL OR (ledger_sequence, application_order) < ($3, $4))
    ORDER BY ledger_sequence DESC, application_order DESC
    LIMIT $2
) m
LIMIT 1 BY m.ledger_sequence, m.application_order;

-- @@ split @@

-- B. The page's transaction headers — the literals are an example.
SELECT
    t.ledger_sequence AS ledger_sequence,
    t.application_order AS application_order,
    lower(hex(t.hash)) AS hash,
    t.successful,
    l.closed_at AS created_at
FROM transactions t
INNER JOIN ledgers l ON l.sequence = t.ledger_sequence
WHERE (t.ledger_sequence, t.application_order) IN ((64613990, 233), (64613989, 17))
  AND intDiv(t.ledger_sequence, 500000) IN (129);
