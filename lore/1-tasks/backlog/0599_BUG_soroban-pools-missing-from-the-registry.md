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
