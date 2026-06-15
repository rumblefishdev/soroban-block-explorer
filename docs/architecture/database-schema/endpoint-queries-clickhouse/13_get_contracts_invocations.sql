-- ============================================================================
-- ⚠️  CH READ-COST NOTE (task 0243) — the live read path is a two-step shape:
--       1. Driver: soroban_invocations_appearances WHERE contract_id = <surrogate>
--          — contract_id is the LEADING primary key (ORDER BY (contract_id,
--          ledger_sequence, transaction_id)), so this is a contract-scoped SEEK.
--          ORDER BY (ledger_sequence, transaction_id) + keyset, LIMIT.
--       2. Fetch the page's transaction headers (hash / successful / closed_at)
--          by (ledger_sequence, id) IN (keys). Do NOT join the driver to an
--          unpruned `transactions FINAL` — that merges the whole transactions
--          table and blew the read_rows quota in the global list (CH Code: 201).
--     A contract's invocations span many partitions; the IN-keys fetch is a
--     primary-key-prefix prune per ledger (multi-partition-safe). Cursor keys on
--     (ledger_sequence, transaction_id) (CH) / (created_at, transaction_id) (PG)
--     — see transactions::dto::TxListCursor. created_at derives from the joined
--     ledger closed_at.
-- ============================================================================
-- Endpoint:     GET /contracts/:contract_id/invocations
-- Purpose:      Paginated list of recent invocations of a contract.
--               Default ordering: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.10
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only. function_name + args + return_value still live in
--               the archive XDR (ADR 0029/0034) on both PG and CH sides;
--               same out-of-scope semantics as PG E13.
-- Inputs:
--   $1  :contract_strkey       String   C-form contract ID
--   $2  :limit                 Int      page size
--   $3  :cursor_ledger         Int64    NULL on first page
--   $4  :cursor_tx_id          Int64    NULL on first page
-- Indexes:      soroban_contracts ORDER BY (id) — leading resolve.
--               soroban_invocations_appearances ORDER BY (contract_id,
--                 ledger_sequence, transaction_id) — contract-leading scan.
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 + intDiv partition.
--               accounts ORDER BY (id) — caller_account LEFT JOIN.
-- CH Engine:    All Replacing — FINAL.
-- CH Pattern:   leading $1 resolve via FINAL'd subquery; partition prune on
--               transactions via intDiv on cursor's ledger; FINAL on every
--               Replacing read.
-- ADR 0044 §:   §4.3 (soroban_invocations_appearances Replacing partitioned),
--               §4.2 (transactions), §4.5 (state Replacing), §5.2 (no
--               closed_at — cursor drops created_at term).
-- Notes:
--   • Same shape as PG E13. Caller split across `caller_id` (G/M account)
--     and `caller_contract_id` (C contract), per task 0183 / ADR 0034.
--   • Cursor drops PG's `created_at` term — natural keyset is
--     `(ledger_sequence, transaction_id)`. Equivalent semantics because
--     `(ledger_sequence, transaction_id)` is fully ordered for a single
--     contract's invocations.
--   • Partition prune is best-effort: the cursor's ledger gives us a hint
--     but the page might span partition boundaries on first page. CH
--     accepts the predicate; planner uses the partitioning if useful.

SELECT
    lower(hex(t.hash))                              AS transaction_hash_hex,
    sia.ledger_sequence,
    caller.account_id                               AS caller_account,
    caller_contract.contract_id                     AS caller_contract,
    sia.amount,
    t.successful,
    t.id                                            AS cursor_tx_id
    -- not in DB: function_name, args, return_value — Archive XDR (ADR 0029/0034).
FROM soroban_invocations_appearances sia FINAL
JOIN transactions t FINAL ON t.id = sia.transaction_id AND t.ledger_sequence = sia.ledger_sequence
LEFT JOIN accounts          caller          FINAL ON caller.id          = sia.caller_id
LEFT JOIN soroban_contracts caller_contract FINAL ON caller_contract.id = sia.caller_contract_id
WHERE
    sia.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $1 LIMIT 1)
    AND ($3 IS NULL OR (sia.ledger_sequence, sia.transaction_id) < ($3, $4))
ORDER BY sia.ledger_sequence DESC, sia.transaction_id DESC
LIMIT $2;
