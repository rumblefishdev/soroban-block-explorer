-- Endpoint:     GET /assets/:id
-- Purpose:      Asset detail. DB returns the composed header (code, type,
--               supply, holder_count, icon, name, symbol, decimals, SAC facet)
--               plus the issuer's on-chain home_domain used as the SEP-1
--               lookup key. The API then runs a runtime SEP-1 fetch against
--               the issuer's stellar.toml to overlay description + home_page.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.9
-- Impl:         crates/api/src/assets/queries.rs — `fetch_by_code_issuer`,
--               `fetch_by_contract_id`, `fetch_native`, all through
--               `hydrate_assets` (task 0364 two-phase read, task 0334 issuer
--               key-seek).
-- Schema:       assets, balance_aggregates (task 0331), asset_enrichment,
--               asset_sac, soroban_contracts, soroban_contract_metadata,
--               accounts; ADR 0044.
-- Data sources: DB + runtime SEP-1 HTTP fetch (per request).
-- Inputs:
--   $1  :issuer_strkey   String  G-StrKey of the issuer (CODE-ISSUER form)
--   $2  :asset_code      String  asset code (CODE-ISSUER form)
--   $3  :issuer_id       Int64   `accounts.id` resolved by A (CODE-ISSUER form)
--   $4  :issuer_id_key   Int64   B's `issuer_id_key` (contract / native forms;
--                                skipped when 0)
--   C–E take the resolved key set, inlined by Rust as literal IN lists; the
--   literals below are examples (same convention as 02 / 08).
-- Resolution — the API maps the public `:id` TOKEN to a key at the request
-- boundary, never to a manufactured surrogate:
--   • `CODE-ISSUER`  → A (issuer seek by StrKey) → B (key seek) → C, D, E.
--                      A already yields the issuer StrKey + home_domain, so F
--                      is not run.
--   • contract `C…`  → a bespoke type-3 token IS its contract: its whole key is
--                      `(3, '', 0, surrogate)` and `id == surrogate`, so A/B
--                      are skipped → C, D, E → F.
--   • `native`       → the fixed singleton `(0, '', 0, 0)`, id =
--                      `ids::NATIVE_ASSET_ID` → C, D, E → F (skipped: issuer 0).
--   A SAC `C…` is NOT an asset address (ADR 0051 facet; task 0364) — it 404s.
-- Indexes:      accounts ORDER BY account_id (A: PK point seek); `idx_acc_id`
--                 bloom (F).
--               assets ORDER BY (asset_type, asset_code, issuer_id,
--                 contract_id) — B and D are PK seeks.
--               balance_aggregates keyed by asset_id; soroban_contracts
--                 `idx_sc_id` bloom (E).
-- CH Engine:    assets — ReplacingMergeTree, no FINAL (B takes LIMIT 1; D
--                 collapses versions with `LIMIT 1 BY`).
--               accounts — latest version via `ORDER BY last_seen_ledger DESC
--                 LIMIT 1` (home_domain is mutable), never FINAL (~16M rows).
--               asset_enrichment — Replacing(version), argMax per key.
--               asset_sac / balance_aggregates / soroban_contract_metadata —
--                 as in 08.
-- CH Pattern:   Key resolution → bounded hydration; the SAC-wrapper seek C and
--               the contract context E run concurrently with D.
-- ADR 0044 §:   §4.5 (Replacing state).
-- Notes:
--   • `total_supply` / `holder_count` come from `balance_aggregates` (task
--     0331), keyed by `assets.id` — ONE aggregate row per asset, a SAC's
--     contract-held balances folded into its classic row. The retired
--     `asset_aggregates` is gone.
--   • The detail hydrates WITH the SAC wrapper (C): `deployed_at_ledger` is
--     the own contract's deploy ledger, else the SAC wrapper's. The list (08)
--     drops that field and skips C.
--   • name / symbol / decimals / contract StrKey are assembled in Rust
--     (`assemble_asset_row`), same precedence as 08.
--   • Do NOT manufacture a cityHash64 surrogate as a routing key — the route
--     rejects it (400). Search hits carry the canonical token in `route_token`
--     (see 22_get_search.sql).
--   • SEP-1 fetch happens at the API layer (`runtime_enrichment::sep1`, task
--     0188), keyed off the issuer home_domain; not in SQL.

-- ============================================================================
-- A. CODE-ISSUER form — resolve the issuer StrKey (`seek_latest_account` by
--    `account_id`, the `accounts` primary key). A miss ⇒ 404.
-- ============================================================================
SELECT id AS id, account_id AS account_id, home_domain AS home_domain
FROM accounts WHERE account_id = $1
ORDER BY last_seen_ledger DESC LIMIT 1;

