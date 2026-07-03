-- ⚠️ SUPERSEDED by task 0331 (unified balances). Supply/holders come from
-- `balance_aggregates` (MV over `balances`, keyed by `assets.id`), NOT
-- `asset_aggregates`; post-ADR-0051 a SAC is a facet folded into its classic row.
-- Authoritative query: `crates/api/src/assets/queries_ch.rs`. Reference SQL below is
-- pre-0331.
-- Endpoint:     GET /assets/:id
-- Purpose:      Asset detail. DB returns the composed header (code, type,
--               supply, holder_count, icon, name, symbol, decimals) plus the
--               issuer's on-chain home_domain used as the SEP-1 lookup key. The
--               API then runs a runtime SEP-1 fetch against the issuer's
--               stellar.toml to overlay description + home_page.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.9
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment.
-- Data sources: DB + runtime SEP-1 HTTP fetch (per request).
-- Inputs:
--   $1  :asset_type    Int16   asset_type domain (0=native, 1=classic_credit, 2=sac, 3=soroban-native)
--   $2  :asset_code    String  '' for native; non-empty for credit/SAC/soroban
--   $3  :issuer_id     Int64   0 for native / soroban-native; cityhash64(strkey) otherwise
--   $4  :contract_id   Int64   0 for native / classic credit; cityhash64(strkey) otherwise
-- Indexes:      assets ORDER BY (asset_type, asset_code, issuer_id, contract_id)
--                 — natural composite key (PR #175). Direct seek.
--               accounts, soroban_contracts ORDER BY natural StrKey.
--               soroban_contract_metadata ORDER BY (contract_id) — metadata join.
--               asset_enrichment / asset_aggregates — side-table joins (below).
-- CH Engine:    assets — Replacing — FINAL required.
--               soroban_contracts — Replacing, joined WITHOUT FINAL
--                 (join miss neutralised by `nullIf`).
--               accounts — Replacing, NO LONGER joined (task 0334). The issuer
--                 StrKey + home_domain are resolved by a separate single
--                 `accounts.id` bloom-pruned key-seek (idx_acc_id), latest
--                 version via ORDER BY last_seen_ledger DESC LIMIT 1.
--               soroban_contract_metadata — Replacing(version), FINAL in the
--                 sub-select (latest row per contract_id).
--               asset_enrichment — Replacing(version), collapsed via argMax in
--                 a GROUP BY sub-select.
--               asset_aggregates — MergeTree batch table (no FINAL).
-- CH Pattern:   Two-step seek (task 0334), mirroring the list `08` (task 0319)
--               and the tx-list `02` (task 0290). Step 1 reads the asset row from
--               the accounts-join-free SELECT; step 2 resolves the issuer by an
--               `accounts.id` key-seek. The full `accounts` join (~18.5M-row hash
--               side) drove the detail to ~21M read_rows / ~1.58 GB per request
--               (prod) — removing it cuts that by orders of magnitude (the issuer
--               seek touches ~1-2 granules). The API resolves the public `:id`
--               TOKEN to the WHERE predicate at the request boundary — NOT a
--               surrogate:
--                 • contract StrKey (`C…`) → seek `soroban_contracts.contract_id`
--                 • `CODE-ISSUER`          → seek `accounts.account_id` → issuer
--                                            surrogate id, then `asset_code` +
--                                            `issuer_id` on `assets`
--                 • `native`               → `asset_type = 0`
--               (task 0243/0334, `assets/queries_ch.rs` + `canonical_id` in
--               `assets/handlers.rs`). The displayed `AssetItem.id` echoes the
--               same token, so the FE never reconstructs the tuple.
-- ADR 0044 §:   §4.5 (Replacing state). **PR #175 amendment:** the surrogate
--               `assets.id` no longer exists; routing is the composite token.
-- Notes:
--   • Do NOT manufacture a cityHash64 surrogate as a routing key — `/assets/:id`
--     rejects it (400). Search hits carry the canonical token in `route_token`
--     (see `22_get_search.sql`), display `identifier` stays the asset code.
--   • Sentinels: `issuer_id=0` and `contract_id=0` represent absence;
--     LEFT JOIN never matches because `accounts.id` / `soroban_contracts.id`
--     are derived from `cityhash64(strkey)` (0 reserved).
--   • SEP-1 fetch still happens at API layer; not in SQL.
--   • **name / symbol / decimals are read-composed from side tables, NOT from
--     the `assets` row.** `assets.name` has had no writer since task 0297.
--     Name precedence: `asset_enrichment.name` (classic/SAC, task 0231) →
--     `soroban_contract_metadata.name` (on-chain SEP-41 `METADATA`, task 0297)
--     → `'Stellar Lumens'` for native. `symbol`/`decimals` come from
--     `soroban_contract_metadata` (decimals defaults to 7 for classic/SAC).
--     The detail now uses the SAME accounts-join-free SELECT as the list `08`
--     (task 0334 collapsed them); the issuer is resolved by a key-seek (step 2).

-- STEP 1 — asset row, accounts-join-free. Shown with the contract-StrKey
-- predicate (primary Soroban path). The API swaps the WHERE for the
-- CODE-ISSUER / native forms above; SELECT is identical.
SELECT
    a.asset_type                        AS asset_type,
    nullIf(a.asset_code, '')            AS asset_code,
    nullIf(sc.contract_id, '')          AS contract_id,
    coalesce(nullIf(ae.name, ''), nullIf(m.name, ''),
             if(a.asset_type = 0, 'Stellar Lumens', NULL)) AS name,
    nullIf(m.symbol, '')                AS symbol,
    coalesce(m.decimals, 7)             AS decimals,
    toString(agg.total_supply)          AS total_supply,
    agg.holder_count                    AS holder_count,
    nullIf(sc.deployed_at_ledger, 0)    AS deployed_at_ledger,
    nullIf(ae.icon_url, '')             AS icon_url,
    a.issuer_id                         AS issuer_id_key,  -- → step 2 seek
    a.contract_id                       AS contract_id_key
    -- not in DB: description, home_page — runtime SEP-1 fetch via
    --   `runtime_enrichment::sep1` (task 0188), keyed off issuer_home_domain.
FROM assets a FINAL
LEFT JOIN soroban_contracts sc  ON sc.id  = a.contract_id
LEFT JOIN (
    SELECT contract_id, name, symbol, decimals
    FROM soroban_contract_metadata FINAL      -- task 0297 side table; RMT(version) → latest per contract
) m ON m.contract_id = sc.contract_id
LEFT JOIN asset_aggregates agg
       ON agg.asset_code = a.asset_code AND agg.issuer_id = a.issuer_id
LEFT JOIN (
    SELECT asset_type, asset_code, issuer_id, contract_id,
           argMax(icon_url, version) AS icon_url,
           argMax(name, version)     AS name
    FROM asset_enrichment                     -- task 0231 side table
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) ae ON ae.asset_type = a.asset_type AND ae.asset_code = a.asset_code
    AND ae.issuer_id = a.issuer_id   AND ae.contract_id = a.contract_id
WHERE sc.contract_id = $1
LIMIT 1;

-- STEP 2 — resolve the issuer surrogate (issuer_id_key from step 1) to its
-- StrKey + home_domain via the idx_acc_id bloom-pruned key-seek (task 0334).
-- Skipped when issuer_id_key = 0 (native / no issuer). For the CODE-ISSUER form
-- this seek runs FIRST (by accounts.account_id) to resolve the issuer surrogate,
-- and the same row supplies the StrKey + home_domain.
SELECT id, account_id, home_domain
FROM accounts
WHERE id = $issuer_id_key            -- CODE-ISSUER form: WHERE account_id = $strkey
ORDER BY last_seen_ledger DESC
LIMIT 1;
