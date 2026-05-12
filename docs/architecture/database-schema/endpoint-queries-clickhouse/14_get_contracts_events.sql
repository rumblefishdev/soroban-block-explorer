-- Endpoint:     GET /contracts/:contract_id/events
-- Purpose:      Paginated list of recent events emitted by a contract.
--               Default ordering: most recent first.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.10
-- Schema:       ADR 0044 (CH pilot, §5.1 full-content soroban_events),
--               parallel to PG ADR 0037 + ADR 0033 (PG-side folded index)
-- Data sources: DB-only (CH-side).
--               **CH WIN vs PG (§5.1):** CH `soroban_events` is the full-
--               content table — `topics_xdr` + `data_xdr` are inlined per
--               row, no Archive XDR overlay required. PG-side this same
--               endpoint needs Archive overlay because PG ADR 0033 keeps
--               only the appearance index (folded).
-- Inputs:
--   $1  :contract_strkey       String   C-form contract ID
--   $2  :limit                 Int      page size
--   $3  :cursor_ledger         Int64    NULL on first page
--   $4  :cursor_tx_id          Int64    NULL on first page
--   $5  :cursor_event_index    Int16    NULL on first page
-- Indexes:      soroban_contracts ORDER BY (id) — leading resolve.
--               soroban_events ORDER BY (contract_id, ledger_sequence,
--                 transaction_id, event_index) + PARTITION BY intDiv.
--               transactions ORDER BY (ledger_sequence, application_order, id).
-- CH Engine:    soroban_events — Replacing partitioned (FINAL; ORDER BY is
--                 unique per row so dedup is a no-op in practice, but FINAL
--                 ensures idempotent reads under replay).
--               transactions, soroban_contracts — Replacing (FINAL).
-- CH Pattern:   contract-leading sparse PK seek; partition prune via intDiv;
--               full payload returned inline (the major §5.1 divergence vs PG).
-- ADR 0044 §:   §4.4 (soroban_events Replacing partitioned full payload),
--               §5.1 (**no folded appearance table on CH side — different
--               from PG; this query returns event payload inline**),
--               §5.2 (closed_at via JOIN ledgers if needed; not needed for
--               cursor here because ledger_sequence is in ORDER BY).
-- Notes:
--   • **CH-specific divergence:** The PG version of this endpoint reads
--     `soroban_events_appearances` (appearance-only) and overlays the
--     event payload from the Archive XDR at API layer (ADR 0033 / 0029).
--     CH-side, `soroban_events` is the full table — `topics_xdr`,
--     `data_xdr`, `event_type`, `signature` are all inlined. The CH API
--     handler can either:
--       (a) Project the inline XDR fields and let the consumer decode;
--       (b) Decode `topics_xdr` + `data_xdr` on the API side and surface
--           the decoded payload directly.
--     This is a meaningful win — CH-backed events endpoint doesn't need
--     Archive S3 fetches and avoids the per-ledger blob decode cost.
--   • Cursor drops `created_at` (§5.2). Order is fully determined by
--     `(ledger_sequence, transaction_id, event_index)` — adding
--     `event_index` to ORDER BY (and projecting it) lets the API page
--     across multi-event txs cleanly.

SELECT
    se.ledger_sequence,
    se.transaction_id,
    se.event_index,
    se.event_type,
    se.signature,
    se.topics_xdr,
    se.data_xdr,
    lower(hex(t.hash))                              AS transaction_hash_hex,
    t.successful
FROM soroban_events se FINAL
JOIN transactions t FINAL ON t.id = se.transaction_id AND t.ledger_sequence = se.ledger_sequence
WHERE
    se.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $1 LIMIT 1)
    -- Cursor includes event_index as the tiebreaker so multi-event txs
    -- page cleanly: ORDER BY uses all three columns, so the keyset
    -- predicate must too (otherwise a page boundary inside a multi-event
    -- tx would skip or repeat rows). Review feedback (Copilot PR #174).
    AND ($3 IS NULL OR (se.ledger_sequence, se.transaction_id, se.event_index) < ($3, $4, $5))
ORDER BY se.ledger_sequence DESC, se.transaction_id DESC, se.event_index DESC
LIMIT $2;
