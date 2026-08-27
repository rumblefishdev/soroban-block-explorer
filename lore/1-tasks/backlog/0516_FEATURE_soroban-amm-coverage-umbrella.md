---
id: '0516'
title: 'Soroban AMM coverage: shared pool model + four-oracle validation method'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374', '0517', '0518', '0008']
tags:
  [
    backend,
    clickhouse,
    api,
    liquidity-pools,
    umbrella,
    priority-medium,
    effort-large,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: '2026-08-26'
    status: backlog
    who: karolkow
    note: >
      Umbrella for multi-protocol AMM coverage. Owns the shared model and the
      validation method so each adapter is small; 0374 (Aquarius) is the first
      adapter and predates it. Prior art in archived 0008.
---

# Soroban AMM coverage — umbrella

## Summary

One pool model and one validation method for every Soroban AMM we index, so
that adding protocol N+1 is an adapter and not another redesign. This task owns
what is shared; each protocol gets its own task for what is not.

## Context

`/liquidity-pools` is fed by the classic protocol only. Soroban AMMs are a
separate world with their own pool contracts, and there are three that matter.
That list is not our guess — the Soroswap aggregator, whose business is to know
every Stellar AMM, integrates exactly those three. Measured 2026-08-27:

| Family                                   |                     Contracts |   Swap events |    Share |
| ---------------------------------------- | ----------------------------: | ------------: | -------: |
| Router-registry family (Aquarius-shaped) | 371 trading of 497 registered |     4 363 284 |     63 % |
| **Phoenix**                              |                        **14** | **1 969 860** | **29 %** |
| Soroswap                                 |      191 trading of 232 pairs |       578 921 |      8 % |

**Phoenix is second, not third** — 3.4x Soroswap's flow across 14 contracts
rather than 232. An earlier estimate put Soroswap second; it was measured only
over contracts carrying Soroswap metadata and missed Phoenix entirely, because
Phoenix's events decode to nothing (see 0517) and so were invisible to a
signature-based count.

**Read [0008](../archive/0008_RESEARCH_event-interpreter-patterns/README.md)
before starting.** It already documents Soroswap's and Phoenix's event shapes
from their own source, and a pattern-registry design. That research was
archived and never reached the parser — the same event shape was rediscovered
from raw data on 2026-08-26. Preventing that second discovery is half the point
of this task.

## The four-oracle rule

Run this per protocol. Record every line as present or absent — never blank.

| #   | Oracle                          | Validates                                | Covers history? |
| --- | ------------------------------- | ---------------------------------------- | --------------- |
| 1   | the protocol's own API          | discovery, pool type, fee, volume        | no              |
| 2   | the contract via RPC simulation | current state; never our decode path     | no              |
| 3   | checkpoint snapshots            | **the only oracle for historical state** | yes             |
| 4   | an independent aggregator       | coarse protocol-level sanity             | no              |

Two rules learned from the Aquarius pass:

- **A protocol with no #1 leaves only #2 and #3** — current state and history,
  no independent volume check. State that in the adapter task as a scope risk.
- **The vendor's API defines the vendor's scope.** Measured: the Aquarius API
  lists one router's pools and has zero overlap with the nine other routers
  emitting the same events. Do not attach a brand to pools the vendor's own
  catalogue excludes.

`stellar.expert` is not an oracle here — its pool API returns classic pools
only. An aggregator's per-pool feed may omit a protocol entirely even while
reporting it at protocol level; check before relying on it.

## Anchor every decoder in the vendor's own source — and record what you could not reach

Soroban defines no AMM standard, so there is no specification to decode
against. That makes the vendor's own source the highest authority available,
and observation the arbiter of last resort. Before writing any decoder:

1. **The vendor's contract source.** Their event-emitting code states the
   payload exactly. Archive the snippet into the task — repositories go
   private (Aquarius's did, between one research pass and the next).
2. **The deployed contract's own spec, pulled from chain.** Live, signed by
   deployment, and independent of any repository:
   `stellar contract info interface --id <contract> --rpc-url …`.
   **It carries function and type definitions but no event definitions**, so
   it can corroborate types and lookup keys and can never confirm an event's
   shape. Use it for what it covers, and say so.
3. **Structural conformance across all history.** Count how many real events
   match the decoder's checks. Anything short of 100 % is a finding.

**Record which of the three you actually reached.** "Verified against the
vendor's source" and "verified against a five-month-old capture that the
vendor has since taken private" are different claims, and the second one is
what we usually have.

Expect vocabularies to disagree between sources rather than to align. One pool
type carried three spellings at once: `constant` in the router's event,
`standard` in pool state, `ConstantProduct` in the contract's own enum — and a
fourth shape (`elastic`) exists on chain that appears in none of them, because
it belongs to a different deployment's code.

## Event names collide across protocols — the name is never the identifier

`withdraw_liquidity`, `provide_liquidity` and `swap` are each emitted by more
than one unrelated protocol. A decoder keyed on an event name will claim
another protocol's events. Key on the **full shape** — topic types, data types,
arity — and measure the false-positive rate against all history before
trusting it.

Three layers keep a decoder honest, and all three are cheap:

1. **Shape, not name.** A specific topic/data layout is a far narrower sieve.
2. **Behaviour confirms a claim.** A registry entry is a _candidate_; the pool
   is real once it emits pool activity. Measured on the router family: 23
   registrations from five dead deployments never emitted anything at all.
3. **Unknowns are counted, never guessed.** An unrecognised value yields
   `None` plus the raw string plus a monitored counter.

## Deployment is the unit, not the brand

Follow the convention DEX front-ends already use, where a protocol appears as
its distinct deployments rather than one merged name. Each router/factory
deployment is its own entry in the protocol dimension and its own filter value.

This is not cosmetic. It bounds implementation scope to the deployment the
vendor documents — which is exactly the scope their API can validate — and it
keeps unidentified deployments from being labelled with someone else's brand.
Deployments with no live pools are indexed for historical completeness and get
no listing entry.

## Shared model — built once, here

1. **Pool identity** widens to a contract address; the classic `L…` strkey
   stays valid. One id column, two shapes.
2. **Legs are an ordered list**, not `asset_a` / `asset_b`. Three-leg pools
   already exist.
3. **Amounts are raw `Int128` plus a decimals column.** An 18-decimal leg is
   already on chain and the classic scale would corrupt it. A leg whose
   decimals cannot be resolved renders with an explicit marker, never a
   plausible wrong number.
4. **Protocol + deployment** as a first-class dimension on every pool row.
5. **Position model as an enum** — fungible share token, ranged position, or
   trustline. Not every protocol has fungible shares; a pool whose positions we
   cannot enumerate must say "not indexed", never show an empty holder list.

## The four seams — all that an adapter may vary

Everything else is shared. If a protocol needs a fifth seam, that is a signal
to change the model here rather than to special-case it in the adapter.

| Seam                       | Known variants                                                 |
| -------------------------- | -------------------------------------------------------------- |
| discovery                  | registry event on a router · self-announcing pair contract     |
| where the event name lives | `topics[0]` · `topics[1]` behind a protocol label              |
| reserve source             | dedicated event · contract state entry                         |
| position model             | separate share token · ranged position · pool is its own token |

## Per-protocol adapter checklist

Copy into each adapter task:

- [ ] four-oracle table filled in, each line present or absent
- [ ] deployments enumerated by shape, each classified with on-chain evidence
- [ ] scope fixed to one deployment, named after it
- [ ] the four seams answered
- [ ] discovery reconciled against an independent count, not against the
      registry that produced it
- [ ] reserves compared to the contract's own answer, across every pool type
- [ ] positions: coverage measured, unresolvable cases render "not indexed"
- [ ] backfill is in-DB where the raw columns already hold the data

## Adapters

| Protocol               | Task                                                                         | State                                      |
| ---------------------- | ---------------------------------------------------------------------------- | ------------------------------------------ |
| Router-registry family | [0374](../active/0374_FEATURE_lp-native-leg-and-soroban-amm-completeness.md) | active, first adapter                      |
| **Phoenix**            | —                                                                            | **next after 0374**; spawn when 0517 lands |
| Soroswap               | [0518](./0518_FEATURE_soroswap-pool-adapter.md)                              | after Phoenix; blocked on 0517             |

**Order reversed 2026-08-27 on measurement.** Phoenix carries 3.4x the swap
flow of Soroswap across 14 contracts instead of 232 — more of the market for
less of the work, and both are unblocked by the same fix (0517). Nothing about
Soroswap changed; it simply is not second.

## Acceptance Criteria

- [ ] shared model items 1–5 implemented and used by at least two adapters
- [ ] four-oracle table filled in for every adapter task
- [ ] protocol + deployment filter on the pool list
- [ ] adding an adapter touches no shared table shape
