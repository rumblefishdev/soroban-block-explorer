---
id: '0281'
title: 'OPS: batched ClickHouse maintenance window — restart-gated + migration changes'
type: OPS
status: completed
related_adr: ['0044', '0047']
related_tasks: ['0221', '0243', '0261', '0266', '0268']
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
  - date: '2026-06-08'
    status: backlog
    who: claude
    note: >
      Spawned from 0243. Several CH changes are gated on a CH restart or pair
      with the indexer redeploy; live ledger ingestion forbids ad-hoc restarts.
      Collect them so they ship in ONE window (ingestion paused).
  - date: '2026-06-10'
    status: backlog
    who: stkrolikiewicz
    note: >
      0261 plan-audit deltas folded in: new pre-window gates section (fresh
      snapshot per the 0272 restore precedent; 0268 Phase 1 ALTERs — incl.
      the new liquidity_pool_snapshots.gross_volume_a column — pre-run ONLINE
      so the window only carries the writer switch + projection swap); 0221
      SAC→nfts_pending routing fix + drain re-run listed as a rider on the
      indexer redeploy.
  - date: '2026-06-17'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active for section C (read-path bounded seek). Sections A/B/D
      already applied on prod during the live 0266 window (0268 ADDs, oa_pool_seek
      + dead idx_oa_contract/idx_oa_type drops, idx_oa_pool_ids bloom skip index);
      init.sql carries the end-state. C box-validated 2026-06-17: the
      LP-transactions driver (has(pool_ids, X)) full-scanned 6.75B rows for a
      popular pool — the inner subquery had no ORDER BY/LIMIT and the bloom cannot
      prune a pool present in ~every granule. Fix needs no helper table /
      projection: push read-in-order `ORDER BY ledger DESC` + `LIMIT` into the
      inner so CH early-terminates — 6.75B -> 6.23M (~1000x) for the top pool.
      `LIMIT 1 BY` blocked optimize_read_in_order (1.13B); plain LIMIT works.
      Implemented in fetch_pool_transactions (queries_ch.rs) with an over-fetch
      factor (limit*4) for the outer tx-dedup. Box-validation also caught the
      SPARSE regime — a pool with fewer than over-fetch txs cannot early-terminate,
      so the driver scans back to its old activity; at the default 0.025 bloom
      that is ~155M rows (the 2.5 % FP floor). Tightened idx_oa_pool_ids to
      `bloom_filter(0.001)` (0290 precedent) -> 5.28M. Both regimes now ~5-8M.
      Net: read-in-order query change + index FP bump, no new table.
  - date: '2026-06-30'
    status: done
    who: stkrolikiewicz
    note: >
      Closeout. All A/B/C/D shipped. Pre-window gates + A/B (0268 ADD/MATERIALIZE,
      gross_volume_a, oa_pool_seek + dead-index drops, idx_oa_pool_ids bloom) applied
      on prod during the live 0266 window. C (read-path) merged to develop via PR #259
      (8c7193cc): fetch_pool_transactions now `has(pool_ids, toFixedString(unhex(?),32))`
      + inner read-in-order ORDER BY/LIMIT with limit*4 over-fetch and Rust (ls,tid)
      dedup; box-measured 6.75B->7.4M (~900x). Emerged in C review: outer GROUP-BY
      lost-tail data-loss bug — fixed with capped-page detection + single re-fetch at
      hard limit*128 bound. D: init.sql on develop carries the final shape (pool_ids
      Array, idx_oa_pool_ids bloom_filter(0.001), no idx_oa_contract/idx_oa_type, no
      stale oa_pool_seek). No follow-up backlog spawned.
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

### Pre-window gates (ONLINE, before pausing ingestion)

- [x] **Fresh snapshot** of the CH volume — mandatory gate. 0272
      precedent: Snapshot B RESTORE of 690 GiB took 642 s; cheap
      insurance, and a window has already failed once (0241 attempt 1).
