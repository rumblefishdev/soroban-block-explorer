-- Endpoint:     GET /nfts/:id
-- Purpose:      NFT detail: identity + media + current owner. The
--               `metadata` JSON blob is NOT served from this query — see
--               Notes (CH `nfts` has no metadata col per ADR 0044 §5.3,
--               same as PG post-migration-20260507120000).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.12
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment + ADR 0043.
-- Data sources: DB columns + API-layer runtime fold-in for metadata
--               (Soroban RPC `token_uri()` + IPFS gateway).
-- Inputs:
--   $1  :contract_strkey  String  C-form contract ID
--   $2  :token_id         String  on-chain token_id (i128/u64/sym/Bytes as String)
-- Indexes:      nfts ORDER BY (contract_id, token_id) — direct PK seek
--                 after FINAL (PR #175 dropped surrogate `id`).
--               soroban_contracts ORDER BY (contract_id) — StrKey resolve.
--               accounts ORDER BY (account_id) — owner LEFT JOIN by id.
-- CH Engine:    nfts — Replacing(current_owner_ledger) (FINAL).
--               soroban_contracts, accounts — Replacing (FINAL).
-- CH Pattern:   FINAL'd direct seek on natural composite key. Inputs
--               changed from single `:id` Int32 (PG surrogate) to
--               (contract_strkey, token_id) tuple per PR #175.
-- ADR 0044 §:   §4.6 (nfts Replacing, no metadata col), §5.3 (divergence).
--   **PR #175 amendment:** inputs changed from `$1 = id Int32` to
--   `(contract_strkey, token_id)` tuple — surrogate dropped.
-- Notes:
--   • PG E16 keeps single `:id` Int32. CH has no surrogate; API passes
--     the tuple. Migration paths: (a) PG-backed id→tuple lookup; (b)
--     frontend uses synthetic cityHash64 surrogate from E15 as routing
--     key and the API decomposes.
--   • Burned NFT: `current_owner_id IS NULL` → LEFT JOIN yields NULL.

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
WHERE n.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $1 LIMIT 1)
  AND n.token_id    = $2;
