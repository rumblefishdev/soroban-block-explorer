-- transitional.sql: tables that are being REPLACED by a parallel change
-- (`docs/deployment.md`) and still exist on production while the indexer
-- writes both the old and the new table. `init.sql` holds the end state; the
-- old definitions live here only so a fresh database (local docker, the
-- CH-gated tests) matches what the transitional writer expects. Applied after
-- `init.sql` by `apply_init_sql` and the `db-clickhouse-init` compose sidecar.
-- Each entry leaves with the PR that stops writing its table; between parallel
-- changes this file holds no statements.

----------------------------------------------------------------------
-- Task 0372 — replaced by `transaction_operations` / `pool_operation_amounts`
----------------------------------------------------------------------

-- `amount` is a **fold count** of identity-tuple duplicates (task 0163 /
-- ADR 0033 PG-side convention; CH inherits same semantic): the number of
-- on-chain operation envelope ops that collapsed into this single
-- appearance row by identity. NOT a stroop value or per-op amount. Real
-- per-op stroop values live in `result_meta_xdr` (Archive XDR overlay
-- on PG; read-time decode on CH via E03 statement C). API callers MUST
-- NOT interpret this column as a token amount.
CREATE TABLE IF NOT EXISTS operations_appearances (
    transaction_id    Int64,
    application_order Int16,
    type              Int16,
    source_id         Nullable(Int64),
    destination_id    Nullable(Int64),
    contract_id       Nullable(Int64),
    asset_code        LowCardinality(String),
    asset_issuer_id   Nullable(Int64),
    -- Crossed liquidity pools (task 0261/0268): single-element for LP
    -- deposit/withdraw, full crossed-pool list (result claim atoms) for
    -- path payments / offers, [] for no pool involvement (Array cannot be
    -- Nullable; has([], x) = 0 so empty arrays miss pool filters). Sorted +
    -- deduped by the stage fold. Filter with
    -- has(pool_ids, toFixedString(unhex(...), 32)).
    pool_ids          Array(FixedString(32)),
    amount            Int64,   -- fold count, see header comment
    ledger_sequence   Int64,
    -- Skip index for the `has(pool_ids, …)` pool filter (E20 /
    -- liquidity-pools/:id/transactions; task 0281 C). The read driver
    -- (fetch_pool_transactions) seeks via read-in-order `ORDER BY ledger DESC
    -- LIMIT`, so a POPULAR pool early-terminates near the tip; this bloom bounds
    -- the OTHER regime — a sparse pool whose last activity is far below the tip,
    -- where the driver must scan back to reach it. `bloom_filter(0.001)` (not the
    -- 0.025 default) keeps that scan's false-positive floor at ~0.1 % of the table
    -- (~6 M rows) instead of ~2.5 % (~155 M, box-measured 2026-06-17); same
    -- tight-FP rationale as the 0290 `idx_acc_id`.
    INDEX idx_oa_pool_ids pool_ids TYPE bloom_filter(0.001) GRANULARITY 1,
    -- Skip index for the contract-filtered transaction-list path (E03
    -- Statement B; task 0333). `contract_id` is NOT the ORDER BY prefix
    -- (unlike the `soroban_events` / `soroban_invocations_appearances` arms of
    -- the same UNION, which seek on `contract_id`), so this arm full-scanned.
    -- The read driver seeks via read-in-order `ORDER BY ledger DESC LIMIT`: a
    -- VERY active contract early-terminates near the tip (cheap), but a SPARSE
    -- contract — few/old appearances — forces a scan of the entire table to
    -- fill the page (box-measured: 42-appearance contract read 13.18 M / the
    -- whole table; this is the ~6.2 B-rows/query full scan that blew the prod
    -- `api_throttle.read_rows` quota on 2026-06-29, CH Code 201). This bloom
    -- bounds that sparse regime to the granules that actually hold the contract.
    -- `bloom_filter(0.001)` (not the 0.025 default) keeps the false-positive
    -- floor tight, same rationale as `idx_oa_pool_ids` / the 0290 `idx_acc_id`.
    -- contract_id is Nullable; `= <id>` never matches NULL rows, and granules
    -- holding only NULLs carry no value → skipped.
    INDEX idx_oa_contract_id contract_id TYPE bloom_filter(0.001) GRANULARITY 1
    -- idx_oa_asset_issuer_id (bloom on asset_issuer_id, was here for the E10
    -- asset-tx CLASSIC arm, task 0334) DROPPED 2026-07-13: task 0359 moved the
    -- asset-tx driver to the `operation_asset_appearances` fan-out seek, so no
    -- query filters `operations_appearances` by `asset_issuer_id` anymore — the
    -- bloom's sole consumer is gone (verified across api / audit-harness /
    -- backfill). Prod is an existing DB (this file is fresh-only): reclaim the
    -- ~97 MiB with `ALTER TABLE operations_appearances DROP INDEX idx_oa_asset_issuer_id`.
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (ledger_sequence, transaction_id, application_order);

-- lp_operation_amounts: see `pool_operation_amounts` in init.sql for the grain,
-- the sign and the two producers — the same rows, keyed by the
-- `transaction_id` surrogate and the 1-based operation index.
CREATE TABLE IF NOT EXISTS lp_operation_amounts (
    pool_id           FixedString(32),
    ledger_sequence   Int64,
    transaction_id    Int64,
    application_order Int16,
    asset_id          Int64,
    amount            Int64
)
ENGINE = ReplacingMergeTree
PARTITION BY intDiv(ledger_sequence, 500000)
ORDER BY (pool_id, ledger_sequence, transaction_id, application_order, asset_id);
