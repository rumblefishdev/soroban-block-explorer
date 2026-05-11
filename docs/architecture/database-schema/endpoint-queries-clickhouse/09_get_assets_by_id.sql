-- Endpoint:     GET /assets/:id
-- Purpose:      Asset detail. DB returns the typed-metadata header (code,
--               type, supply, holder_count, icon, name) plus the issuer's
--               on-chain home_domain used as the SEP-1 lookup key. The API
--               then runs a runtime SEP-1 fetch against the issuer's
--               stellar.toml to overlay description + home_page.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.9
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB + runtime SEP-1 HTTP fetch (per request).
-- Inputs:
--   $1  :id  Int32  asset surrogate id (API resolves StrKey / contract
--                   identity to this id at the request boundary)
-- Indexes:      assets ORDER BY (id) — direct PK seek after FINAL.
--               accounts, soroban_contracts ORDER BY (id) for joins.
-- CH Engine:    All three Replacing — FINAL required.
-- CH Pattern:   single FINAL'd JOIN tree; identical to PG shape.
-- ADR 0044 §:   §4.5 (Replacing state tables).
-- Notes:
--   • Same shape as PG E09. SEP-1 fetch happens at the API layer; not in SQL.
--   • `issuer_home_domain` is the SEP-1 lookup key — projected for internal
--     consumption, not serialised to API response.
--   • `deployed_at_ledger` from soroban_contracts is populated for SAC /
--     soroban-native types; classic + native return NULL via LEFT JOIN.

SELECT
    a.id,
    a.asset_type                        AS asset_type,
    a.asset_code,
    iss.account_id                      AS issuer,
    iss.home_domain                     AS issuer_home_domain,  -- internal SEP-1 key
    sc.contract_id                      AS contract_id,
    a.name,
    a.total_supply,
    a.holder_count,
    a.icon_url,
    sc.deployed_at_ledger               AS deployed_at_ledger
    -- not in DB: description, home_page — runtime SEP-1 fetch via
    --   `runtime_enrichment::sep1` (task 0188), keyed off issuer_home_domain.
FROM assets a FINAL
LEFT JOIN accounts          iss FINAL ON iss.id = a.issuer_id
LEFT JOIN soroban_contracts sc  FINAL ON sc.id  = a.contract_id
WHERE a.id = $1;
