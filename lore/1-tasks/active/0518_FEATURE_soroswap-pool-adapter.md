---
id: '0518'
title: 'Soroswap pool adapter — reserves from sync, volume from swap'
type: FEATURE
status: active
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
  - date: '2026-09-02'
    status: active
    who: karolkow
    note: >
      Activated after 0517's rule landed (PR #443) and the pre-adapter
      probes settled every architecture seam. Ordering re-reversed to
      Soroswap-first on the per-swap re-measurement (see 0516).
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

## Pre-adapter probes (2026-09-02) — the decisive seams are already settled

Probed live before any code, after the 0517 rule made the events readable:

1. **Reserves live in the pair's OWN instance storage** — read verbatim from
   mainnet (`stellar contract read` on pair `CAM7DY53…`, the native-USDC
   pair): Soroswap's `DataKey` is a plain enum, so instance keys are u32
   DISCRIMINANTS, not symbols: `0` = token0 address, `1` = token1 address,
   `2`/`3` = Reserve0/Reserve1 as `i128`, `4` = the FACTORY address
   (`CA4HEQTL…7AW2` — exactly the archived factory), plus SEP-41
   `METADATA`/`TotalSupply`/`name`/`symbol` — the pair IS the LP token, with
   on-chain metadata ("native-USDC Soroswap LP Token").
   **Consequences:** (a) the reserve source is self-authenticated ledger
   STATE (owner == pool) — the ADR 0058 doctrine fits with no new tables;
   `sync` demotes to the monitored cross-check, exactly `update_reserves`'
   fate; (b) the pair's own factory pointer gives the registration
   corroboration the same class of authenticity as Aquarius's `Router` key;
   (c) the parser needs a u32-discriminant reader (not symbol-keyed) — the
   discriminants must be anchored in vendor source per deployment WASM, per
   the umbrella's shape-not-name rule.
2. **True-swap flow re-measured — Soroswap first** (recorded in 0516):
   48,334 vs Phoenix 5,526 per 1M fresh ledgers (8.7x), both eras ahead.
3. **`new_pair` shape fetched from current vendor source** (factory
   `event.rs`): topics `[String("SoroswapFactory"), Symbol("new_pair")]`
   (0517 arm 2 → signature `new_pair`), data
   `{token_0, token_1, pair, new_pairs_length}` — the registration carries
   the pair address in DATA from an authenticated emitter, and
   `new_pairs_length` is a free monotone counter for the discovery
   closure check. Discovery counts to reconcile: 191 label-emitting pairs
   (measured, = the 0516 "trading" count) vs 253 `…Soroswap…` metadata
   names (drift from 248) vs factory `all_pairs_length` (RPC).
4. **Vendor API (oracle #1) EXISTS but is keyed**: `api.soroswap.finance`
   is live (NestJS), `/docs` serves a Swagger UI, and `/pools` answers
   **403 Forbidden** — the endpoint exists behind an API key. Scope line
   for the four-oracle table: oracle #1 available IF a key is obtained;
   otherwise the independent volume check falls to the #2/#3 oracles and
   must be stated as the 0516 scope risk.

## In-DB completeness PROVEN + decisions (karolkow, 2026-09-02)

Measured before implementation, settling the backfill question with evidence:

- **Four factory deployments** share the `SoroswapFactory` label: the
  documented `CA4HEQTL…7AW2` (214 pairs, counter 1→214 GAPLESS,
  50.75M→63.8M) plus three dead early deployments (11/6/4 pairs, eras
  50.69-50.75M, each counter complete 1→N) — the Aquarius ten-routers story
  again. 235/235 registrations, 235 distinct pairs, no pair registered
  twice.
- **`min(new_pairs_length) = 1` per factory** — every factory's FIRST pair
  postdates the ingest floor; no pre-floor gap exists for this protocol.
- **Orphan emitters: 0** — every `SoroswapPair`-labelled emitter in history
  is in the registered set.
- Therefore: registry (identity + legs from `new_pair` data), reserve
  history (`sync`, absolute values) and volume (`swap`) are ALL fully
  derivable in-DB. **Decision 63: backfill is in-DB (option A)**, and the
  closure check comes free from the vendor's own monotone counter:
  `max(new_pairs_length) == count()` per factory — stronger than set
  reconciliation.
- **Decision 64: `pool_type_raw` stays `''`** — the vendor emits no type
  (one fixed constant-product mode); an invented label would be our
  interpretation, not a verbatim value.
- Reserve SOURCE per the probes above: live = the pair's own instance
  storage (self-authenticated; `plane_id` = the pair's own id — stamp,
  declaration and owner coincide); history = `sync`-derived rows; the
  live/history seam is verified by the bidirectional anti-test on the
  overlap. `sync` thereafter stays the monitored cross-check.
- Minor, recorded: current `total_shares` for DORMANT pairs is the one
  value not in the DB — the live writer fills it on a pair's next
  activity; an optional one-shot RPC pass covers the rest.

## Acceptance Criteria

- [ ] four-oracle table completed with evidence, absences stated
- [ ] deployments enumerated by shape and classified
- [ ] pools discovered, reconciled against an independent count
- [ ] reserves from `sync`, verified against `get_reserves()` on a sample
- [ ] volume from `swap`
- [ ] holders from `balances` on the pair asset, deduped
- [ ] unioned into the pool list under its own deployment label
- [ ] backfill in-DB, no S3 re-parse
