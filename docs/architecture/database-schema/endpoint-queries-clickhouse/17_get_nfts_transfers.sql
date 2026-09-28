-- Endpoint:     GET /nfts/:id/transfers
-- Purpose:      Paginated transfer/ownership history for a single NFT.
--               Default ordering: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.12
-- Schema:       ADR 0044 + PR-#175 hybrid-surrogate amendment; ADR 0059
--               (each change located by its source event, task 0424).
-- Data sources: DB-only.
-- Inputs:
--   $1  :contract_strkey       String  C-form contract ID
--   $2  :token_id              String  on-chain token_id
--   $3  :limit                 Int     page size
--   $4  :cursor_ledger         Int64   NULL on first page
--   $5  :cursor_app_order      Int16   NULL on first page
--   $6  :cursor_operation      UInt16  NULL on first page
--   $7  :cursor_event          UInt32  NULL on first page
-- Indexes:      soroban_contracts ORDER BY (contract_id) — StrKey resolve.
--               nft_ownership_changes ORDER BY (contract_id, token_id,
--                 ledger_sequence, application_order, operation_index,
--                 event_index) + PARTITION BY intDiv(ledger_sequence, 500000).
--               transactions ORDER BY (ledger_sequence, application_order)
--                 + intDiv partition.
--               accounts ORDER BY (account_id) — owner LEFT JOIN by id.
-- CH Engine:    nft_ownership_changes — Replacing partitioned.
--               accounts, transactions — Replacing.
-- CH Pattern:   LEAD window for from_owner synthesis. The cursor walks the
--               PK after its pinned (contract_id, token_id) prefix: the
--               change's location, which is the chain's own order.
-- Notes:
--   • Same `LEAD()`-on-DESC-window pattern as PG E17 for from_owner; the
--     API uses `leadInFrame` with an explicit full frame and dedups the page
--     with `LIMIT 1 BY` on the location before the window runs.
--   • Before task 0424 this read `nft_ownership`, ordered inside a ledger by
--     `event_order` — our own per-token counter — and joined `transactions`
--     on the `transaction_id` surrogate.
--   • `event_type_name` PG helper not available — project raw Int16.
--   • `owner_id` is NULL on a burn — LEFT JOIN handles it.
--   • The transaction is joined on its position, the `transactions` key.

SELECT
    no.ledger_sequence,
    no.application_order,
    no.operation_index,
    no.event_index,
    no.event_type                                                                   AS event_type,
    LEAD(own.account_id) OVER (
        PARTITION BY no.contract_id, no.token_id
        ORDER BY no.ledger_sequence DESC, no.application_order DESC,
                 no.operation_index DESC, no.event_index DESC
    )                                                                               AS from_owner,
    own.account_id                                                                  AS to_owner,
    lower(hex(t.hash))                                                              AS transaction_hash_hex
FROM nft_ownership_changes no
LEFT JOIN accounts     own ON own.id = no.owner_id AND no.owner_id IS NOT NULL
JOIN      transactions t   ON t.ledger_sequence = no.ledger_sequence
                          AND t.application_order = no.application_order
WHERE no.contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = $1 LIMIT 1)
  AND no.token_id    = $2
  AND ($4 IS NULL
       OR (no.ledger_sequence, no.application_order, no.operation_index, no.event_index)
          < ($4, $5, $6, $7))
ORDER BY no.ledger_sequence DESC, no.application_order DESC,
         no.operation_index DESC, no.event_index DESC
LIMIT $3;
