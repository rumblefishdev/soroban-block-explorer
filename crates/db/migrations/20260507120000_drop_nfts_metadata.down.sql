-- Reverse of 20260507120000_drop_nfts_metadata.up.sql.
-- Re-adds `metadata` as a nullable JSONB column (original shape from
-- migration 0005). Pre-drop data is not restored — per the 2026-04-10
-- pipeline audit there was no such data (always NULL), so no backfill
-- is required.

ALTER TABLE nfts ADD COLUMN metadata JSONB;
