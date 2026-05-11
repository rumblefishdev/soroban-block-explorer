-- Endpoint:     GET /nfts/:id
-- Purpose:      NFT detail: identity + media + current owner. The
--               `metadata` JSON blob (attributes, traits, description,
--               animation_url, etc.) is NOT served from this query — see
--               Notes (CH `nfts` table has no metadata column per ADR
--               0044 §5.3, same as PG post-migration-20260507120000).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.12
-- Schema:       ADR 0044 (CH pilot, §5.3 metadata dropped), parallel to
--               PG ADR 0037 + ADR 0043
-- Data sources: DB-only column projection + API-layer runtime fold-in for
--               metadata (Soroban RPC `token_uri()` + IPFS gateway).
-- Inputs:
--   $1  :id  Int32  NFT surrogate id
-- Indexes:      nfts ORDER BY (id) — direct seek after FINAL.
--               soroban_contracts ORDER BY (id), accounts ORDER BY (id).
-- CH Engine:    nfts — Replacing(current_owner_ledger) (FINAL).
--               soroban_contracts, accounts — Replacing (FINAL).
-- CH Pattern:   single FINAL'd JOIN tree.
-- ADR 0044 §:   §4.6 (nfts Replacing, no metadata col), §5.3 (divergence).
-- Notes:
--   • Same shape as PG E16 (post-ADR-0043 migration). Both sides return
--     identical column sets — neither projects `nfts.metadata`.
--   • The detail handler (`crates/api/src/nfts/handler.rs::get_detail`)
--     fetches the JSON blob at request time from the per-token `token_uri()`
--     URL via `runtime_enrichment::nft_token_uri` (LRU 24h, fail-soft) and
--     surfaces it as `metadata` on the JSON response. This is the same path
--     PG-backed and CH-backed; ADR 0043 detail-only carve-out applies to
--     both stores.
--   • Burned NFT: `current_owner_id = NULL` → LEFT JOIN yields NULL
--     `current_owner` (per ADR 0037 §13).

SELECT
    n.id,
    sc.contract_id,
    n.token_id,
    n.collection_name,
    n.name,
    n.media_url,
    n.minted_at_ledger,
    own.account_id                            AS current_owner,
    n.current_owner_ledger
FROM nfts n FINAL
JOIN      soroban_contracts sc  FINAL ON sc.id  = n.contract_id
LEFT JOIN accounts          own FINAL ON own.id = n.current_owner_id
WHERE n.id = $1;
