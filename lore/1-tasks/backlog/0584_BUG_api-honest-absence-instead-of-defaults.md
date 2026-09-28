---
id: '0584'
title: 'BUG: API renders defaults (1970 dates, empty ids, Classic kind) where a lookup missed'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0374']
tags: [layer-api, priority-low, effort-small]
links: []
history:
  - date: '2026-09-25'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0374 (decision 127 A): the band-aid sweep of the API found
      small read-side defaults that turn a missed lookup into a plausible
      value. None is frequent; each is wrong when it fires.
  - date: '2026-09-28'
    status: backlog
    who: karolkow
    note: >
      Widened (0374 decision 92 B): the guessed 7 decimals for a soroban token
      that publishes none, on account balances, balance changes and asset
      supply, moves here from 0374 decision 119.
---

# BUG: API renders defaults where a lookup missed

## Summary

A few read paths fill a missed join or an unknown enum with a default that
looks like data. Carry the absence through (`Option`, `null`, "—") instead.

## Context

Found by the 0374 band-aid sweep, 2026-09-25 (read in code, not measured on
production):

- **Contract events without their transaction** — `contracts/queries.rs`
  (`txs.get(..).unwrap_or_default()`, ~line 1075): empty hash,
  `successful = false`, `created_at` 1970-01-01, which `ContractEvents.tsx`
  renders.
- **Unknown pool kind decoded as Classic** — `common/strkey.rs`
  (`decode_pool_kind`, ~line 91): would print a well-formed but wrong `L…`
  id. Cannot happen with today's data; should fail loudly.
- **Empty strings for "unknown"** — contract ids in
  `transactions/queries.rs` (~1002, ~1053) and the event type in
  `contracts/queries.rs` (~976).
- **Config-factory pool fee frozen at creation** — `liquidity_pools.fee_bps`
  is written once (`stage.rs`, config arm), while the contract can change it.
  Measure first whether any live fee differs from the stored one; only then
  decide between a versioned fee and nothing.
- **Guessed 7 decimals for a soroban token that publishes none** —
  `accounts/queries.rs` (`coalesce(m.decimals, 7)`, ~line 488), and the same
  default behind balance changes and asset supply. The amount is served raw
  with `decimals`, so a token with a different scale renders off by a power of
  ten (0374 notes a USST amount 10^11 too large). 82 tokens have no `decimals`
  in their newest metadata row (production, 2026-09-28). The pool reads
  already serve `null` for such a leg (`soroban_reserves::leg_reserves`, PR
  #518); these surfaces should do the same: `decimals: null`, amount shown as
  "—" or unscaled with a marker, never scaled by 7. Check task 0473 first — its
  patch for a third metadata layout may shrink the 82.

Out of scope: `join_use_nulls` for the API's read-only user (an operator
setting), merged accounts (0321).

## Acceptance Criteria

- [ ] Each read above returns `null`/`None` on a miss; the frontend shows "—"
- [ ] `decode_pool_kind` errors on an unknown kind instead of guessing Classic
- [ ] Config-factory fee drift measured on production; outcome recorded here
- [ ] No read scales a soroban token amount by an assumed 7; `decimals` is `null` where the token publishes none, and the frontend does not scale it
- [ ] API types regenerated; docs updated where a field became nullable
