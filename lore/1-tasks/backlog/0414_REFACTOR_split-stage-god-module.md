---
id: '0414'
title: 'REFACTOR: split the stage.rs ingest god-module into per-concern builders'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0393']
tags:
  [
    'db-clickhouse',
    'indexer',
    'refactor',
    'phase-future',
    'effort-large',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-07-20
    status: backlog
    who: karolkow
    note: 'Spawned from the 393/410 architecture audit (finding A1). Pre-existing (not caused by 393/410); whole-ingest scope.'
---

# REFACTOR: split the stage.rs ingest god-module into per-concern builders

## Summary

`crates/db-clickhouse/src/persist/stage.rs` is a **god-module** — one file, one
mega-function (`prepare_with_sac_overrides`) that stages EVERY ingest concern:
presence rows, NFT, liquidity pools, balances, SAC overrides, WASM-upgrade,
net-settled value. It is the largest source of change-amplification in the
pipeline: any new ingest feature bloats it further.

## Context

- **Evidence (architecture audit):** 2988 LOC; the cargo/module-dep graph shows it
  is a god-module by **SIZE / low cohesion**, NOT by coupling — its fan-in is low
  (nothing imports its submodules), so this is a "does too much internally" file,
  not a dependency hub. Splitting it is a mechanical decomposition, not a
  re-architecture.
- **Scope note:** this predates and is independent of task 0393/0410. The
  net-settled functions (`token_events_net_settled`, `event_asset_surrogate`, …)
  sit here **legitimately** — they resolve the DB surrogate via `persist::ids`, so
  they belong at the persistence boundary. They are NOT the problem; the problem is
  the seven unrelated concerns colocated in one function/file.

## Implementation

- Extract each concern into its own module + a `build_*_rows(input) -> Vec<Row>`
  function: e.g. `stage/presence.rs`, `stage/nft.rs`, `stage/balances.rs`,
  `stage/liquidity.rs`, `stage/sac.rs`, `stage/wasm_upgrade.rs`,
  `stage/net_settled.rs`. `prepare_with_sac_overrides` becomes a thin orchestrator
  that calls them and assembles the `StagedLedger`.
- Preserve behaviour exactly (this is a pure refactor — the existing e2e + unit
  tests are the safety net). Move-first, edit-never per commit discipline.

## Acceptance Criteria

- [ ] `prepare_with_sac_overrides` is a thin orchestrator; each concern lives in its
      own module with a `build_*_rows` entry point.
- [ ] No behaviour change — all existing db-clickhouse unit + e2e tests green.
- [ ] `stage.rs` (or `stage/mod.rs`) is materially smaller; no single file/function
      carries more than one ingest concern.
