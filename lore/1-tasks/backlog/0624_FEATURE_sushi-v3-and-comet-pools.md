---
id: '0624'
title: 'Index Sushi V3 and Comet pools — two Soroban AMM families the pool registry misses'
type: FEATURE
status: backlog
related_adr: ['0058']
related_tasks: ['0374', '0622', '0620']
tags: ['effort-medium', 'priority-medium', 'soroban', 'liquidity-pools']
links:
  - https://github.com/rumblefishdev/soroban-block-explorer/issues/405
  - https://github.com/Lum-Agg/stellar-dex-agg
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from 0374 — measured gap in the Soroban pool registry.'
---

# Index Sushi V3 and Comet pools

## Summary

The Soroban pool registry knows three families (router, pair-factory,
config-factory). Two live AMM families have a different shape and are not
registered at all: **Sushi V3** (concentrated liquidity) and **Comet**
(weighted pools). Their pools appear nowhere on `/liquidity-pools`.

## Stan teraz

- Done: the gap measured on production (below).
- Next: per-family measurement — registration events, storage layout,
  code versions — one family at a time (0374's depth-first rule).
- In force: one family at a time; shape-driven discovery, no address
  allowlist (ADR 0058).

## Context

Measured 2026-09-26 on production ClickHouse, read-only. A pool address is
"in the registry" when its 32-byte contract payload is a `pool_id` of
`liquidity_pools` with `pool_kind = 1`; the same check finds 3 of 3 sampled
`add_pool` pools.

| Family   | Registration (emitter, event)                                    | Pools | In registry | Activity                                                       |
| -------- | ---------------------------------------------------------------- | ----: | ----------: | -------------------------------------------------------------- |
| Sushi V3 | factory `CD3KRKGD…GLYF`, `pool_created` (fee, pool_address, …)   |    58 |           0 | router `CDMIM23W…ZCHL`: 47,409 `swap` events since L61,739,552 |
| Comet    | factory `CA2LVIPU…TJKF`, topics `[LOG, NEW_POOL]` (caller, pool) |     2 |           0 | pool `CAS3FL6T…VEAM` (Blend backstop): 119,941 `POOL`/`swap`   |

Registry total at the time: 774 Soroban pools. The two pools sampled for
recent activity emitted 14,313 events in the last ~30 days.

- Comet's factory event predates the pool it most visibly serves
  (`CAS3FL6T…` emits from L51,499,922, the factory's two `NEW_POOL` events
  are at L51,498,988 and L51,499,545) — check whether that pool is one of
  the two or was deployed another way.
- Sushi V3 pools are upgraded through the factory (`pool_upgraded` ×57,
  `pool_migrated` ×3, `wasm_approved` ×2) — the per-code-version check of
  0374 applies.
- Sushi V3 positions are NFTs (`SUSHI-V3-POS`, already decoded for metadata
  in `xdr-parser/src/token_metadata.rs`) — unlike Aquarius concentrated
  positions.

## Implementation

- Per family: registration arm (shape of the event, emitter authenticated as
  in ADR 0058), reserves source, total shares, legs.
- If 0622 lands first, reserves and shares come from the family's view
  functions through the 0620 executor, and only registration is new code.
- Reference only, not an oracle: an open-source Apache-2.0 aggregator
  (link in frontmatter) decodes both families in
  `crates/dex-adapters/src/{sushi,comet,comet_math,clmm_math}.rs`, with
  tests against on-chain values. The chain (`getLedgerEntries`, simulation)
  settles every value.

## Acceptance Criteria

- [ ] Every pool announced by each family's factory is in the registry,
      both directions, 0 missing / 0 extra (the 0374 closure check).
- [ ] Newest reserves equal the chain for every pool, per code version.
- [ ] Pools render on `/liquidity-pools` under the Soroban filter.
- [ ] **Docs updated** — `docs/architecture/**` pool families (ADR 0032).
