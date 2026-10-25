---
id: '0571'
title: 'BUG: contract-held SAC balances and pool legs keyed on the SAC contract instead of the asset it wraps'
type: BUG
status: completed
related_adr: ['0051', '0058']
related_tasks: ['0374', '0530', '0503', '0640', '0641', '0647', '0648']
tags:
  [
    'clickhouse',
    'indexer',
    'data-integrity',
    'priority-medium',
    'effort-medium',
  ]
links: []
history:
  - date: '2026-09-22'
    status: backlog
    who: karolkow
    note: >
      Filed from the 0530 review of PR #455. Two write paths store an asset
      surrogate for which no `assets` row is ever written. Both measured on
      production and traced to their cause; the two orphaned pool legs were
      repaired by hand the same day, the balances are not.
  - date: '2026-10-16'
    status: backlog
    who: karolkow
    note: >
      Part 1 (issuer-only classic pool legs) split out to 0648. Both roots of
      the SAC half found and measured on both networks
      (notes/R-sac-identity-2026-10-16.md). Fix decided: keep the write-time
      translation, take SAC identity from the instance entry, drop the
      deployed filter, fail loudly on an unresolved SAC, repair both
      networks. The read-time shape is filed as 0647.
  - date: '2026-10-19'
    status: active
    who: karolkow
    note: >
      Step 1 of the order of work: SAC identity from the instance entry and
      the map without the deployed filter.
  - date: '2026-10-19'
    status: active
    who: karolkow
    note: >
      Step 1 done: PR #671 (tests moved) and #672 (identity from the instance
      entry) merged; the stored flags repaired on both networks. Next: deploy
      #672 (testnet, then mainnet), then task 0641 decides the direction.
  - date: '2026-10-19'
    status: active
    who: karolkow
    note: >
      SAC-keyed rows re-keyed on both networks (G-sac-rekey-repair.sql), all
      checks 0. Left: #672 on mainnet with the next release; the backfill
      still mis-keys same-partition SACs (step 5a in docs/backfills.md).
  - date: '2026-10-19'
    status: completed
    who: karolkow
    note: >
      Completed: root causes found, PR #671/#672 deployed on both networks
      (29 testnet SACs deployed since, all with the flag set), flags and
      SAC-keyed rows repaired on both networks (all asset-id columns 0).
      Deferred: the miss alarm to 0652, a backfill without the manual step
      to 0647.
---

# BUG: SAC balances and pool legs keyed on the SAC contract

## Summary

A contract that holds a classic asset through its Stellar Asset Contract
(SAC) gets its balance stored under the SAC contract's surrogate instead of
the asset the SAC wraps. A SAC has no `assets` row (ADR 0051), so the row
joins to nothing: the asset page misses the holder, and supply is short by
the amount. Soroban pool legs and pool movements are keyed by the same
function and break the same way.

Two independent causes, one per network — see "Root causes" below.

## Root causes (measured 2026-10-16)

Full numbers, queries and the deploy classification:
[notes/R-sac-identity-2026-10-16.md](notes/R-sac-identity-2026-10-16.md).

**1. Mainnet — SAC identity read from the envelope.** A SAC deployed by
another contract carries no asset in its envelope, so its facet got
`sac_deployed = 0` and the SAC → asset map skipped it: 147 of 147 checkable
SACs with the flag at 0 were contract-deployed; 53 of 53 with 1 were direct.
Fixed in PR #672 (identity from the instance entry's `METADATA.name`).

**2. Testnet — write-time translation depends on backfill order.** A
partition commits at its end, so a balance written before its SAC's facet is
visible falls through: 100% in the deploy's own partition, 3% eight or more
partitions later. Same class as 0640; covered by step 5a of
`docs/backfills.md` until 0647.

## Decisions so far

