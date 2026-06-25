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
--               accounts, soroban_contracts — Replacing, joined WITHOUT FINAL
--                 (join miss neutralised by `nullIf`).
--               soroban_contract_metadata — Replacing(version), FINAL in the
--                 sub-select (latest row per contract_id).
--               asset_enrichment — Replacing(version), collapsed via argMax in
--                 a GROUP BY sub-select.
--               asset_aggregates — MergeTree batch table (no FINAL).
-- CH Pattern:   FINAL'd seek on `assets` + the metadata sub-select. The API
--               resolves the public `:id` TOKEN to the WHERE predicate at the
--               request boundary — NOT a surrogate:
--                 • contract StrKey (`C…`) → seek `soroban_contracts.contract_id`
--                 • `CODE-ISSUER`          → `asset_code` + `accounts.account_id`
--                 • `native`               → `asset_type = 0`
--               (task 0243, `assets/queries_ch.rs` + `canonical_id` in
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
--     → `'Stellar Lumen'` for native. `symbol`/`decimals` come from
--     `soroban_contract_metadata` (decimals defaults to 7 for classic/SAC).
--     This is the joined detail variant (keeps the `accounts` issuer join,
--     which the list `08` drops per task 0319).

-- Shown with the contract-StrKey predicate (primary Soroban path). The API
-- swaps the WHERE for the CODE-ISSUER / native forms above; SELECT is identical.
SELECT
    a.asset_type                        AS asset_type,
    nullIf(a.asset_code, '')            AS asset_code,
    nullIf(iss.account_id, '')          AS issuer,
    nullIf(iss.home_domain, '')         AS issuer_home_domain,  -- internal SEP-1 key
    nullIf(sc.contract_id, '')          AS contract_id,
    coalesce(nullIf(ae.name, ''), nullIf(m.name, ''),
             if(a.asset_type = 0, 'Stellar Lumen', NULL)) AS name,
    nullIf(m.symbol, '')                AS symbol,
    coalesce(m.decimals, 7)             AS decimals,
    toString(agg.total_supply)          AS total_supply,
    agg.holder_count                    AS holder_count,
    nullIf(sc.deployed_at_ledger, 0)    AS deployed_at_ledger,
    nullIf(ae.icon_url, '')             AS icon_url,
    a.issuer_id                         AS issuer_id_key,
    a.contract_id                       AS contract_id_key
    -- not in DB: description, home_page — runtime SEP-1 fetch via
    --   `runtime_enrichment::sep1` (task 0188), keyed off issuer_home_domain.
FROM assets a FINAL
LEFT JOIN accounts          iss ON iss.id = a.issuer_id
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
