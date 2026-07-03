-- Endpoint:     GET /nfts/:id
-- Purpose:      NFT detail: identity + media + current owner. The
--               `metadata` JSONB blob (attributes, traits,
--               description, animation_url, etc.) is NOT served from
--               this query — see Notes.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.12
-- Schema:       ADR 0037, ADR 0043 (detail-only allocation)
-- Data sources: DB columns + runtime type-2 enrichment fold-in.
-- Inputs:
--   $1  :id  INT  NFT surrogate id
-- Indexes:      nfts PK (id),
--               soroban_contracts PK (id) for contract join,
--               accounts PK (id) for owner join.
-- Notes:
--   • Single statement. Owner LEFT JOIN tolerates NULL — happens for
--     burned NFTs (current_owner_id NULL) per ADR 0037 §13.
--   • The `nfts.metadata` JSONB column was dropped in migration
--     20260507120000_drop_nfts_metadata.up.sql per ADR 0043
--     detail-only carve-out (task 0195 §2d). The detail handler
--     fetches the JSON blob at request time from the per-token
--     `token_uri()` URL (Soroban RPC + IPFS gateway via
--     `runtime_enrichment::nft_token_uri`, LRU 24h, fail-soft) and
--     surfaces it as `metadata` on `NftDetailResponse`. The contract-
--     defined JSONB shape is therefore intentionally not standardised
--     in this file.
--
-- ClickHouse read path (task 0243 — MIGRATED; see crates/api/src/nfts/queries_ch.rs):
--   • The wire `id` surrogate is dropped (external identity is the composite
--     (contract_id, token_id) per task 0264 Phase 8a; CH has no surrogate).
--   • `name`/`media_url`/`collection_name` come from `nft_enrichment`
--     (`argMax(_, version)`), NOT the vestigial-NULL `nfts` columns.
--   • `nfts n FINAL` (collapse re-ingest); plain no-FINAL LEFT JOINs to
--     `accounts` (owner) / `soroban_contracts` — one id is a cheap bloom probe,
--     so the restricted CTE the list uses is unnecessary for a single row.

SELECT
    n.id,
    sc.contract_id,
    n.token_id,
    n.collection_name,
    n.name,
    n.media_url,
    n.minted_at_ledger,
    own.account_id    AS current_owner,
    n.current_owner_ledger
FROM nfts n
JOIN      soroban_contracts sc  ON sc.id  = n.contract_id
LEFT JOIN accounts          own ON own.id = n.current_owner_id
WHERE n.id = $1;
