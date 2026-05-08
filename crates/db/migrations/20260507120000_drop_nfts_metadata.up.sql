-- Task 0195 §2d: drop `nfts.metadata` JSONB column.
-- NFT metadata (attributes, traits, description, animation_url, etc.) is
-- detail-only per ADR 0043 — read by `GET /v1/nfts/:id`, never by list.
-- Now served via runtime type-2 enrichment (`runtime_enrichment::nft_metadata`)
-- mirroring the `assets.description` precedent in
-- `20260424000000_drop_assets_sep1_detail_cols.up.sql`.
--
-- Lambda 2 §2d still writes the three list-endpoint columns
-- (`name`, `media_url`, `collection_name`) from the same `token_uri()`
-- IPFS fetch.
--
-- Zero data loss: the 2026-04-10 pipeline audit confirmed `metadata`
-- was always NULL (parser hardcodes `None`; no enrichment path has
-- written it yet).

ALTER TABLE nfts DROP COLUMN metadata;
