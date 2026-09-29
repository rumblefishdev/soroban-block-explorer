---
id: '0584'
title: 'BUG: API renders defaults (1970 dates, empty ids, Classic kind) where a lookup missed'
type: BUG
status: active
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
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Activated. Widened (decision 3 A): the API reads
      soroban_contract_metadata 13 times in 7 files with three dedup styles;
      one shared definition of the newest metadata row replaces them.
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
  patch for a third metadata layout may shrink the 82. The shared resolver
  (`common/asset_identity.rs`, ~lines 190–199) also reads
  `argMax(decimals, version)`, which skips a `NULL` argument and so returns an
  older version's decimals when the newest row has none; the pool read fixed
  the same thing with `argMax(tuple(decimals), version).1` (PR #518).

Out of scope: `join_use_nulls` for the API's read-only user (an operator
setting), merged accounts (0321).

## Acceptance Criteria

- [ ] Each read above returns `null`/`None` on a miss; the frontend shows "—"
- [ ] `decode_pool_kind` errors on an unknown kind instead of guessing Classic
- [ ] Config-factory fee drift measured on production; outcome recorded here
- [ ] No read scales a soroban token amount by an assumed 7; `decimals` is `null` where the token publishes none, and the frontend does not scale it
- [ ] API types regenerated; docs updated where a field became nullable
- [ ] Every read of `soroban_contract_metadata` goes through one definition of the newest row (NULLs included)

## One read of contract metadata (2026-09-28, decision 3 A)

The API read `soroban_contract_metadata` 13 times in 7 files, deduplicated
three ways: `FINAL` (3), `argMax(x, version)` (8) and
`argMax(tuple(x), version).1` (1, PR #518). `argMax` skips a `NULL`
argument, so the eight fell back to an older version's name, symbol or
decimals where the newest row has none. Production today: 0 of 3,947
contracts differ between the forms, so the bug is latent.

A plain view was considered and rejected: a Rust constant gives the same
single definition without a production DDL or a deploy-order dependency,
and the API is the table's only reader. Branch
`fix/0584-one-contract-metadata-read`, stacked on #535:
`common::contract_metadata::CONTRACT_METADATA` is the only read (newest
whole row per contract, `FINAL`, as `init.sql` documents). A test fails if
another file reads the table; a CH-gated test pins the `NULL` case (red
with `argMax`: `"Old"` instead of `NULL`).

Local API against production vs the deployed API: account balances, asset
list/detail/code filter, NFT list/collection filter/detail, search by name
and by id, soroban pool list — identical. One difference, not caused by this:
contract name search has no `ORDER BY`, so its results come back in a
different order on every call (same code, three calls, two orders).

## 2026-09-29 — "publishes no decimals" is our gap, not the token's

The 127 held Soroban tokens without decimals in `soroban_contract_metadata`,
asked on chain (`decimals()` simulated over RPC, mainnet): 124 answer, 2
answer 43,224 (the two `PIKA` contracts — not a scale), 1 has no token
interface (only `get_balance`). Of the 124: 82 are 7, 42 are not (0: 1,
6: 8, 8: 5, 9: 2, 13: 9, 18: 17); 185 holdings sit on those 42. So nearly
every such token does publish its decimals — through the SEP-41 function,
not in the `METADATA` instance-storage layout the parser reads. Before #535
those 185 holdings render off by 10^(d−7); after it they read "—"; the
real values come from task 0473 (standard-compliant metadata reads). The
wording "a token that publishes no decimals" in #535's comments should say
"whose decimals we have not read".
