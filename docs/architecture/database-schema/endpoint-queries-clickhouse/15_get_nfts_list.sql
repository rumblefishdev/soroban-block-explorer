-- Endpoint:     GET /nfts
-- Purpose:      Paginated list of NFTs. Optional filters: collection name,
--               contract id, name substring.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.11
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037 + ADR 0043
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit             Int       page size
--   $2  :cursor_id         Int32     NULL on first page
--   $3  :collection_name   String    NULL = no filter (exact match)
--   $4  :contract_strkey   String    NULL = no filter (resolved to Int64 id)
--   $5  :name              String    NULL = no filter; case-insensitive substring
-- Indexes:      nfts ORDER BY (id) — keyset cursor.
--               soroban_contracts ORDER BY (id) — leading $4 resolve.
--               accounts ORDER BY (id) — owner LEFT JOIN.
-- CH Engine:    nfts — Replacing(current_owner_ledger) (FINAL).
--               soroban_contracts, accounts — Replacing (FINAL).
-- CH Pattern:   FINAL on all reads; contract StrKey resolve via correlated
--               scalar; name substring via positionCaseInsensitiveUTF8
--               (no pg_trgm in CH).
-- ADR 0044 §:   §4.5 (Replacing state), §4.6 (nfts Replacing, **no metadata col**),
--               §5.3 (nfts.metadata dropped — see Notes).
-- Notes:
--   • ADR 0044 §5.3: CH `nfts` table has no `metadata` column. PG dropped
--     this column too in migration 20260507120000 (ADR 0043 detail-only
--     carve-out), so neither side returns metadata on the list endpoint —
--     same MVP behaviour.
--   • The detail endpoint (E16) serves metadata via `runtime_enrichment::nft_token_uri`
--     at API layer (Soroban RPC `token_uri()` + IPFS gateway, fail-soft).
--     Not in SQL on either side.
--   • Same trigram-regression as E08: CH has no pg_trgm. We use
--     `positionCaseInsensitiveUTF8` which is a linear scan — acceptable
--     for the NFTs table (pilot-scale).
--   • Contract resolve uses a correlated subquery with FINAL.

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
JOIN      soroban_contracts sc  FINAL ON sc.id = n.contract_id
LEFT JOIN accounts          own FINAL ON own.id = n.current_owner_id
WHERE
    ($2 IS NULL OR n.id < $2)
    AND ($3 IS NULL OR n.collection_name = $3)
    AND ($4 IS NULL OR n.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $4 LIMIT 1))
    AND ($5 IS NULL OR positionCaseInsensitiveUTF8(coalesce(n.name, ''), $5) > 0)
ORDER BY n.id DESC
LIMIT $1;