- [x] **0268 Phase 1 ALTERs pre-run** (both online; the MATERIALIZE
      mutation runs for hours on 5.8B rows — do NOT spend window time
      on it): `operations_appearances.pool_ids` ADD + MATERIALIZE,
      and `liquidity_pool_snapshots.gross_volume_a` ADD (consumed by
      the 0266 backfill). The old indexer keeps writing meanwhile —
      INSERTs without the column fill `pool_ids` via the DEFAULT from
      the scalar.
- [x] Disk headroom check (`df -h /srv/clickhouse-data`).

### A. CH config — needs a restart to take effect

- [x] **`api_reader` → allow `force_optimize_projection`** (task 0243). Add a
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

- [x] **`operations_appearances.pool_id` (scalar `Nullable(FixedString(32))`)
      → `pool_ids Array(FixedString(32))`** for multi-hop path payments. Heavy
      column migration on a 6B+ row table + indexer write-path change (emit the
      full crossed-pool list). See 0268 for the migration plan. The heavy
      ADD + MATERIALIZE part pre-runs ONLINE (pre-window gates above); the
      window itself carries only the writer switch + projection swap (C).
- [x] **Indexer redeploy** — the path-payment pool-id corrections (0261/0266),
      the 0221 SAC→`nfts_pending` routing fix (+ post-deploy drain runbook
      re-run), and any other staged fixes ride this same window.

### C. 0243 read-path rework FORCED by B (do together with 0268)

The LP CH read path filters `operations_appearances.pool_id = unhex(X)`. After
0268 it must become `has(pool_ids, unhex(X))` — and the seek strategy changes:

- [x] **`fetch_pool_transactions` (`crates/api/src/liquidity_pools/queries_ch.rs`)**:
      `WHERE oa.pool_id = unhex(?)` → `WHERE has(oa.pool_ids, unhex(?))`.
- [x] **`oa_pool_seek` projection is invalidated** — `ORDER BY (pool_id, …)`
      cannot serve `has(pool_ids, X)` (array membership ≠ scalar prefix seek).
      Redesign the seek: e.g. a `bloom_filter`/`set` skip index on `pool_ids`, an
      `arrayJoin(pool_ids)`-backed projection, or a normalized
      `op_pool_appearances(pool_id, ledger_sequence, transaction_id)` helper
      table. Re-validate the read cost (the current bare-filter trick relied on
      the scalar projection auto-route).
- [x] Re-check the other LP endpoints + the global tx-list contract filter for
      any `operations_appearances.pool_id` references.

### D. Non-restart CH cleanups — batch here for convenience

- [x] **Drop the dead skip indexes** `idx_oa_contract` (bloom) + `idx_oa_type`
      (set) on `operations_appearances` — proven useless (0243 handoff: a hot
      contract's ops scatter across every granule; bloom skips nothing), only
      materialized on partition 125, waste insert-time work.
      `ALTER TABLE operations_appearances DROP INDEX idx_oa_contract; DROP INDEX
idx_oa_type;`
- [x] **Sync `init.sql`** so fresh CH instances match prod: add `PROJECTION
oa_pool_seek` + `SETTINGS deduplicate_merge_projection_mode = 'rebuild'`
      to the `operations_appearances` CREATE TABLE
      (`crates/db-clickhouse/schema/init.sql`). Prod already has the projection
      via ALTER (0243). NOTE: if B (0268) lands, the projection is replaced by
      whatever C chooses — sync that instead.

## Acceptance Criteria

- [x] Pre-window gates done (snapshot + 0268 Phase 1 ALTERs) before
      the ingestion pause.
- [x] All A/B items applied in a single ingestion-paused window.
- [x] C (0243 read-path) updated + box-validated against the post-0268 schema.
- [x] D cleanups applied; `init.sql` reflects the final operations_appearances
      shape (no drift vs prod).
- [x] LP transactions endpoint re-smoked on CH after the window (cost + parity).
