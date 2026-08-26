---
id: '0518'
title: 'Soroswap pool adapter — reserves from sync, volume from swap'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0516', '0517', '0374', '0008']
tags:
  [backend, clickhouse, api, liquidity-pools, priority-medium, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: '2026-08-26'
    status: backlog
    who: karolkow
    note: >
      Second AMM adapter under 0516. HARD DEPENDENCY on 0517 — until event
      names resolve, every Soroswap event reads as NULL and nothing here is
      reachable.
---

# Soroswap pool adapter

## Summary

Index Soroswap pools — reserves, volume, positions — and union them into the
pool list under their own deployment label. Second adapter under
[0516](./0516_FEATURE_soroban-amm-coverage-umbrella.md).

## Blocked on 0517

Every Soroswap event currently has a `NULL` signature. Until
[0517](./0517_FIX_event-name-read-from-wrong-topic.md) lands there is nothing
to query. Do not start before it.

## What the store already holds

Once names resolve, 1 145 174 events become readable, including 572 576 `sync`
events. `sync` is the reserve announcement after every trade, so reserve
history needs no reconstruction — the same conclusion 0374 reached for Aquarius
by a different route.

## The four seams for this protocol

| Seam           | Soroswap                                                                                                                                   |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| discovery      | **no router registry** — pairs self-announce. Unlike Aquarius, this cannot be a registry read; settle factory-vs-shape with a measurement. |
| event name     | `topics[1]`, behind the `SoroswapPair` label                                                                                               |
| reserves       | `sync`                                                                                                                                     |
| position model | the pair contract **is** the LP token — mint/transfer/burn on the pair itself                                                              |

The position model is the cheap one here: LP tokens are already indexed as
assets, so holders come from `balances` with the usual dedup.

## Four-oracle table — fill before implementing

| #   | Oracle                                                       | Status                                                                                                                                                            |
| --- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Soroswap's own API                                           | **unknown — check first.** Their aggregator docs exist; a pool/pair endpoint has not been confirmed. If absent, say so: it removes the independent volume check.  |
| 2   | pair contract via RPC (`get_reserves`, `token_0`, `token_1`) | available                                                                                                                                                         |
| 3   | checkpoint snapshots                                         | available; the only historical oracle                                                                                                                             |
| 4   | independent aggregator                                       | **unreliable for this protocol** — one major aggregator reports it at protocol level with a per-pool feed that omits it entirely. Do not use as a coverage check. |

## Acceptance Criteria

- [ ] four-oracle table completed with evidence, absences stated
- [ ] deployments enumerated by shape and classified
- [ ] pools discovered, reconciled against an independent count
- [ ] reserves from `sync`, verified against `get_reserves()` on a sample
- [ ] volume from `swap`
- [ ] holders from `balances` on the pair asset, deduped
- [ ] unioned into the pool list under its own deployment label
- [ ] backfill in-DB, no S3 re-parse
