---
id: '0599'
title: 'Soroban pools missing from the registry'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0374']
tags: ['effort-medium', 'priority-medium']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-09-30
    status: backlog
    who: karolkow
    note: "Spawned from 0374 (decision 161 A): the devil's-advocate review of W1 found pool-shaped emitters outside the registry."
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Emitters identified as Sushi V3 pools; Comet added. Absorbs 0624, filed the same day as a duplicate.'
---

# Soroban pools missing from the registry

## Summary

About 44 contracts emit events that look like AMM pool swaps but are not in
`liquidity_pools` (kind 1), so the explorer lists none of their pools,
reserves or movements. Find which protocols they are and whether each is a
pool family we should register.

## Context

- Found 2026-09-30 while reviewing W1 (0374, PR #540), read-only, ledgers
  58M to the tip: 42 unregistered contracts emit concentrated-liquidity
  `[swap]` maps (`amount0`, `amount1`, `sqrt_price_x96`, `tick`; ~119k
  events), and 2 more emit `amount_in`, `amount_out`, `recipient` (~86k).
- Found 2026-09-30 during the W1 backfill: a Soroswap-shaped fork, first
  topic `RaumFiPair` and kinds `swap_pair_event`, `deposit_pair_event`,
  `withdraw_pair_event`, `sync_pair_event` (same map fields as
  `SoroswapPair`). Whole history: one contract,
  `CBXZK6363UPH444OD6RNC6ZQ7RAUMKGXTGO574WMYTOW5TSPL4UFULSN`, ledgers
  52,786,193–53,857,580, 171 swaps, 24 deposits, 21 withdrawals. It stages
  pool state, so the W1 decoder reads it and logs "unknown name"; registering
  it means adding `RaumFiPair` to the pair arm and re-running
  `soroban-pool-amounts`.
- Identified 2026-10-05 (read-only, production): the concentrated `[swap]`
  emitters are **Sushi V3** pools. Its factory `CD3KRKGD…GLYF` announces 58
  pools in `pool_created` events (fee, pool_address); 53 of them emit the
  `sqrt_price_x96` swap map since L58M; 0 of the 58 are in the registry. The
  router `CDMIM23W…ZCHL` emits the `amount_in` / `amount_out` / `recipient`
  map (47,409 swaps since L61,739,552) — a router, not a pool. Pools are
  upgraded through the factory (`pool_upgraded` ×57, `pool_migrated` ×3), so
  check every code version. Positions are NFTs (`SUSHI-V3-POS`).
- A second family the 44 above do not include: **Comet** (weighted pools).
  Factory `CA2LVIPU…TJKF`, topics `[LOG, NEW_POOL]` (caller, pool), 2 pools,
  0 in the registry. Pool `CAS3FL6T…VEAM` (Blend backstop) emits topics
  `[POOL, swap]`, 119,941 swaps from L51,499,922; the factory's two events
  are at L51,498,988 and L51,499,545 — check whether this pool is one of them.
- Reference only, not an oracle: an open-source Apache-2.0 DEX aggregator,
  https://github.com/Lum-Agg/stellar-dex-agg, decodes both families in
  `crates/dex-adapters/src/{sushi,comet,comet_math,clmm_math}.rs`. The chain
  settles every value.
- The registry knows three families (router, pair factory, config factory);
  a pool of any other family is invisible end to end.

## Implementation

- List the emitters with their event shapes, WASM hashes and deployers;
  group them into protocols.
- Per protocol: is it an AMM pool (reserves, liquidity providers) or a router
  or aggregator that only looks like one? Check its interface spec and a
  sample of its storage on chain.
- For each real pool family: registration source (factory events), reserve
  source (instance storage), and the W1 decoder arm — as the three existing
  families did.

## Acceptance Criteria

- [ ] Every pool-shaped emitter classified: registered family, new family, or
      not a pool (with the evidence).
- [ ] A decision per new family: register it (follow-up task) or not (reason).
- [ ] **Docs updated** — N/A until a family is added.
