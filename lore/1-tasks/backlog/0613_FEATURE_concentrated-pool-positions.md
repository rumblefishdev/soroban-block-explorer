---
id: '0613'
title: 'Index liquidity positions of concentrated Soroban pools'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374', '0516', '0612']
tags: ['effort-large', 'priority-low', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from 0516 (the "Aquarius concentrated positions" item, 0374 deferred step 23): concentrated pools show their providers as not indexed.'
---

# Index liquidity positions of concentrated Soroban pools

## Summary

A concentrated pool issues no share token: each provider holds a position (a
price range and an amount of liquidity) stored inside the pool contract. The
participants list and count read share-token holders from `balances`, so
these pools answer `not_indexed` and the page says the providers are not
indexed. Index the positions, then list and count them.

## Context

Production, 2026-10-02: every Soroban pool without a share token
(`pool_instance_state.share_token_id = 0` on its latest row) is concentrated —
51 pools: 37 from the Aquarius router `CBQDHNBF…` (all of its concentrated
pools; the protocol's API reports `pool_type: concentrated` and the pool
address as its "share token", decimals 0) and 14 from the deployment
`CA7RQDMM…`, which `protocol_labels.rs` does not name and the Aquarius API
does not list. Example: `CD74JDJI…` (AQUA/sUSD), $6.6M volume in September.
All 2-leg pools with a share token and all 3- and 4-leg pools list their
providers today.

The 0374 worklog has the earlier findings: concentrated reserves ride the
pool instance after the legs (T4), and positions are not NFTs (2026-09-02
correction).

## Implementation Plan

- Identify `CA7RQDMM…` (protocol, router) and label it, or record why not.
- Index from the `position_update` events, as decided on 2026-08-27 (0374
  worklog); check them against the position storage in ledger entries (raw
  XDR is the arbiter). Table shape: one row per position change, located by
  its event, no `transaction_id`.
- Write path + backfill; read path for the participants list and count;
  remove the `not_indexed` answer for pools whose positions are indexed.

## Acceptance Criteria

- [ ] The participants list and count of every concentrated pool come from
      indexed positions; spot-checked against contract state on chain.
- [ ] `CA7RQDMM…` named or explained.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (participants), `docs/architecture/database-schema/**` (new table).
