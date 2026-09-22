-- ============================================================================
-- ✅ CH STATUS (task 0243) — MIGRATED. `list_events` dispatches PG/CH via
--    `DataSource::for_module(Module::Contracts)`; CH path =
--    `contracts/queries_ch::fetch_events`. CH `soroban_events` stores the ScVal
--    payload pre-decoded to JSON at ingest (column names `topics_xdr`/`data_xdr`
--    are a misnomer; diagnostic-source events are also dropped at ingest), so
--    the read path just JSON-deserializes inline — no Archive overlay, no
--    read-time XDR decode. Keyset is 3-component
--    `(ledger_sequence, transaction_index, operation_index, event_index)` —
--    the stellar-rpc event id (ADR 0059). Divergence from PG: CH
--    pages per EVENT (one row → one item, `data.len() <= limit`) vs PG's folded
--    appearance (expands to many). The PG fold-count is an internal storage
--    detail and is not surfaced on the wire.
-- ============================================================================
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
--   $3  :cursor_ledger              Int64    NULL on first page
--   $4  :cursor_transaction_index  UInt32   NULL on first page
--   $5  :cursor_operation_index    UInt16   NULL on first page
--   $6  :cursor_event_index        UInt32   NULL on first page
-- Indexes:      soroban_contracts ORDER BY (id) — leading resolve.
--               soroban_events ORDER BY (contract_id, ledger_sequence,
--                 transaction_index, operation_index, event_index) — the
--                 stellar-rpc event id (ADR 0059) — + PARTITION BY intDiv.
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
--   • Live read (task 0381): no FINAL; the page's keys are picked first with
--     `LIMIT 1 BY` on the key columns alone, and `topics_xdr` / `data_xdr`
--     read only for those keys. `LIMIT 1 BY` walks every row it dedups, so
--     beside the payload it read 1.2 GiB for the native SAC's first page
--     (0.6 GiB this way, 2026-09-22).
--   • Cursor drops `created_at` (§5.2). Order is fully determined by
--     the rpc event id, so a page boundary inside a transaction (or inside
--     one operation) is exact, and the order the rows come back in is
--     execution order: fee charge, operation events, fee refund.

SELECT
    se.ledger_sequence,
    se.transaction_index,
    se.operation_index,
    se.event_index,
    se.application_order,
    se.event_type,
    se.signature,
    se.topics_xdr,
    se.data_xdr,
    lower(hex(t.hash))                              AS transaction_hash_hex,
    t.successful
FROM soroban_events se FINAL
JOIN transactions t FINAL
  ON t.ledger_sequence = se.ledger_sequence AND t.application_order = se.application_order
WHERE
    se.contract_id = (SELECT id FROM soroban_contracts FINAL WHERE contract_id = $1 LIMIT 1)
    -- The keyset is the whole rpc id: ORDER BY uses all four columns, so the
    -- predicate must too, or a page boundary inside a transaction would skip
    -- or repeat rows.
    AND ($3 IS NULL OR (se.ledger_sequence, se.transaction_index, se.operation_index, se.event_index)
                     < ($3, $4, $5, $6))
ORDER BY se.ledger_sequence DESC, se.transaction_index DESC,
         se.operation_index DESC, se.event_index DESC
LIMIT $2;

-- The transaction joins by POSITION, never by `transaction_id`: a fee refund's
-- id carries the end-of-ledger sentinel (transaction 1048575), so only
-- `application_order` says which transaction the event belongs to (ADR 0059).
