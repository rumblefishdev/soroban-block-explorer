---
id: '0517'
title: 'Event name is read from topics[0]; protocols that label there lose it'
type: FIX
status: backlog
related_adr: []
related_tasks: ['0516', '0518', '0008']
tags: [backend, xdr-parser, clickhouse, priority-high, effort-medium]
links: []
history:
  - date: '2026-08-26'
    status: backlog
    who: karolkow
    note: >
      Measured on production while scoping 0516. Correctness bug for all
      Soroban events, not only AMMs — filed separately so it is not gated on
      AMM work. Shape was documented in 0008 before the parser was written.
---

# Event name read from the wrong topic

## Summary

We take an event's name from `topics[0]`. Some protocols put a protocol label
there and the name in `topics[1]`. For those, `signature` lands `NULL` and the
event is unusable — silently, with no error anywhere.

## Measured on production, 2026-08-26

Chain-wide, 3 816 728 events carry a `NULL` signature — 0.04 % of 10.3 G. That
looks negligible until it is grouped by emitter:

| Emitter label   | Event                                                     | Rows    |
| --------------- | --------------------------------------------------------- | ------- |
| `SoroswapPair`  | `sync`                                                    | 572 576 |
| `SoroswapPair`  | `swap`                                                    | 570 858 |
| `SoroswapPair`  | `deposit`                                                 | 1 369   |
| `SoroswapPair`  | `withdraw`                                                | 348     |
| `SoroswapPair`  | `skim`                                                    | 23      |
| `DeFindexVault` | `deposit` / `withdraw` / `rebalance` / `rescue` / `dfees` | 674     |

**1 145 174 rows — 30 % of every undecoded event on the chain — are one
protocol.** The decoded topic vector shows why:

```
topics = [ String("SoroswapPair"), Symbol("sync") ]
              ^ we read the name here    ^ the name is here
```

`sync` carries the pool reserves after every trade. So an entire protocol's
reserve history is already on disk and unreadable.

## This was known

[0008](../archive/0008_RESEARCH_event-interpreter-patterns/notes/R-dex-swap-event-signatures.md)
records the shape from Soroswap's own source:

```rust
e.events().publish(("SoroswapPair", symbol_short!("swap")), event);
```

It also covers Phoenix. The research was archived and the parser was written
against the other convention anyway.

## Implementation

- Name resolution takes `topics[0]` when it is a `Symbol`; when it is not,
  fall through to `topics[1]` and keep `topics[0]` as the emitting protocol
  label. Both are worth storing — the label is a free protocol discriminator.
- Nothing may be dropped silently. An event whose name resolves nowhere keeps
  `NULL` **and** increments a monitored counter, so the next convention shows
  up as a number rather than as absence.
- Backfill is in-DB: `topics_xdr` already holds the decoded vector, so this is
  `INSERT … SELECT`, not an S3 re-parse.

## Acceptance Criteria

- [ ] Soroswap pair events carry names (`sync`, `swap`, `deposit`, `withdraw`,
      `skim`)
- [ ] DeFindex vault events carry names
- [ ] emitting protocol label preserved where present
- [ ] historical rows backfilled in-DB; no event with a resolvable name left
      `NULL`
- [ ] residual unresolved count exposed as a monitored metric, not a silence
- [ ] unit tests cover both topic conventions and an unknown third shape
