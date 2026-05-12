-- Endpoint:     GET /assets/:id
-- Purpose:      Asset detail. DB returns typed-metadata header (code, type,
--               supply, holder_count, icon, name) plus the issuer's on-chain
--               home_domain used as the SEP-1 lookup key. The API then runs
--               a runtime SEP-1 fetch against the issuer's stellar.toml to
--               overlay description + home_page.
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
-- CH Engine:    All three Replacing — FINAL required.
-- CH Pattern:   FINAL'd direct seek on the 4-tuple natural key. API caller
--               supplies the tuple (resolved via PG or re-derived from a
--               synthetic cityHash64 surrogate on the frontend).
-- ADR 0044 §:   §4.5 (Replacing state). **PR #175 amendment:** input shape
--               changes from single `Int32 id` to natural 4-tuple — the
--               surrogate `assets.id` no longer exists.
-- Notes:
--   • PG E09 keeps single `:id` Int32 surrogate. CH side has no surrogate;
--     the API takes the 4-tuple. Migration paths for the API: (a) PG-backed
--     id→tuple lookup before hitting CH; (b) frontend computes the synthetic
--     cityHash64 surrogate from the tuple as opaque routing key.
--   • Sentinels: `issuer_id=0` and `contract_id=0` represent absence;
--     LEFT JOIN never matches because `accounts.id` / `soroban_contracts.id`
--     are derived from `cityhash64(strkey)` (0 reserved).
--   • SEP-1 fetch still happens at API layer; not in SQL.

SELECT
    a.asset_type                        AS asset_type,
    a.asset_code,
    iss.account_id                      AS issuer,
    iss.home_domain                     AS issuer_home_domain,  -- internal SEP-1 key
    sc.contract_id                      AS contract_id_strkey,
    a.issuer_id                         AS issuer_id_raw,
    a.contract_id                       AS contract_id_raw,
    a.name,
    a.total_supply,
    a.holder_count,
    a.icon_url,
    sc.deployed_at_ledger               AS deployed_at_ledger
    -- not in DB: description, home_page — runtime SEP-1 fetch via
    --   `runtime_enrichment::sep1` (task 0188), keyed off issuer_home_domain.
FROM assets a FINAL
LEFT JOIN accounts          iss FINAL ON iss.id = a.issuer_id   AND a.issuer_id   != 0
LEFT JOIN soroban_contracts sc  FINAL ON sc.id  = a.contract_id AND a.contract_id != 0
WHERE a.asset_type = $1
  AND a.asset_code = $2
  AND a.issuer_id  = $3
  AND a.contract_id = $4;