-- @@ split @@
-- ============================================================================
-- B. CODE-ISSUER form — phase-1 key seek (no FINAL). Post-ADR 0051 a
--    `(code, issuer)` names one classic_credit row; `ORDER BY asset_type` is
--    the deterministic tiebreak anyway.
-- ============================================================================
SELECT a.asset_type AS asset_type, a.asset_code AS asset_code,
       a.issuer_id AS issuer_id, a.contract_id AS contract_id, a.id AS id
FROM assets a
WHERE a.asset_code = $2 AND a.issuer_id = $3
ORDER BY a.asset_type LIMIT 1;

-- @@ split @@
-- ============================================================================
-- C. SAC-wrapper surrogate (detail only) — feeds E, for the wrapper's deploy
--    ledger.
-- ============================================================================
SELECT DISTINCT sac_contract_id AS id
FROM asset_sac
WHERE (asset_type, asset_code, issuer_id, contract_id) IN ((1,'USDC',987654321,0))
  AND sac_contract_id != 0;

-- @@ split @@
-- ============================================================================
-- D. Phase-2 hydration (`hydrate_sql`) — identical to 08 statement B, here
--    for one key.
-- ============================================================================
SELECT
    a.asset_type                AS asset_type,
    nullIf(a.asset_code, '')    AS asset_code,
    nullIf(ae.name, '')         AS name_enrichment,
    toString(bagg.total_supply) AS total_supply,
    bagg.holder_count           AS holder_count,
    nullIf(ae.icon_url, '')     AS icon_url,
    a.issuer_id                 AS issuer_id_key,
    a.contract_id               AS contract_id_key,
    sac.sac_contract_id         AS sac_contract_surrogate,
    sac.sac_deployed            AS sac_deployed,
    a.id                        AS id
FROM assets a
LEFT JOIN (
    SELECT asset_id, total_supply, holder_count
    FROM balance_aggregates WHERE asset_id IN (1234567890123)
) bagg ON bagg.asset_id = a.id
LEFT JOIN (
    SELECT asset_type, asset_code, issuer_id, contract_id,
           argMax(icon_url, version) AS icon_url,
           argMax(name, version)     AS name
    FROM asset_enrichment
    WHERE (asset_type, asset_code, issuer_id, contract_id) IN ((1,'USDC',987654321,0))
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) ae ON ae.asset_type  = a.asset_type  AND ae.asset_code  = a.asset_code
    AND ae.issuer_id   = a.issuer_id   AND ae.contract_id = a.contract_id
LEFT JOIN (
    SELECT asset_type, asset_code, issuer_id, contract_id,
           max(sac_contract_id)        AS sac_contract_id,
           toBool(max(sac_deployed))   AS sac_deployed
    FROM asset_sac
    WHERE (asset_type, asset_code, issuer_id, contract_id) IN ((1,'USDC',987654321,0))
    GROUP BY asset_type, asset_code, issuer_id, contract_id
) sac ON sac.asset_type  = a.asset_type  AND sac.asset_code  = a.asset_code
    AND sac.issuer_id   = a.issuer_id   AND sac.contract_id = a.contract_id
WHERE (a.asset_type, a.asset_code, a.issuer_id, a.contract_id) IN ((1,'USDC',987654321,0))
LIMIT 1 BY a.asset_type, a.asset_code, a.issuer_id, a.contract_id;

-- @@ split @@
-- ============================================================================
-- E. `soroban_contracts` context (`resolve_soroban_contracts`) for the own
--    contract (type-3) and the SAC wrapper from C — identical to 08
--    statement C. Skipped when both are absent.
-- ============================================================================
SELECT sc.id AS id,
       argMax(sc.contract_id, sc.wasm_uploaded_at_ledger)        AS contract_id,
       argMax(sc.deployed_at_ledger, sc.wasm_uploaded_at_ledger) AS deployed_at_ledger,
       any(m.name)     AS name,
       any(m.symbol)   AS symbol,
       any(m.decimals) AS decimals
FROM soroban_contracts sc
LEFT JOIN (
    SELECT contract_id, name, symbol, decimals
    FROM soroban_contract_metadata FINAL
) m ON m.contract_id = sc.contract_id
WHERE sc.id IN (3456789012345)
GROUP BY sc.id;

-- @@ split @@
-- ============================================================================
-- F. Contract / native forms — resolve D's `issuer_id_key` to StrKey +
--    home_domain (`resolve_issuer`: the `idx_acc_id` bloom seek). Skipped
--    when it is 0 (native, bespoke Soroban token).
-- ============================================================================
SELECT id AS id, account_id AS account_id, home_domain AS home_domain
FROM accounts WHERE id = $4
ORDER BY last_seen_ledger DESC LIMIT 1;
