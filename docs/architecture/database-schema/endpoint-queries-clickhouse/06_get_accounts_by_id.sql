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
--   • Two statements, same shape as PG E06. The API threads `account.id`
--     (Int64) from A into B.
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
