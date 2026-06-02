-- Endpoint:     GET /nfts
-- Purpose:      Paginated list of NFTs. Optional filters: collection name,
--               contract id, name substring.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.11
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment + ADR 0043.
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit                 Int     page size
--   $2  :cursor_contract_id    Int64   NULL on first page (tuple cursor)
--   $3  :cursor_token_id       String  NULL on first page
--   $4  :collection_name       String  NULL = no filter (exact match)
--   $5  :contract_strkey       String  NULL = no filter (resolved to Int64 id)
--   $6  :name                  String  NULL = no filter (substring)
-- Indexes:      nfts ORDER BY (contract_id, token_id) — natural composite
--                 (PR #175 dropped surrogate `id Int32`).
--               soroban_contracts ORDER BY (contract_id) — StrKey resolve.
--               accounts ORDER BY (account_id) — owner LEFT JOIN by id.
-- CH Engine:    nfts — Replacing(current_owner_ledger) (FINAL).
--               soroban_contracts, accounts — Replacing (FINAL).
-- CH Pattern:   FINAL on all reads; tuple cursor matches natural PK shape;
--               contract StrKey resolve via correlated scalar on
--               `soroban_contracts.id` (cityhash64 hub).
-- ADR 0044 §:   §4.6 (nfts Replacing, **no metadata col**), §5.3 (metadata
--               dropped). **PR #175 amendment:** `nfts.id` surrogate dropped;
--               natural composite key (contract_id, token_id).
-- Notes:
--   • Cursor on `(contract_id DESC, token_id DESC)` walks the natural PK
--     in one direction. Frontend that wants a single opaque id can compute
--     `cityHash64(toString(contract_id), token_id)` synthetic surrogate.
--   • Collection_name filter is exact (PG side too — no trigram on
--     collection_name).
--   • Name substring via `positionCaseInsensitiveUTF8` — same pg_trgm
--     regression as E08 (linear scan).
--   • Metadata served by API layer via `runtime_enrichment::nft_token_uri`
--     (ADR 0043 detail-only carve-out).

SELECT
    n.contract_id                             AS contract_id_raw,
    sc.contract_id                            AS contract_id_strkey,
    n.token_id,
    n.collection_name,
    n.name,
    n.media_url,
    n.minted_at_ledger,
    own.account_id                            AS current_owner,
    n.current_owner_ledger
FROM nfts n FINAL
JOIN      soroban_contracts sc  FINAL ON sc.id  = n.contract_id
LEFT JOIN accounts          own FINAL ON own.id = n.current_owner_id AND n.current_owner_id IS NOT NULL
WHERE
    ($2 IS NULL OR (n.contract_id, n.token_id) < ($2, $3))
    AND ($4 IS NULL OR n.collection_name = $4)
    AND ($5 IS NULL OR n.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $5 LIMIT 1))
    AND ($6 IS NULL OR positionCaseInsensitiveUTF8(coalesce(n.name, ''), $6) > 0)
ORDER BY n.contract_id DESC, n.token_id DESC
LIMIT $1;
