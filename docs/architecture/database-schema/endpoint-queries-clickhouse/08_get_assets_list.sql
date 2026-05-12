-- Endpoint:     GET /assets
-- Purpose:      Paginated list of assets across all four types (native,
--               classic credit, SAC, soroban-native). Optional filters:
--               type, asset_code (substring search).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.8
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment.
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                 Int     page size
--   $2  :cursor_asset_type     Int16   NULL on first page (tuple cursor)
--   $3  :cursor_asset_code     String  NULL on first page
--   $4  :cursor_issuer_id      Int64   NULL on first page
--   $5  :cursor_contract_id    Int64   NULL on first page
--   $6  :asset_type_filter     Int16   NULL = no filter
--   $7  :asset_code_filter     String  NULL = no filter (substring)
-- Indexes:      assets ORDER BY (asset_type, asset_code, issuer_id, contract_id)
--                 — natural composite key (PR #175 dropped surrogate `id`).
--               accounts ORDER BY (account_id) — issuer join via accounts.id.
--               soroban_contracts ORDER BY (contract_id) — contract join via id.
-- CH Engine:    assets — ReplacingMergeTree (no version) (FINAL required).
--               accounts, soroban_contracts — ReplacingMergeTree (FINAL).
-- CH Pattern:   FINAL on all three reads. Keyset cursor on the 4-tuple
--               natural key (DESC) for stable pagination. asset_code
--               substring via positionCaseInsensitiveUTF8 (no pg_trgm in CH;
--               linear scan acceptable on small `assets` table).
-- ADR 0044 §:   §4.5 (Replacing state). **PR #175 amendment:** `assets`
--               dropped surrogate `id Int32`; natural composite key now.
--               `issuer_id` / `contract_id` are `Int64` (NOT Nullable);
--               `0` is the sentinel for "no issuer" / "no contract" (native
--               XLM: both 0; classic credit: contract_id=0; soroban-native:
--               issuer_id=0).
-- Notes:
--   • PR #175 dropped `assets.id`. Routing previously used the surrogate;
--     now route via the 4-tuple natural key. Frontends that want a single
--     opaque id can synthesize via
--     `cityHash64(toString(asset_type), asset_code, toString(issuer_id),
--     toString(contract_id))` at the API layer.
--   • `iss.id = 0` and `sc.id = 0` LEFT JOINs never match because hub
--     surrogates use `cityhash64(strkey)` (0 reserved for sentinels) —
--     semantically equivalent to PG's `NULL` issuer/contract.
--   • Cursor tuple matches the ORDER BY shape DESC so the planner walks
--     the sparse PK in one direction.

SELECT
    a.asset_type                              AS asset_type,
    a.asset_code                              AS asset_code,
    iss.account_id                            AS issuer,           -- NULL when issuer_id = 0
    sc.contract_id                            AS contract_id,      -- NULL when contract_id = 0
    a.issuer_id                               AS issuer_id_raw,    -- exposed for cursor / routing
    a.contract_id                             AS contract_id_raw,  -- exposed for cursor / routing
    a.name,
    a.total_supply,
    a.holder_count,
    a.icon_url
FROM assets a FINAL
LEFT JOIN accounts          iss FINAL ON iss.id = a.issuer_id   AND a.issuer_id   != 0
LEFT JOIN soroban_contracts sc  FINAL ON sc.id  = a.contract_id AND a.contract_id != 0
WHERE
    ($2 IS NULL OR (a.asset_type, a.asset_code, a.issuer_id, a.contract_id) < ($2, $3, $4, $5))
    AND ($6 IS NULL OR a.asset_type = $6)
    AND ($7 IS NULL OR positionCaseInsensitiveUTF8(a.asset_code, $7) > 0)
ORDER BY a.asset_type DESC, a.asset_code DESC, a.issuer_id DESC, a.contract_id DESC
LIMIT $1;
