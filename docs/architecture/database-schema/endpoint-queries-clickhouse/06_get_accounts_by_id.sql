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
--     and C. C no longer needs `last_seen_ledger`.
--   • Statement C READS the `deleted` flag off the native holding's lifecycle
--     column (task 0500) — it no longer derives it from operation history.
--     CH-only (prod serves accounts from CH).
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
-- C. `deleted` flag — READ, not derived (task 0500, ADR 0055).
--    Inputs: $1 = account.id (Int64, from A); $2 = the native asset surrogate,
--            BOUND from Rust (a cityhash ClickHouse cannot recompute).
--
--    Native XLM lives on the `AccountEntry`, so "the account was removed" and
--    "its native holding was closed" are one fact, recorded once in the
--    lifecycle column. No row at all ⇒ false: an address we have only ever
--    seen referenced is not "deleted".
--
--    This REPLACED an operations_appearances × transactions derivation that
--    asked whether the account sourced a successful account_merge in its
--    last-seen ledger. It under-detected badly — 22 of 60 sampled merged
--    accounts — for two independent reasons:
--      • participation bumps `last_seen_ledger` past the death (task 0500:
--        16,187 candidates in a single 200k-ledger window), so the anchor
--        ledger held no merge at all;
--      • the merge operation is not attributed to the account being merged —
--        one sampled ledger carried exactly one type-8 appearance and none of
--        its 664 appearances named the merged account.
--    A fact cannot be derived correctly from a table that does not carry it.
--
--    Chain-verified via getLedgerEntries, 236 accounts, no exceptions: closed
--    row -> 100/100 ABSENT, open row -> 100/100 PRESENT, merged-then-recreated
--    -> 36/36 PRESENT and correctly alive. Re-measuring task 0500's window
--    after the fix: 16,139 of 16,187 now read deleted and the remaining 48
--    probed PRESENT on chain — the defect population is zero.
--
--    The re-create case needs no special handling: a re-create writes a new
--    open row over the tombstone and FINAL keeps one row per key (measured:
--    zero accounts hold both an open and a closed native row). That also
--    retires the same-ledger merge-then-create caveat the old query carried.
-- ============================================================================
SELECT closed_at_ledger != 0
FROM balances FINAL
WHERE holder_id = $1 AND asset_id = $2;
