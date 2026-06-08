---
id: '0281'
title: 'OPS: batched ClickHouse maintenance window — restart-gated + migration changes'
type: OPS
status: backlog
related_adr: ['0044', '0047']
related_tasks: ['0243', '0268']
tags:
  [
    priority-medium,
    effort-medium,
    clickhouse,
    ops,
    maintenance-window,
    milestone-2,
  ]
milestone: 2
links:
  - lore/1-tasks/backlog/0268_SCHEMA_pool-id-array-for-multi-hop-path-payments.md
history:
  - date: 2026-06-08
    status: backlog
    who: claude
    note: >
      Spawned from 0243. Several CH changes are gated on a CH restart or pair
      with the indexer redeploy; live ledger ingestion forbids ad-hoc restarts.
      Collect them so they ship in ONE window (ingestion paused).
---

# OPS: batched ClickHouse maintenance window

## Summary

A coordination checklist for the next ClickHouse maintenance window — the moment
ledger ingestion can be paused and the **indexer is redeployed**. Several CH
changes either require a CH restart or are coupled to the indexer/schema
migration; batching them avoids multiple ingestion interruptions.

## Why

Production CH ingests live, so a container restart (the only way to reload the
`:ro` single-file-bind-mounted `users.d`, and the safe way to apply a heavy
column migration) can't be done ad-hoc. These items wait for the window.

## Batched checklist

### A. CH config — needs a restart to take effect

- [ ] **`api_reader` → allow `force_optimize_projection`** (task 0243). Add a
      `changeable_in_readonly` constraint for _only_ that setting to the
      `read_only` profile (`crates/db-clickhouse/users.d/profiles.xml`), or set
      the profile to `readonly = 2`. `settings_constraints_replace_previous` is
      already `true` in config.xml. **Edit the host file in place (`cat > f`,
      NEVER `sed -i` — the mount binds the inode; `sed -i` de-syncs container
      from host) then restart the CH container.** Unblocks switching the LP
      `fetch_pool_transactions` driver from the current bare-filter seek (cheap
      read, but transfers the whole pool's op keys) to the forced
      `oa_pool_seek` projection (bounded ≤limit transfer). Constraint XML +
      rationale are in 0243 git history (commits e645701f reverted,
      8b76a17e bare-filter fallback).

### B. Schema migration — pairs with the indexer redeploy (task 0268)

- [ ] **`operations_appearances.pool_id` (scalar `Nullable(FixedString(32))`)
      → `pool_ids Array(FixedString(32))`** for multi-hop path payments. Heavy
      column migration on a 6B+ row table + indexer write-path change (emit the
      full crossed-pool list). See 0268 for the migration plan.
- [ ] **Indexer redeploy** — the path-payment pool-id corrections (0261/0266)
      and any other staged fixes ride this same window.

### C. 0243 read-path rework FORCED by B (do together with 0268)

The LP CH read path filters `operations_appearances.pool_id = unhex(X)`. After
0268 it must become `has(pool_ids, unhex(X))` — and the seek strategy changes:

- [ ] **`fetch_pool_transactions` (`crates/api/src/liquidity_pools/queries_ch.rs`)**:
      `WHERE oa.pool_id = unhex(?)` → `WHERE has(oa.pool_ids, unhex(?))`.
- [ ] **`oa_pool_seek` projection is invalidated** — `ORDER BY (pool_id, …)`
      cannot serve `has(pool_ids, X)` (array membership ≠ scalar prefix seek).
      Redesign the seek: e.g. a `bloom_filter`/`set` skip index on `pool_ids`, an
      `arrayJoin(pool_ids)`-backed projection, or a normalized
      `op_pool_appearances(pool_id, ledger_sequence, transaction_id)` helper
      table. Re-validate the read cost (the current bare-filter trick relied on
      the scalar projection auto-route).
- [ ] Re-check the other LP endpoints + the global tx-list contract filter for
      any `operations_appearances.pool_id` references.

### D. Non-restart CH cleanups — batch here for convenience

- [ ] **Drop the dead skip indexes** `idx_oa_contract` (bloom) + `idx_oa_type`
      (set) on `operations_appearances` — proven useless (0243 handoff: a hot
      contract's ops scatter across every granule; bloom skips nothing), only
      materialized on partition 125, waste insert-time work.
      `ALTER TABLE operations_appearances DROP INDEX idx_oa_contract; DROP INDEX
    idx_oa_type;`
- [ ] **Sync `init.sql`** so fresh CH instances match prod: add `PROJECTION
    oa_pool_seek` + `SETTINGS deduplicate_merge_projection_mode = 'rebuild'`
      to the `operations_appearances` CREATE TABLE
      (`crates/db-clickhouse/schema/init.sql`). Prod already has the projection
      via ALTER (0243). NOTE: if B (0268) lands, the projection is replaced by
      whatever C chooses — sync that instead.

## Acceptance Criteria

- [ ] All A/B items applied in a single ingestion-paused window.
- [ ] C (0243 read-path) updated + box-validated against the post-0268 schema.
- [ ] D cleanups applied; `init.sql` reflects the final operations_appearances
      shape (no drift vs prod).
- [ ] LP transactions endpoint re-smoked on CH after the window (cost + parity).
