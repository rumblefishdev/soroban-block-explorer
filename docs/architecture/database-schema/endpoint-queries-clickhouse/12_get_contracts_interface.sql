-- Endpoint:     GET /contracts/:contract_id/interface
-- Purpose:      Contract interface: WASM interface metadata (function list
--               with parameter names + types) parsed at deploy time from
--               the WASM custom section.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.10
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
-- Inputs:
--   $1  :contract_strkey  String   C-form contract ID
-- Indexes:      soroban_contracts ORDER BY (id) — FINAL walk.
--               wasm_interface_metadata ORDER BY (wasm_hash) — direct seek.
-- CH Engine:    soroban_contracts — Replacing(wasm_uploaded_at_ledger) (FINAL).
--               wasm_interface_metadata — ReplacingMergeTree, no version
--                 (no FINAL — immutable per wasm_hash; was MergeTree, lore-0293).
-- CH Pattern:   Single statement; LEFT JOIN tolerates SAC contracts (NULL
--               wasm_hash).
-- ADR 0044 §:   §4.5 (soroban_contracts state), §4.8 (wasm_interface_metadata
--                 immutable lookup, plain MergeTree).
-- Notes:
--   • Identical shape to PG E12. `wasm_interface_metadata.metadata` is
--     stored as `String` in CH (CH `String` is just bytes — JSON content
--     is preserved verbatim, no validation). The API consumer treats it
--     as opaque JSON same as PG.
--   • SAC contracts have NULL `wasm_hash`; LEFT JOIN yields NULL metadata.
--   • Same schema-doc gap as PG E12: the JSON shape inside `metadata` is
--     not standardised in `docs/architecture/**`. Producing + documenting
--     the shape is the indexer's responsibility (follow-up).

SELECT
    sc.contract_id,
    lower(hex(sc.wasm_hash))                AS wasm_hash_hex,
    wim.metadata                            AS interface_metadata
FROM soroban_contracts sc FINAL
LEFT JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash
WHERE sc.contract_id = $1;
