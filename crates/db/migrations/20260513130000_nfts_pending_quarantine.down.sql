-- Reverse of 20260513130000_nfts_pending_quarantine.up.sql.
--
-- Drops the quarantine tables. Note: rows still in `nfts_pending` /
-- `nft_ownership_pending` at the time of DROP are lost — for a
-- production rollback, drain them first via the procedure in
-- `docs/runbooks/0217_nfts_pending_drain.md` (TODO).

DROP TABLE IF EXISTS nft_ownership_pending;
DROP TABLE IF EXISTS nfts_pending;
