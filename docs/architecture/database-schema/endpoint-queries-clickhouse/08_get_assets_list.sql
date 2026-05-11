-- Endpoint:     GET /assets
-- Purpose:      Paginated list of assets across all four types (native,
--               classic credit, SAC, soroban-native). Optional filters:
--               type, asset_code (substring search).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.8
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit         Int       page size
--   $2  :cursor_id     Int       NULL on first page
--   $3  :asset_type    Int16     NULL = no filter (token_asset_type domain)
--   $4  :asset_code    String    NULL = no filter; case-insensitive substring
-- Indexes:      assets ORDER BY (id) — keyset cursor uses this directly.
--               accounts ORDER BY (id), soroban_contracts ORDER BY (id) for joins.
-- CH Engine:    assets — ReplacingMergeTree (no version, FINAL required).
--               accounts, soroban_contracts — ReplacingMergeTree (FINAL).
-- CH Pattern:   FINAL on all three reads. asset_code substring via
--               positionCaseInsensitiveUTF8 — no pg_trgm equivalent in CH;
--               falls back to LIKE-style scan on `assets` (small table,
--               acceptable cost for the pilot).
-- ADR 0044 §:   §4.5 (assets/accounts Replacing state).
-- Notes:
--   • Keyset on `id DESC` — assets surrogate id is mint-time correlated,
--     stable ordering. Same shape as PG E08.
--   • CH has no `pg_trgm` GIN index. The `asset_code` filter uses
--     `positionCaseInsensitiveUTF8` (CH builtin) which scans the whole
--     `assets` table after FINAL dedup. `assets` is small (one row per
--     unique asset across the chain) so this is acceptable — full-text
--     search optimisation is out of scope for the pilot.
--   • `asset_type_name` helper not available in CH — project raw Int16
--     and decode in the API layer.
--   • Native asset has `issuer_id = NULL` and `contract_id = NULL`; LEFT
--     JOIN handles both.

SELECT
    a.id,
    a.asset_type                                AS asset_type,
    a.asset_code,
    iss.account_id                              AS issuer,
    sc.contract_id                              AS contract_id,
    a.name,
    a.total_supply,
    a.holder_count,
    a.icon_url
FROM assets a FINAL
LEFT JOIN accounts          iss FINAL ON iss.id = a.issuer_id
LEFT JOIN soroban_contracts sc  FINAL ON sc.id  = a.contract_id
WHERE
    ($2 IS NULL OR a.id < $2)
    AND ($3 IS NULL OR a.asset_type = $3)
    AND ($4 IS NULL OR positionCaseInsensitiveUTF8(coalesce(a.asset_code, ''), $4) > 0)
ORDER BY a.id DESC
LIMIT $1;