| situation                                                                       | what happens                                                                                                                                                                                                                                                                                                                                                                                                 | date       |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------- |
| order of work                                                                   | (1) now: SAC identity from the instance entry + one-time repair of the stored flags, right under any later direction; (2) task 0641 decides one direction for every backfill-vs-live class, as an ADR; (3) 0647 as its first application, or dropped if the ADR says so; (4) in parallel, the invariant runner of 0564. Write-time event seeding, partition memory and the SAC-to-asset re-key are not built | 2026-10-19 |
| existing SAC-keyed rows                                                         | re-keyed now by one script per network, testnet first ([notes/G-sac-rekey-repair.sql](notes/G-sac-rekey-repair.sql)): insert under the asset, then delete; 0641 postponed                                                                                                                                                                                                                                    | 2026-10-19 |
| a SAC instance entry is created or changes                                      | its identity (code + issuer, or native) comes from the entry's `AssetInfo` storage, not from the envelope                                                                                                                                                                                                                                                                                                    | 2026-10-16 |
| the SAC → asset map is read                                                     | keeps `HAVING max(sac_deployed) = 1`; the wrong flags are repaired once ([notes/G-sac-deployed-flag-repair.sql](notes/G-sac-deployed-flag-repair.sql)). Measured: without the filter the map grows from 3,916 to 327,512 entries on mainnet, nearly all reserved SACs that cannot hold a balance                                                                                                             | 2026-10-19 |
| a balance stored in a SAC (BalanceValue struct) whose SAC the map does not know | row still written under the SAC; `error!` with `alarm = "sac_map_miss"` and a CloudWatch alarm (not a failed ledger: any contract can forge the shape) — PR #680 closed 2026-10-19, deferred                                                                                                                                                                                                                 | 2026-10-19 |
| repair                                                                          | one statement set per network, testnet first, run by the owner; `balances`, `liquidity_pools.legs`, `pool_movements`, the `sac_deployed` facets                                                                                                                                                                                                                                                              | 2026-10-16 |
| both networks                                                                   | every fix and repair covers `testnet` and `default`                                                                                                                                                                                                                                                                                                                                                          | 2026-10-16 |

## Not yet decided

- Where a backfill into a database without the SAC facets (testnet reset,
  fresh environment) gets the full map before its partitions run. Measured
  2026-10-16: the checkpoint seed does not decode `ContractData`
  (`snapshot/seed.rs`, "deliberately NOT seeded"), so a snapshot source is
  new code.
- How "known to be a SAC" is decided for a pool-leg token address, which
  carries no entry of its own in the ledger being staged.

## Out of scope

- Issuer-only classic pool legs: 0648.
- Storing the raw holding contract and resolving at read time: 0647.
- Other post-backfill passes: 0641.

## History

The state as first filed, the cases ruled out and the notes from the pool-leg
repair: [notes/R-first-filing-2026-09-22.md](notes/R-first-filing-2026-09-22.md).
Measurement queries: [notes/R-sac-identity-2026-10-16.md](notes/R-sac-identity-2026-10-16.md).

## Acceptance Criteria

- [x] Root cause of `sac_deployed = 0` on a deployed SAC found (2026-10-16)
- [x] A SAC deployed by another contract gets its identity from its instance
      entry — parser test on a real contract-deployed SAC instance — PR #672, merged 2026-10-19 (not yet deployed); end-to-end on ledger 64,951,791 in the PR
- [x] Every SAC in `soroban_contracts` has `sac_deployed = 1` (flag repair run on `testnet`, then `default`) — repaired 2026-10-19: testnet 73 → 0, default 148 → 0
- [ ] Neither `build_balance_rows` nor `contract_token_asset_id` keys a known
      SAC as a type-3 token; an unresolved SAC is an error naming it — deferred to 0652 (PR #680 with the `sac_map_miss` alarm closed 2026-10-19, not merged)
- [ ] A backfill into a database without the facets ends with no SAC-keyed
      rows, with no manual step — deferred to 0647; until then step 5a of `docs/backfills.md`
- [x] Repair run on `testnet`, then `default`: no `balances`,
      `liquidity_pools.legs` or `pool_movements` row keyed on a SAC contract;
      `sac_deployed = 1` for every SAC in `soroban_contracts` — done 2026-10-19: testnet 22,166 balances / 6,560 movements / 501 legs → 0; default 300 balances → 0
- [x] **Docs updated** — `docs/architecture/**` on where SAC identity comes
      from; `docs/backfills.md` if the backfill gains a step; or `N/A — reason` — `xdr-parsing-overview.md` (PR #672), `docs/backfills.md` step 5a
- [x] **API types regenerated** — `N/A` unless `crates/api/**` changes — N/A — no `crates/api/**` change
