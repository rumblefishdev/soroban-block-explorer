-- Endpoint:     GET /contracts/:contract_id
-- Purpose:      Contract detail: header (deployer, WASM hash, type, SAC flag)
--               + lightweight stats (recent invocations and unique callers in
--               the last N days, NOT a full-history count). No name field —
--               the contract API surfaces none (task 0297 #3).
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.10
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037 + ADR 0042/0156
-- Data sources: DB-only.
-- Inputs:
--   $1  :contract_strkey  String   C-form contract ID
--   $2  :stats_window_days Int     stats window in days (e.g. 7)
-- Indexes:      soroban_contracts ORDER BY (id) — StrKey resolve walks FINAL.
--               soroban_invocations_appearances ORDER BY (contract_id,
--                 ledger_sequence, transaction_id) — contract-leading scan.
--               ledgers ORDER BY (sequence) — JOIN for closed_at filter.
-- CH Engine:    soroban_contracts — Replacing(wasm_uploaded_at_ledger) (FINAL).
--               soroban_invocations_appearances — Replacing partitioned (FINAL).
--               accounts — Replacing (FINAL).
--               ledgers — MergeTree (no FINAL).
-- CH Pattern:   2 statements; B uses uniqExact for distinct-caller count;
--               window filter applied via JOIN to ledgers (§5.2 closed_at
--               not on appearances table) + intDiv partition prune.
-- ADR 0044 §:   §4.2/§4.3 (Replacing partitioned), §4.5 (Replacing state),
--               §5.2 (closed_at via ledgers JOIN).
-- Notes:
--   • Two statements, mirroring PG E11.
--   • Statement A: same shape as PG, minus the legacy `metadata JSONB`
--     (gone post-ADR-0042/0156) and the typed `name` column — `sc.name` has no
--     writer since task 0297 and is not surfaced by contract detail. On-chain
--     token metadata lives in the `soroban_contract_metadata` side table and is
--     surfaced via /assets, not /contracts.
--   • Statement B: PG uses `created_at >= NOW() - INTERVAL`. CH-side
--     `soroban_invocations_appearances` has no closed_at (§5.2). We JOIN
--     `ledgers` on `ledger_sequence` and filter on `ledgers.closed_at`.
--     The window param is a day count (`$2 = 7`) interpreted as
--     `INTERVAL $2 DAY` to keep the CH grammar simple — PG uses an
--     INTERVAL literal but CH variants of `now() - INTERVAL $2 DAY`
--     accept the integer cleanly.
--   • `uniqExact` returns exact distinct count; `uniq` would be HLL-
--     approximate. For a stats card we want exact (small N expected),
--     and `uniqExact` is O(N) memory but bounded by the per-window result.

-- ============================================================================
-- A. Contract header.
--    `upgradeable` (task 0327) is resolved here, not in a separate query: a
--    LEFT JOIN to wasm_interface_metadata exposes the parsed mutability bit.
--    JSONHas distinguishes a frozen contract (key present, false) from a row
--    predating the flag (key absent → Unknown/null → no chip). SAC / no-WASM
--    rows have no metadata match and are mapped to Immutable in the handler.
-- ============================================================================
SELECT
    sc.id                                  AS contract_pk,
    sc.contract_id,
    lower(hex(sc.wasm_hash))               AS wasm_hash_hex,
    nullIf(sc.wasm_uploaded_at_ledger, 0)  AS wasm_uploaded_at_ledger,
    nullIf(deployer.account_id, '')        AS deployer,
    sc.deployed_at_ledger,
    sc.contract_type                   AS contract_type,
    sc.is_sac,
    -- NO `sc.name`: contract detail surfaces no name (task 0297 #3). The dead
    -- `soroban_contracts.name` is read only by the CH name-search substring
    -- fallback in the contracts LIST, never composed here.
    toUInt8(JSONHas(wim.metadata, 'upgradeable'))        AS upgradeable_has,
    toUInt8(JSONExtractBool(wim.metadata, 'upgradeable')) AS upgradeable_val
FROM soroban_contracts sc FINAL
LEFT JOIN accounts deployer ON deployer.id = sc.deployer_id
LEFT JOIN wasm_interface_metadata wim ON wim.wasm_hash = sc.wasm_hash
WHERE sc.contract_id = $1
LIMIT 1;

-- @@ split @@

-- ============================================================================
-- B. Recent-window stats.
--    Inputs: $1 = soroban_contracts.id (Int64, from A.contract_pk),
--            $2 = window in days (Int).
-- ============================================================================
SELECT
    count()                            AS recent_invocations,
    uniqExact(sia.caller_id)           AS recent_unique_callers,
    toInt32($2)                        AS stats_window_days
FROM soroban_invocations_appearances sia FINAL
JOIN ledgers l ON l.sequence = sia.ledger_sequence
WHERE sia.contract_id = $1
  AND l.closed_at >= now64() - INTERVAL $2 DAY;
