-- ⚠️ SUPERSEDED by task 0331 (unified balances). `total_supply` / `holder_count` now
-- come from `balance_aggregates` (refreshable MV over the unified `balances` table,
-- keyed by the RE-ADDED `assets.id` surrogate — PR #175 dropped it, 0331 restored it),
-- NOT `asset_aggregates`. Post-ADR-0051 a SAC is a FACET folded into its classic row,
-- so there is ONE aggregate row per asset (no separate SAC row). Authoritative query:
-- `crates/api/src/assets/queries_ch.rs`. Reference SQL below is pre-0331.
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
--               soroban_contracts ORDER BY (contract_id) — contract join via id.
--               soroban_contract_metadata ORDER BY (contract_id) — metadata join.
--               asset_enrichment / asset_aggregates — side-table joins (below).
-- CH Engine:    assets — ReplacingMergeTree (no version) (FINAL required).
--               soroban_contracts — Replacing (joined WITHOUT FINAL; a join
--                 miss is neutralised by `nullIf`, the ~18M-row FINAL avoided).
--               soroban_contract_metadata — Replacing(version) (FINAL in the
--                 sub-select → latest row per contract_id).
--               asset_enrichment — Replacing(version), collapsed via argMax in
--                 a GROUP BY sub-select (no FINAL).
--               asset_aggregates — MergeTree batch table (no FINAL).
-- CH Pattern:   FINAL on `assets` + the metadata sub-select only. Keyset cursor
--               on the 4-tuple natural key (DESC) for stable pagination.
--               asset_code substring via positionCaseInsensitiveUTF8 (no
--               pg_trgm in CH; linear scan acceptable on small `assets`).
-- ADR 0044 §:   §4.5 (Replacing state). **PR #175 amendment:** `assets`
--               dropped surrogate `id Int32`; natural composite key now.
--               `issuer_id` / `contract_id` are `Int64` (NOT Nullable);
--               `0` is the sentinel for "no issuer" / "no contract" (native
--               XLM: both 0; classic credit: contract_id=0; soroban-native:
--               issuer_id=0).
-- Notes:
--   • PR #175 dropped `assets.id`. Route via the 4-tuple natural key (or a
--     synthesized `cityHash64(...)` at the API layer).
--   • **List variant (task 0319): NO `accounts` issuer join.** The issuer
--     StrKey + home_domain are resolved per page by a bloom-pruned key-seek;
--     the ~18M-row `accounts` hash-build was the dominant list cost. The
--     detail paths (09) keep the issuer join. `issuer_id_key` is exposed for
--     the cursor + that per-page seek.
--   • **name / symbol / decimals are read-composed from side tables, NOT from
--     the `assets` row.** `assets.name` has had no writer since task 0297
--     (empty going forward) and no reader; pending `DROP COLUMN` (task 0310). Name
--     precedence: `asset_enrichment.name` (classic/SAC enrichment, task 0231)
--     → `soroban_contract_metadata.name` (on-chain SEP-41 `METADATA`, task
--     0297) → `'Stellar Lumens'` literal for native. `symbol` / `decimals`
--     come from `soroban_contract_metadata` (decimals defaults to 7 for
--     classic/SAC). PG still reads `a.name` until 0243 (see the PG snapshot).

SELECT
    a.asset_type                              AS asset_type,
    nullIf(a.asset_code, '')                  AS asset_code,
    nullIf(sc.contract_id, '')                AS contract_id,
    coalesce(nullIf(ae.name, ''), nullIf(m.name, ''),
             if(a.asset_type = 0, 'Stellar Lumens', NULL)) AS name,
    nullIf(m.symbol, '')                      AS symbol,         -- on-chain token symbol (0297); NULL for classic/native
    coalesce(m.decimals, 7)                   AS decimals,       -- on-chain decimals (0297); 7 = classic/SAC default
    toString(agg.total_supply)                AS total_supply,   -- asset_aggregates (task 0293)
    agg.holder_count                          AS holder_count,
    nullIf(sc.deployed_at_ledger, 0)          AS deployed_at_ledger,
    nullIf(ae.icon_url, '')                   AS icon_url,       -- asset_enrichment (task 0231)
    a.issuer_id                               AS issuer_id_key,  -- cursor + per-page issuer seek
    a.contract_id                             AS contract_id_key
FROM assets a FINAL
LEFT JOIN soroban_contracts sc ON sc.id = a.contract_id
LEFT JOIN (
    SELECT contract_id, name, symbol, decimals
    FROM soroban_contract_metadata FINAL      -- task 0297 side table; RMT(version) → latest per contract
) m ON m.contract_id = sc.contract_id
-- Task 0331: `asset_aggregates` is retired. Supply and holders come from
-- `balance_aggregates`, the refreshable MV over the unified `balances` table,
-- keyed by the `assets.id` surrogate rather than by (asset_code, issuer_id) —
-- so one row covers a classic asset and its SAC facet together.
LEFT JOIN balance_aggregates agg
       ON agg.asset_id = a.id
LEFT JOIN (
    SELECT asset_type, asset_code, issuer_id, contract_id,
           argMax(icon_url, version) AS icon_url,
           argMax(name, version)     AS name
    FROM asset_enrichment                     -- task 0231 side table
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) ae ON ae.asset_type = a.asset_type AND ae.asset_code = a.asset_code
    AND ae.issuer_id = a.issuer_id   AND ae.contract_id = a.contract_id
WHERE
    ($2 IS NULL OR (a.asset_type, a.asset_code, a.issuer_id, a.contract_id) < ($2, $3, $4, $5))
    AND ($6 IS NULL OR a.asset_type = $6)
    AND ($7 IS NULL OR positionCaseInsensitiveUTF8(a.asset_code, $7) > 0)
ORDER BY a.asset_type DESC, a.asset_code DESC, a.issuer_id DESC, a.contract_id DESC
LIMIT $1;
