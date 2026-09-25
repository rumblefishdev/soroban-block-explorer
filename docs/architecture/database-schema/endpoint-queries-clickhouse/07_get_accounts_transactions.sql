-- ============================================================================
-- ⚠️  `contract_ids` REMOVED from the API (task 0386) — the shared helper now
--     returns `operation_types` only; the `contract_ids` array below is dead.
-- ⚠️  CH 26.3 CORRECTION (task 0243) — do NOT implement the
--     `operation_types` array with a correlated scalar subquery
--     (`… WHERE oa.transaction_id = t.id`, the shape this file once showed):
--     ClickHouse 26.3 rejects it with `Code: 48 NOT_IMPLEMENTED`. Use the
--     NON-correlated two-step the shipped modules use — fetch the page, then
--     aggregate per `(ledger_sequence, application_order) IN (…)` grouped by
--     that position (step 3 below; keyed by `transaction_id` until task 0372).
--     Reuse the shared Rust helper
--     `crates/api/src/common/ch.rs::fetch_tx_list_aggregates`.
-- ============================================================================
-- ⚠️  CH READ-COST NOTE (task 0243) — the live read path is the step shape
--     below, not a single JOIN. An account's transactions span MANY ledger
--     partitions (it is active over time), so the global `/transactions`
--     single-partition prune does NOT apply here:
--       1. Driver: `transaction_participants` WHERE account_id = <surrogate>
--          — `account_id` is the LEADING primary key (ORDER BY (account_id,
--          ledger_sequence, application_order)), so this is an account-scoped
--          SEEK, not a scan. ORDER BY (ledger_sequence, application_order) +
--          keyset, LIMIT — execution order inside a ledger (task 0575).
--       2. Fetch the ≤limit transaction rows by `(ledger_sequence,
--          application_order) IN (keys)` — the full `transactions` key, which
--          spans partitions safely. Do NOT join the driver to an unpruned
--          `transactions FINAL` (that merges the whole 3.6B-row table and blew
--          the api_reader read_rows quota in the global list — CH Code: 201).
--       3. operation_types via the shared two-step aggregate over
--          `transaction_operations`, keyed by the page positions from step 1
--          (task 0372; it was keyed by the page rows' `id`); re-order rows in
--          Rust by the driver keyset order.
--       4. `balance_changes` (task 0540) — this account's signed per-asset
--          movement per transaction, from `asset_transfers`, keyed on
--          `(ledger_sequence, application_order)` which step 2 already has and
--          which is that table's sort-key PREFIX, so it is a seek. Two
--          statements: the signed sum, then the asset identity for the ids it
--          returned. Measured on production, 25-transaction page: 20 ms /
--          13 824 rows and 44 ms / 268 k rows. Rules that are NOT optional —
--          the inner GROUP BY over the FULL sort key (version-less RMT with
--          permanent duplicates), the `intDiv(ledger_sequence, 500000)`
--          partition prune, `to - from` per row so a self-transfer cancels,
--          and `CAST(… AS Array(Int64))` on any inlined id list (ClickHouse
--          types an array literal from its values, so an all-positive list
--          becomes `Array(UInt64)` and fails to decode). Transactions BELOW
--          the index floor get `null`, never an empty list: the table has no
--          rows there and an absent measurement must not render as a zero.
--          See `crates/api/src/accounts/balance_changes.rs`.
--     Cursor keys on `(ledger_sequence, application_order)` — the
--     transaction's position (task 0575); a surrogate cursor minted before
--     answers 400 `invalid_cursor`. See `transactions::dto::TxListCursor`.
-- ============================================================================
-- Endpoint:     GET /accounts/:account_id/transactions
-- Purpose:      Paginated transactions involving a given account (as source
--               OR as a participant). Default ordering: newest first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.7
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :account_strkey      String   G-form account ID (StrKey)
--   $2  :limit               Int      page size
--   $3  :cursor_ledger       Int64    NULL on first page
--   $4  :cursor_app_order    Int16    NULL on first page
-- Indexes:      accounts ORDER BY (id) — leading CTE resolves StrKey → id.
--               transaction_participants ORDER BY (account_id, ledger_sequence,
--                 application_order) — natural keyset for this scan.
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 + PARTITION BY intDiv(ledger_sequence, 500000).
--               transaction_operations ORDER BY (ledger_sequence,
--                 application_order, operation_index) — the operation_types
--                 aggregate is a primary-key seek on the page positions
--                 (task 0372).
-- CH Engine:    accounts, transactions, transaction_participants,
--               transaction_operations — all ReplacingMergeTree. No FINAL on
--               the page path: the driver collapses unmerged rows with
--               `LIMIT 1 BY`, the page rows are merged by position in Rust,
--               the aggregate folds duplicates with `groupUniqArray`.
-- CH Pattern:   driver seek on transaction_participants → page seek on
--               transactions by position with the partition prune →
--               groupUniqArray for operation_types[] by position.
-- ADR 0044 §:   §4.2/§4.3 (Replacing partitioned), §4.5 (state Replacing),
--               §5.2 (no `created_at` — partition predicate via intDiv).
-- Notes:
--   • PG's CTE `acc` resolves the StrKey via accounts.account_id UNIQUE.
--     CH has no equivalent UNIQUE constraint; we resolve via a subquery on
--     `accounts FINAL` instead. Selectivity is high; result is one Int64
--     after FINAL.
--   • Cursor tuple drops `created_at` (§5.2) — natural keyset for CH
--     transaction_participants is `(account_id, ledger_sequence,
--     application_order)`. We page on `(ledger_sequence, application_order) <
--     ($3, $4)` since account_id is pinned in WHERE.
--   • `operation_types[]` is a separate non-correlated aggregate over the
--     page positions (step 3) — CH 26.3 rejects the correlated form. The
--     Int16 array is decoded to op_type_name strings in the API layer.
--   • Source StrKey by surrogate-id key seek (`resolve_accounts`, task 0345),
--     not a JOIN to accounts.
--   • The page keys are integers, inlined as literal `IN (…)` lists with the
--     touched partitions — the literals below are examples.

-- Step 1 — account-scoped driver seek (no FINAL; `LIMIT 1 BY` collapses a
-- re-ingest duplicate). The API binds the account's surrogate from its lookup;
-- the subquery stands in for it here.
SELECT tp.ledger_sequence AS ledger_sequence, tp.application_order AS application_order
FROM transaction_participants tp
WHERE tp.account_id = (SELECT id FROM accounts FINAL WHERE account_id = $1 LIMIT 1)
  AND tp.ledger_sequence <= (SELECT max(sequence) FROM ledgers)
  AND ($3 IS NULL OR (tp.ledger_sequence, tp.application_order) < ($3, $4))
-- Sort direction driven by `order` query param (`asc` | `desc`, default
-- `desc`). `order=asc` flips the `<` above to `>` and DESC→ASC in lock-step.
ORDER BY tp.ledger_sequence DESC, tp.application_order DESC
LIMIT 1 BY tp.ledger_sequence, tp.application_order
LIMIT $2;

-- @@ split @@

-- Step 2 — the page rows at the positions from step 1. `t.id` is selected
-- for the balance-change step (task 0540) only.
SELECT
    t.id                  AS id,
    lower(hex(t.hash))    AS hash,
    t.ledger_sequence,
    t.application_order,
    t.source_id           AS source_id,
    t.fee_charged,
    t.successful,
    t.operation_count,
    t.has_soroban,
    l.closed_at           AS created_at
FROM transactions t
INNER JOIN ledgers l ON l.sequence = t.ledger_sequence
WHERE (t.ledger_sequence, t.application_order) IN ((64000123, 12), (63990001, 3))
  AND intDiv(t.ledger_sequence, 500000) IN (127, 128);

-- @@ split @@

-- Step 3 — `operation_types[]` for the page positions (shared helper, see 02
-- statement D). No FINAL: `groupUniqArray` collapses an unmerged duplicate.
SELECT ledger_sequence, application_order, groupUniqArray(type) AS codes
FROM transaction_operations
WHERE (ledger_sequence, application_order) IN ((64000123, 12), (63990001, 3))
  AND intDiv(ledger_sequence, 500000) IN (127, 128)
GROUP BY ledger_sequence, application_order;
