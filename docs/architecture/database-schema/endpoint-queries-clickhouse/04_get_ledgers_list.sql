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

-- Step 2 (task 0445): per-ledger successful-transaction count for the page.
--
-- A separate round trip, not a JOIN or subquery on the read above: that read is
-- tuned for optimize_read_in_order, and hanging an aggregate off it risks the
-- plan. Runs after the page rows are deduped, bounded by their min/max sequence.
--
-- Cheap because transactions is ORDER BY (ledger_sequence, application_order),
-- so the range is a PK-prefix seek — measured 16,384 read_rows / 176 KiB / 5 ms
-- for a 10-ledger page (2026-08-12). Granule-bound, so a wider page costs the
-- same.
--
-- uniqExactIf, not countIf: transactions is a ReplacingMergeTree. FINAL is not
-- an option (0420 measured 19x read amplification on a comparable read).
--
-- A ledger absent from this result keeps a NULL successful count on the wire —
-- distinct from 0, which would assert that every transaction in it failed.
--
-- Inputs:
--   $1  :min_sequence  Int64   lowest sequence on the deduped page
--   $2  :max_sequence  Int64   highest sequence on the deduped page

SELECT
    ledger_sequence,
    uniqExactIf(application_order, successful) AS successful_count
FROM transactions
WHERE ledger_sequence BETWEEN $1 AND $2
GROUP BY ledger_sequence;
