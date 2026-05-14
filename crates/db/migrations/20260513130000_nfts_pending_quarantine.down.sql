-- Reverse of 20260513130000_nfts_pending_quarantine.up.sql.
--
-- Drops the quarantine tables. Note: rows still in `nfts_pending` /
-- `nft_ownership_pending` at the time of DROP are lost — for a
-- production rollback, drain them first via the post-backfill drain
-- procedure in
-- `docs/runbooks/0217_nfts_pending_migration_and_drain.md` §Part 2.

DROP TABLE IF EXISTS nft_ownership_pending;
DROP TABLE IF EXISTS nfts_pending;
