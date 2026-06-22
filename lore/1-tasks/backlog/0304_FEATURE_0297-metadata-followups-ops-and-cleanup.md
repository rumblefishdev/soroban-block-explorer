---
id: '0304'
title: 'FEATURE: 0297 metadata follow-ups — backfill, deploy/flip, validation, frontend amounts, cleanup'
type: FEATURE
status: backlog
related_adr: ['0050']
related_tasks: ['0297', '0231', '0243']
tags:
  [
    clickhouse,
    soroban,
    ops,
    frontend,
    cleanup,
    validation,
    priority-medium,
    effort-large,
  ]
links: []
history:
  - date: 2026-06-18
    status: backlog
    who: karolkow
    note: >
      Spawned from 0297. 0297 ships the CODE (parser → soroban_contract_metadata
      side table → API read-compose; task 0297). This task carries everything
      that is NOT that core code implementation: backfill, deploy/flag-flip,
      perf validation, live tests, frontend amount rendering, and the legacy
      name-path cleanup that was too entangled to land safely inside 0297.
---

# 0297 metadata follow-ups (ops / validation / frontend / cleanup)

## Summary

[Task 0297](0297_FEATURE_contract-name-enrichment-and-bytes-decode/README.md)
implemented the on-chain Soroban token metadata pipeline in **code**
(`soroban_contract_metadata` side table, parser extract → indexer write → API
read-compose; task 0297).
This task bundles the remaining non-implementation work.

## Scope

### Backfill & data

- [ ] Backfill `soroban_contract_metadata` for existing contracts (table starts
      empty). Decide: re-parse historical ledgers vs RPC `getLedgerEntries` dump —
      archived/evicted instances need re-parse (RPC ~7-day retention).
- [ ] Direct created-vs-updated confirmation on a representative deploy via the
      galexie archive (RPC retention can't reach old deploys; see 0297 Option B).

### Validation & perf

- [ ] Validate read JOIN / `FINAL` / `argMax` cost on the CH snapshot for
      contract detail + asset detail/list before flipping the read flag
      (`read_rows` quota history).
- [ ] Live integration tests (real CH): metadata is written + read-composed.

### Deploy

- [ ] Flip the read path to surface metadata in prod (gated on perf + backfill).

### Frontend

- [ ] Render amounts using `decimals` (e.g. "1.5 USDC") across asset/amount views.
- [ ] Surface `symbol` / `name` where useful.

### List endpoints

- [ ] Contract LIST name-search still hits the dead `sc.name` column (empty →
      effectively contract_id-only). NOT repointed to the metadata side table:
      the contract API doesn't surface a name (0297 #3), so name-search is low
      value — decide drop-the-name-clause vs repoint. (The assets COALESCE
      already reads `m.name` from the side table.)

### Name-search / column-drop — BLOCKED on 0243 (search+assets → CH)

The legacy `name` columns (`soroban_contracts.name`, `assets.name`) have **no
writer since 0297** (empty going forward) but cannot be DROPped yet because the
new source (`soroban_contract_metadata`) is **CH-only** while these readers run
on **Postgres**:

- [ ] Repoint PG global-search label (`search/queries.rs` `COALESCE(sc.name,'')`)
      — blocked: `search` is PG-only (no CH variant); needs search ported to CH
      (task 0243) or it has no metadata source.
- [ ] Repoint PG assets list `a.name` (`assets/queries.rs`) — same blocker.
- [ ] Redefine PG `soroban_contracts.search_vector` (GENERATED from `name`, ADR 0042) so it no longer depends on the column.
- [ ] After backfill + read-flip + the above: `ALTER TABLE … DROP COLUMN name`
      on CH (`soroban_contracts`, `assets`) and the PG migration.

(CH-side: the assets COALESCE already reads `m.name` from the side table.
Contract name-search was NOT repointed — it stays on the dead `sc.name`
column, see the List-endpoints item above.)

### Cleanup (code)

- [x] Legacy `contract_name_writes` / `Symbol("name")` path fully un-threaded —
      done in 0297 PR (`extract_contract_data_name_writes`, deploy second pass,
      `ParseOutput.contract_name_writes`, PG `apply_contract_name_writes` + the
      `assets.name` mirror, all plumbing + tests). `ExtractedContractDeployment.name`
      kept (now always `None`) until the column drop above.

### Docs (ADR 0032)

- [ ] Finish sync: `backend/backend-overview.md`, and the reference SQL snapshots
      (`11_get_contracts_by_id`, `08`/`09_get_assets*`) in both `endpoint-queries`
      sets. (0297 did `database-schema-overview` + `xdr-parsing-overview`.)

## Acceptance Criteria

- [ ] Backfilled + read flag flipped in prod; perf validated; live tests green.
- [ ] Frontend renders amounts via `decimals`.
- [ ] Legacy name path removed; vestigial columns dropped.
- [ ] Docs fully synced per ADR 0032.
