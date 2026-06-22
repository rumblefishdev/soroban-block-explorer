---
id: '0304'
title: 'FEATURE: 0297 metadata follow-ups — backfill, deploy/flip, validation, frontend amounts, cleanup'
type: FEATURE
status: backlog
related_adr: ['0049']
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
      side table → API read-compose; ADR 0049). This task carries everything
      that is NOT that core code implementation: backfill, deploy/flag-flip,
      perf validation, live tests, frontend amount rendering, and the legacy
      name-path cleanup that was too entangled to land safely inside 0297.
---

# 0297 metadata follow-ups (ops / validation / frontend / cleanup)

## Summary

[Task 0297](0297_FEATURE_contract-name-enrichment-and-bytes-decode/README.md)
implemented the on-chain Soroban token metadata pipeline in **code**
(`soroban_contract_metadata` side table, parser extract → indexer write → API
read-compose; [ADR 0049](../../2-adrs/0049_soroban-contract-metadata-onchain-side-table.md)).
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

### List endpoints (perf-deferred)

- [ ] Contract LIST read-compose (`name`) — deferred behind JOIN-perf validation
      (asset list already done in 0297).

### Cleanup (code — deferred from 0297 due to entanglement)

- [ ] Fully un-thread the legacy `contract_name_writes` path:
      `extract_contract_data_name_writes`, the deploy-time `Symbol("name")` second
      pass + `ExtractedContractDeployment.name` / `detect_assets`, PG
      `apply_contract_name_writes`, plumbing + tests (~20 sites, ~8 files). 0297
      removed only the obsolete CH tripwire + dead loop.
- [ ] DROP vestigial `soroban_contracts.name` / `assets.name` once read-compose
      proven in prod.

### Docs (ADR 0032)

- [ ] Finish sync: `backend/backend-overview.md`, and the reference SQL snapshots
      (`11_get_contracts_by_id`, `08`/`09_get_assets*`) in both `endpoint-queries`
      sets. (0297 did `database-schema-overview` + `xdr-parsing-overview`.)

## Acceptance Criteria

- [ ] Backfilled + read flag flipped in prod; perf validated; live tests green.
- [ ] Frontend renders amounts via `decimals`.
- [ ] Legacy name path removed; vestigial columns dropped.
- [ ] Docs fully synced per ADR 0032.
