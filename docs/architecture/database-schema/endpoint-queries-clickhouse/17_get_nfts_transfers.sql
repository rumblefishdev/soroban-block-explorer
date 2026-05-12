-- Endpoint:     GET /nfts/:id/transfers
-- Purpose:      Paginated transfer/ownership history for a single NFT.
--               Default ordering: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.12
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment.
-- Data sources: DB-only.
-- Inputs:
--   $1  :contract_strkey       String  C-form contract ID
--   $2  :token_id              String  on-chain token_id
--   $3  :limit                 Int     page size
--   $4  :cursor_ledger         Int64   NULL on first page
--   $5  :cursor_event_order    Int16   NULL on first page
-- Indexes:      soroban_contracts ORDER BY (contract_id) — StrKey resolve.
--               nft_ownership ORDER BY (contract_id, token_id, ledger_sequence,
--                 event_order) + PARTITION BY intDiv(ledger_sequence, 500000).
--                 Natural composite — PR #175 dropped surrogate `nft_id`.
--               transactions ORDER BY (ledger_sequence, application_order)
--                 + intDiv partition.
--               accounts ORDER BY (account_id) — owner LEFT JOIN by id.
-- CH Engine:    nft_ownership — Replacing partitioned (FINAL).
--               accounts, transactions — Replacing (FINAL).
-- CH Pattern:   LEAD window for from_owner synthesis. Cursor on
--               `(ledger_sequence, event_order)` walks the natural PK
--               (contract_id and token_id pinned in WHERE).
-- ADR 0044 §:   §4.3 (nft_ownership Replacing partitioned). **PR #175
--               amendment:** `nft_ownership` uses natural composite key
--               (contract_id, token_id, …); inputs changed from `:nft_id`
--               to (contract_strkey, token_id).
-- Notes:
--   • Same `LEAD()`-on-DESC-window pattern as PG E17 for from_owner.
--   • Cursor on `(ledger_sequence, event_order)` is the natural keyset
--     since the row PK is `(contract_id =, token_id =, ledger_sequence,
--     event_order)` and both leading columns are pinned.
--   • `event_type_name` PG helper not available — project raw Int16.
--   • `owner_id` is NULL on a burn — LEFT JOIN handles it.
--   • Transaction join uses `transaction_id = no.transaction_id` since CH
--     transactions has no `created_at`. Partition pruning on transactions
--     via intDiv on `no.ledger_sequence`.

SELECT
    no.ledger_sequence,
    no.event_order,
    no.event_type                                                                   AS event_type,
    LEAD(own.account_id) OVER (
        PARTITION BY no.contract_id, no.token_id
        ORDER BY no.ledger_sequence DESC,
                 no.event_order DESC
    )                                                                               AS from_owner,
    own.account_id                                                                  AS to_owner,
    lower(hex(t.hash))                                                              AS transaction_hash_hex
FROM nft_ownership no FINAL
LEFT JOIN accounts     own FINAL ON own.id = no.owner_id AND no.owner_id IS NOT NULL
JOIN      transactions t   FINAL ON t.id = no.transaction_id AND t.ledger_sequence = no.ledger_sequence
WHERE no.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $1 LIMIT 1)
  AND no.token_id    = $2
  AND ($4 IS NULL
       OR (no.ledger_sequence, no.event_order) < ($4, $5))
ORDER BY no.ledger_sequence DESC, no.event_order DESC
LIMIT $3;
