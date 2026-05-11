-- Endpoint:     GET /nfts/:id/transfers
-- Purpose:      Paginated transfer/ownership history for a single NFT.
--               Default ordering: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.12
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :nft_id                Int32   NFT surrogate id
--   $2  :limit                 Int     page size
--   $3  :cursor_ledger         Int64   NULL on first page
--   $4  :cursor_event_order    Int16   NULL on first page
-- Indexes:      nfts ORDER BY (id) — keyed input.
--               nft_ownership ORDER BY (nft_id, ledger_sequence, event_order)
--                 + PARTITION BY intDiv(ledger_sequence, 500000). The keyset
--                 walks the natural PK direction.
--               transactions ORDER BY (ledger_sequence, application_order, id)
--                 — composite join for transaction_hash.
-- CH Engine:    nft_ownership — Replacing partitioned (FINAL).
--               accounts, transactions — Replacing (FINAL).
-- CH Pattern:   LEAD window function for from_owner synthesis (CH 23.x+
--                 supports LEAD with PARTITION BY + ORDER BY).
--               Cursor drops `created_at` (§5.2) — natural keyset is
--                 `(ledger_sequence, event_order)`.
-- ADR 0044 §:   §4.3 (nft_ownership Replacing partitioned), §4.5 (accounts
--                 Replacing state), §5.2 (no created_at — partition prune
--                 via intDiv on cursor's ledger).
-- Notes:
--   • Same `LEAD()`-on-DESC-window pattern as PG E17 — see the PG file's
--     "from_owner synthesis" note for the careful reasoning around LEAD
--     vs LAG on a DESC-ordered window and the page-boundary peek-row
--     stitching contract.
--   • CH supports `LEAD(expr) OVER (PARTITION BY ... ORDER BY ...)` since
--     23.x. The `OVER` clause is identical to PG syntax (CH ANSI-window).
--   • Cursor drops `created_at` term — natural CH keyset is
--     `(ledger_sequence, event_order)` since those columns form the PK
--     ordering and §5.2 dropped `created_at` from `nft_ownership`.
--   • `event_type_name` PG helper not available in CH — project raw Int16,
--     decode in API.
--   • `owner_id` is NULL on a burn (per ADR 0037 §13 + ADR 0044 §4.3 same
--     semantics); LEFT JOIN handles it.
--   • Transaction-hash join uses just `transaction_id = nft_ownership.transaction_id`
--     since CH transactions has no `created_at` (§5.2). Partition pruning
--     on transactions via intDiv on `nft_ownership.ledger_sequence`.

SELECT
    no.ledger_sequence,
    no.event_order,
    no.event_type                                                                   AS event_type,
    LEAD(own.account_id) OVER (
        PARTITION BY no.nft_id
        ORDER BY no.ledger_sequence DESC,
                 no.event_order DESC
    )                                                                               AS from_owner,
    own.account_id                                                                  AS to_owner,
    lower(hex(t.hash))                                                              AS transaction_hash_hex
FROM nft_ownership no FINAL
LEFT JOIN accounts     own FINAL ON own.id = no.owner_id
JOIN      transactions t   FINAL ON t.id = no.transaction_id AND t.ledger_sequence = no.ledger_sequence
WHERE no.nft_id = $1
  AND ($3 IS NULL
       OR (no.ledger_sequence, no.event_order) < ($3, $4))
ORDER BY no.ledger_sequence DESC, no.event_order DESC
LIMIT $2;
