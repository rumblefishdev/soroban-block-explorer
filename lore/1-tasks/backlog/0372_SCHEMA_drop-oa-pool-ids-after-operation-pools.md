---
id: '0372'
title: 'SCHEMA: drop pool_ids from operations_appearances once lptxs reads operation_pools (op→pool response field unused on frontend) — post-M3'
type: SCHEMA
status: backlog
related_adr: []
related_tasks: ['0365', '0268', '0261', '0281']
blocked_by: ['0365']
tags: [priority-low, effort-medium, layer-clickhouse, milestone-4]
milestone: 4
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/transactions/queries.rs
  - crates/api/src/transactions/dto.rs
  - crates/db-clickhouse/src/persist/stage.rs
history:
  - date: 2026-07-09
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0365. Once lptxs seeks `operation_pools`, the `pool_ids` array on
      `operations_appearances` serves only the op→pool direction — the `pool_ids`
      field on the transactions response — which the `web/` frontend does NOT consume
      (verified: `operationEntries.ts:56` stubs `[]`; grep finds no `.pool_ids` read).
      Dropping the column reclaims more disk than `operation_pools` costs (net smaller)
      and removes dead ingestion work. Sequence POST-M3, strictly after 0365 ships.
---

# SCHEMA: drop pool_ids from operations_appearances (post-0365)

## Summary

`pool_ids Array(FixedString(32))` on `operations_appearances` was added by 0261/0268
to attribute multi-hop path-payment pool crossings to the LP transactions endpoint
(pool→op). Once **0365** moves that filter to `operation_pools`, the array's only
remaining reader is the **op→pool** direction — the `pool_ids` field on the
transactions response — which the frontend does not use. Drop the column (and its
bloom) to reclaim space and stop computing a dead field.

## Context

- **Pool→op** (the reason the array exists) moves to `operation_pools` in 0365.
- **Op→pool** is the only leftover: `transactions/queries.rs:870`
  (`arrayMap(x -> lower(hex(x)), oa.pool_ids)`) → `transactions/dto.rs:182`
  (`pub pool_ids: Vec<String>`). The `web/` frontend does **not** read it —
  `operationEntries.ts:56` stubs `pool_ids: []`; a repo grep finds no `.pool_ids`
  access. Populated-but-unused response field.
- **Storage:** the `pool_ids` column is ~63 GB uncompressed (measured — a full
  column scan processed 63.39 GB over 6.41B rows), a few GB compressed. Dropping it
  frees more than `operation_pools` (~2 GB) adds → net smaller on disk.

## Plan

1. **Confirm external consumers** of the `pool_ids` response field (public API /
   OpenAPI). Decide: (a) drop the field — deprecate + **regenerate API types**
   (`crates/api/**` → CI `API types freshness` gate), or (b) keep serving `[]` from
   a trivial constant. Do NOT drop the wire field on unverified external use.
2. **Ingestion** — keep computing pool crossings in `stage::prepare` (0365's
   `operation_pools` still needs them); stop writing them into the oa row.
3. **Schema migration** — `DROP COLUMN pool_ids` + drop `idx_oa_pool_ids` on prod
   `operations_appearances` (6.41B rows), coordinated in a deploy window. Mirror the
   0268 Phase-3 playbook (`REMOVE DEFAULT` first if a DEFAULT exists, gate on E20
   green). Update `init.sql`.
4. **Readers** — remove the `arrayMap` in `transactions/queries.rs`; handle the DTO
   field per step 1.

## Acceptance Criteria

- [ ] `operations_appearances` has no `pool_ids` column; `idx_oa_pool_ids` gone;
      `init.sql` updated.
- [ ] lptxs unaffected (reads `operation_pools`, 0365); E20 green.
- [ ] Transactions endpoint: `pool_ids` field dropped (API types regenerated) OR
      served as `[]`; no external consumer broken.
- [ ] **Docs updated** — schema + ingestion pages under `docs/architecture/**`
      per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — required IF the response field is dropped.

## Notes

- **Hard dependency on 0365** (`blocked_by`) — cannot drop until lptxs no longer
  reads `oa.pool_ids`. Sequenced **post-M3** per direction.
- Template: 0268 Phase 3 dropped the legacy scalar `pool_id` the same way
  (`REMOVE DEFAULT` → `DROP COLUMN`, gated on E20).
