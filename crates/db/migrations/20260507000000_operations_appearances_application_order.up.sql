-- Task 0192 — re-introduce `application_order SMALLINT` on
-- `operations_appearances` so endpoint 03 Statement C can ORDER BY a
-- column that carries on-chain apply order, instead of relying on
-- `oa.id` BIGSERIAL (which task 0192 audit shows is not monotone with
-- apply order — the indexer's HashMap-keyed staging in
-- `crates/indexer/src/handler/persist/staging.rs` produces alphabetic-by-asset_code
-- INSERT order, not apply order).
--
-- Background: task 0163 dropped this column from the old `operations`
-- table on the premise that "no API endpoint reads application_order".
-- Endpoint 03 Statement C re-introduced an implicit ordering dependency
-- by using `ORDER BY oa.id` and documenting it as apply-order. This
-- migration makes the dependency explicit at the schema level.
--
-- NULLABLE for backward compatibility: existing rows stay NULL.
-- The forward-only indexer fix populates the column for new rows; an
-- audit (`audit-harness operations-order-diff`) drives the decision on
-- whether to backfill historical rows. NOT NULL transition is deferred.
--
-- Folding semantics: `operations_appearances` is an appearance index;
-- multiple envelope ops with identical natural identity collapse into
-- one row (the `amount` column counts the fold). For folded rows,
-- `application_order = MIN(operation_index across folded ops)` —
-- the position of the first occurrence in `tx.operations[]`. This
-- produces non-contiguous values for tx with folds; the API handler
-- maps result-set row-position 1..N for display.
--
-- Convention: 1-based per ecosystem parity (Horizon / stellar.expert /
-- task 0172 / ADR 0028). The indexer-supplied
-- `ExtractedOperation.operation_index` is already 1-based.
--
-- Partitioning: PG ≥ 14 cascades `ADD COLUMN` and `CHECK CONSTRAINT`
-- propagation to all attached child partitions atomically; no
-- per-partition loop needed.
--
-- No new index in this migration. The natural read pattern
-- (`WHERE transaction_id = X AND created_at = Y ORDER BY application_order`)
-- is served by the leftmost prefix of `uq_ops_app_identity`; the result
-- subset (≤100 ops/tx by Stellar protocol) sorts in-memory in <10 ms.
-- Add `idx_ops_app_tx_apporder (transaction_id, application_order)` later
-- if endpoint-03 P95 regresses.

ALTER TABLE operations_appearances
    ADD COLUMN IF NOT EXISTS application_order SMALLINT;

ALTER TABLE operations_appearances
    ADD CONSTRAINT ck_ops_app_application_order_range
    CHECK (application_order IS NULL OR (application_order BETWEEN 1 AND 32767));
