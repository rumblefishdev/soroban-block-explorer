-- ============================================================================
-- ⚠️  `contract_ids` REMOVED from the API (task 0386) — the embedded-tx rows
--     now carry `operation_types` only; the `contract_ids` projection is dead.
-- ⚠️  CH 26.3 CORRECTION (task 0243) — the embedded-transactions
--     `operation_types` projection must NOT use correlated
--     scalar subqueries (`… WHERE oa.transaction_id = t.id`): ClickHouse 26.3
--     rejects them with `Code: 48 NOT_IMPLEMENTED`. The live read path uses a
--     NON-correlated two-step — fetch the page of tx keys, then aggregate per
--     `(ledger_sequence, transaction_id) IN (…)` with `GROUP BY transaction_id`.
--     See `crates/api/src/common/ch.rs::fetch_tx_list_aggregates`.
-- ============================================================================
-- Endpoint:     GET /ledgers/:sequence
-- Purpose:      Ledger detail — header row + prev/next navigation + embedded
--               paginated transactions[] for the ledger.
-- Source:       backend-overview.md §6.3 / frontend-overview.md §6.6
-- Schema:       ADR 0044 (CH pilot), parallel to PG ADR 0037
-- Data sources: DB-only.
--               Statement A: ledgers header + prev_sequence + next_sequence.
--               Statement B: transactions in this ledger, partition-pruned
--                            via intDiv(ledger_sequence, 500000), keyset.
-- Inputs:
--   $1  :sequence    Int64       the ledger sequence to fetch
--   $2  :cursor_lseq Int64       optional keyset cursor `ledger_sequence`
--                                (equal to $1 for in-ledger paging)
--   $3  :cursor_id   Int64       optional keyset cursor `id`
--   $4  :limit       Int         page size (caller-side clamped, default 20)
-- Indexes:      ledgers PK (sequence) — main lookup + prev/next via scalar
--               subqueries (sparse PK in CH is index-only by default).
--               transactions PARTITION BY intDiv(ledger_sequence, 500000)
--               + ORDER BY (ledger_sequence, application_order, id) — the
--               `ledger_sequence = $1` predicate pinpoints a single
--               partition + sparse PK seek.
-- CH Engine:    ledgers — MergeTree (no FINAL).
--               transactions — ReplacingMergeTree (FINAL required).
--               accounts — ReplacingMergeTree(last_seen_ledger) (FINAL).
-- CH Pattern:   prev/next via scalar subqueries (no LATERAL needed);
--               partition prune via intDiv(ledger_sequence, 500000); FINAL
--               on Replacing reads.
-- ADR 0044 §:   §4.1 (ledgers MergeTree), §4.2 (transactions Replacing
--               partitioned), §4.5 (accounts Replacing state), §5.2
--               (closed_at native here; not needed via JOIN).
-- Notes:
--   • prev/next via two scalar subqueries — CH plans these as PK seeks
--     on the sparse index in opposite directions, same outcome as the
--     PG LATERAL pattern, simpler to read.
--   • Statement B cursor drops PG's `created_at` term (§5.2 — column
--     absent from CH transactions). The natural keyset for CH-side
--     transactions is `(ledger_sequence, id)` — equivalent semantics
--     because we're paginating within a single ledger, so ledger_sequence
--     is constant and the order is fully determined by `id`.
--     `application_order` (Int16) is also available but `id` is the
--     primary surrogate per CH ORDER BY.
--   • CH partition is one chunk per 500_000 ledgers; the equality on
--     `ledger_sequence = $1` reaches exactly one partition + one sparse
--     PK granule. Effectively index-only scan within that granule, on
--     par with PG's `created_at = $closed_at` partition prune.
--   • The original PG file carried `closed_at` from statement A into B
--     for partition pruning. CH partitions by ledger_sequence directly,
--     so we drop the carry-forward — statement B needs only $1.

-- =====================================================================
-- Statement A — ledger header + navigation
-- =====================================================================

SELECT
    l.sequence,
    lower(hex(l.hash))                                                                       AS hash_hex,
    l.closed_at,
    l.protocol_version,
    l.transaction_count,
    l.base_fee,
    (SELECT sequence FROM ledgers WHERE sequence < $1 ORDER BY sequence DESC LIMIT 1)        AS prev_sequence,
    (SELECT sequence FROM ledgers WHERE sequence > $1 ORDER BY sequence ASC  LIMIT 1)        AS next_sequence
FROM ledgers l
WHERE l.sequence = $1;

-- Note: CH 26.x does not support correlated subqueries with ORDER BY/LIMIT
-- referencing the outer row (NOT_IMPLEMENTED). Since `$1` is the bound input
-- and the outer WHERE pins `l.sequence = $1`, the prev/next subqueries can
-- reference `$1` directly — semantically identical to the PG LATERAL form,
-- only the syntax differs.

-- @@ split @@

-- =====================================================================
-- Statement B — embedded transactions[] for this ledger
--   • Partition-pruned via `ledger_sequence = $1` + intDiv() partition
--     scheme. One partition + one sparse-PK granule per ledger.
--   • Keyset cursor: `(ledger_sequence, id)` — see header note on
--     §5.2 simplification.
-- =====================================================================

SELECT
    t.id,
    lower(hex(t.hash))                                              AS hash_hex,
    t.ledger_sequence,
    a.account_id                                                    AS source_account,
    t.successful,
    t.fee_charged,
    t.operation_count
FROM transactions t FINAL
JOIN accounts a FINAL ON a.id = t.source_id
WHERE t.ledger_sequence = $1
  AND intDiv(t.ledger_sequence, 500000) = intDiv($1, 500000)        -- partition prune
  AND ($2 IS NULL OR (t.ledger_sequence, t.id) < ($2, $3))
ORDER BY t.ledger_sequence DESC, t.id DESC
LIMIT $4;

-- @@ split @@

-- =====================================================================
-- Statement C — successful-transaction count for this ledger (task 0445)
--   • Same statement as 04's Statement B, called with a one-element list.
--   • A round trip rather than a scalar subquery on Statement A: a subquery
--     returns 0 for a ledger with no `transactions` rows, which is
--     indistinguishable from "all of them failed". Absent here → NULL on
--     the wire → the UI renders the plain total instead of a split.
--   • Rationale in full: `fetch_successful_counts`, crates/api/src/ledgers/queries.rs
-- =====================================================================

SELECT
    ledger_sequence,
    countIf(successful) AS successful_count
FROM (
    SELECT ledger_sequence, application_order, successful
    FROM transactions
    WHERE ledger_sequence IN ($1)
      AND intDiv(ledger_sequence, 500000) IN ($2)
    LIMIT 1 BY ledger_sequence, application_order
)
GROUP BY ledger_sequence;
