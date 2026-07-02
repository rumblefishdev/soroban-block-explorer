-- ⚠️ SUPERSEDED by task 0331 (unified balances). The account portfolio now reads
-- per-holder balances from the unified `balances` table (joined on `assets.id`),
-- NOT `account_balances_current`; it also surfaces Soroban (type-3) token balances.
-- Authoritative query: `crates/api/src/accounts/queries_ch.rs::fetch_balances`.
-- The reference SQL below is pre-0331, kept for endpoint/inputs documentation only.
-- Endpoint:     GET /accounts/:account_id
-- Purpose:      Account detail: header + current balances (native + credit).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.7
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :account_strkey  String   G-form account ID (StrKey)
-- Indexes:      accounts ORDER BY (id) — StrKey lookup walks the FINAL
--               result; selectivity on `account_id` is high so the read
--               touches one PK granule after dedup.
--               account_balances_current ORDER BY (account_id, asset_type,
--               asset_code, issuer_id) — pre-filtered by account_id.
-- CH Engine:    accounts — ReplacingMergeTree(last_seen_ledger) (FINAL).
--               account_balances_current — ReplacingMergeTree(last_updated_ledger)
--                 with allow_nullable_key=1 (FINAL).
-- CH Pattern:   FINAL on both reads. Issuer StrKey via LEFT JOIN accounts FINAL.
-- ADR 0044 §:   §4.5 (accounts, account_balances_current Replacing state).
--               No partition predicate — state tables not partitioned.
-- Notes:
--   • Three statements. The API threads `account.id` (Int64) from A into B
--     and C, and `account.last_seen_ledger` into C.
--   • Statement C derives the `deleted` flag (account_merge) — task 0324.
--     CH-only (prod serves accounts from CH); the PG fallback reports
--     `deleted = false`.
--   • CH has no `token_asset_type_name`/`asset_type_name` SQL helper —
--     project raw SMALLINT (Int16) and decode in the API layer (Rust enum,
--     same source of truth as PG).
--   • Native row: `asset_type = 0`, `asset_code = NULL`, `issuer_id = NULL`.
--     Credit row: `asset_type != 0`, both set. LEFT JOIN handles NULL issuer.
--   • Ordering on issuer StrKey: CH does not sort NULLs the same way as PG
--     by default; we use `ORDER BY abc.asset_type, abc.asset_code, ...` so
--     native (type=0) always comes first regardless of NULL semantics.

-- ============================================================================
-- A. Account header.
-- ============================================================================
SELECT
    a.id,
    a.account_id,
    a.first_seen_ledger,
    a.last_seen_ledger,
    a.sequence_number,
    a.home_domain
FROM accounts a FINAL
WHERE a.account_id = $1;

-- @@ split @@

-- ============================================================================
-- B. Current balances.
--    Inputs: $1 = account.id (Int64, from A).
-- ============================================================================
SELECT
    abc.asset_type                          AS asset_type,
    abc.asset_code,
    iss.account_id                          AS asset_issuer,
    abc.balance,
    abc.last_updated_ledger
FROM account_balances_current abc FINAL
LEFT JOIN accounts iss FINAL ON iss.id = abc.issuer_id
WHERE abc.account_id = $1
ORDER BY abc.asset_type, abc.asset_code, iss.account_id;

-- @@ split @@

-- ============================================================================
-- C. Derived `deleted` flag (account_merge) — task 0324.
--    Inputs: $1 = account.id (Int64, from A); $2 = account.last_seen_ledger.
--
--    `deleted` ⟺ the account's LAST op in its last-seen ledger is an
--    account_merge (type = 8) where it was the `source`. Since last_seen_ledger
--    = GREATEST(all appearances), any deleting merge sits in that ledger;
--    argMax over (transaction_id, application_order) picks the account's
--    chronologically-last op within it, so a same-ledger re-create (merge then
--    create_account at a higher application order) correctly yields `false`.
--
--    `ledger_sequence = $2` as a LITERAL is load-bearing: operations_appearances
--    is PARTITION BY intDiv(ledger_sequence, 500000) with no sort key on
--    source_id/type, so anchoring on the (already-known) last_seen_ledger
--    prunes to a single granule (~8K rows). Without it the planner scans the
--    whole ~6.2B-row table and trips the query memory limit — hence a
--    dedicated 3rd query keyed by the literal, never a join on `accounts`.
-- ============================================================================
SELECT argMax(type = 8 AND source_id = $1, (transaction_id, application_order))
FROM operations_appearances
WHERE ledger_sequence = $2
  AND (source_id = $1 OR destination_id = $1);
