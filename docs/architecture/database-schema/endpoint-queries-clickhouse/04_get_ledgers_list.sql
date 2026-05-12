-- Endpoint:     GET /ledgers
-- Purpose:      Paginated list of ledgers for the chain history browser.
--               Default ordering: newest closed_at first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.5
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :limit              Int           page size (validated 1..200 in API)
--   $2  :cursor_closed_at   DateTime64    NULL on first page
--   $3  :cursor_sequence    Int64         NULL on first page
-- Indexes:      ledgers ORDER BY (sequence). Stellar's sequence ↔ closed_at
--               monotonicity means scanning DESC by sequence is equivalent
--               to scanning DESC by closed_at for chain-tip pagination; CH
--               does not materialise a secondary closed_at index.
-- CH Engine:    ledgers — plain MergeTree (no FINAL).
-- CH Pattern:   keyset cursor on (closed_at DESC, sequence DESC), tuple
--               comparison; ORDER BY by sequence still scans the right
--               direction because closed_at and sequence are co-monotone
--               on Stellar (the planner reads the sparse PK in reverse).
-- ADR 0044 §:   §4.1 (ledgers plain MergeTree, immutable lookup).
-- Notes:
--   • Same cursor shape as PG E04 — preserving (closed_at, sequence) tuple
--     so the API layer does not need a CH-specific cursor format.
--   • CH `hex(FixedString)` returns UPPERCASE; PG `encode(bytea, 'hex')`
--     returns lowercase. `lower(hex(...))` normalises to the PG-compatible
--     hex string the frontend already expects.
--   • Both filter and order touch ORDER BY-prefix columns directly — the
--     sparse PK is usable end-to-end. No FINAL needed because ledgers is
--     an immutable lookup table (every ingested ledger is unique by PK).

SELECT
    l.sequence,
    lower(hex(l.hash))      AS hash_hex,
    l.closed_at,
    l.protocol_version,
    l.transaction_count,
    l.base_fee
FROM ledgers l
WHERE $2 IS NULL
   OR (l.closed_at, l.sequence) < ($2, $3)
ORDER BY l.closed_at DESC, l.sequence DESC
LIMIT $1;
