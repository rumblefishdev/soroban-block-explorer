---
id: '0508'
title: 'REFACTOR: three crates do something their role does not allow'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0455', '0418']
tags:
  [
    'architecture',
    'rust',
    'crate-boundaries',
    'effort-medium',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0455 review sweep (findings 20, 21, 24). Three separate
      reports, one rule: a crate does work its role does not allow. Bundled
      deliberately — each alone reads as a detail and would never be picked up;
      together they are a stated boundary and three enforcements of it.
---

# REFACTOR: three crates do something their role does not allow

## Summary

The crate layout is a compile-enforced ports-and-adapters-lite arrangement and
is sound as a shape. Three concrete violations undermine it. None is a bug
today; each removes a boundary the layout is supposed to guarantee, and each
was found independently by a different pass of the same review.

## Context — the three, all verified 2026-08-19

**A. An operational tool links a deployable.**
`crates/backfill-runner/Cargo.toml` declares `indexer = { path = "../indexer" }`.
`indexer` is the crate deployed as a Lambda. A CLI an operator runs on a laptop
therefore compiles, and can call, the deployed handler's code. Whatever the
runner needs from it is either shared logic that belongs in a library crate, or
handler logic it should not be reaching for.

**B. A fetch library also persists.**
`crates/enrichment-shared/Cargo.toml` depends on `db-clickhouse`. Anything that
links the fetch library links the persistence crate — and `crates/api` links
`enrichment-shared`. The read-side API therefore compiles the write path.

**C. A pure parser holds process-global state.**
`crates/xdr-parser/src/sac.rs:63` holds
`static NET_ID: OnceLock<Option<[u8; 32]>>`, initialised from the
`STELLAR_NETWORK_PASSPHRASE` environment variable. A parser that reads process
environment and caches it for the life of the process cannot be exercised twice
with different networks in one test binary, and its behaviour depends on which
caller initialised it first. The code comment records that this consolidated an
earlier duplicate global from the indexer — the consolidation was right, the
remaining question is whether a parser should own it at all rather than take it
as a parameter.

## Implementation

Decide the rule first, then apply it three times:

- Which crates are **deployables** (compiled into an artifact that ships) and
  therefore may not be depended on by anything else.
- Which are **libraries**, and what a library may not do (reach the database,
  read process environment, hold global state).

Then: A — move whatever `backfill-runner` needs out of `indexer` into a library
crate, or duplicate it if it is small. B — split persistence out of
`enrichment-shared`, or invert it so the caller supplies the sink. C — take the
network id as a parameter, with the `OnceLock` retained at the composition root
if a cache is still wanted.

The rule belongs in the module-conventions ADR that task 0418 already carries;
this task is its first enforcement.

## Acceptance Criteria

- [ ] Deployable-vs-library distinction written down (ADR, with 0418)
- [ ] No crate depends on a deployable crate; enforced mechanically, not by review
- [ ] `crates/api` no longer links the persistence crate transitively through
      the fetch library
- [ ] `xdr-parser` reads no process environment; the network id arrives as a
      parameter
- [ ] A test binary can parse against two different networks without process
      isolation
- [ ] **Docs updated** — `docs/architecture/**` crate map reflects the rule
- [ ] **API types regenerated** — required if `crates/api` dependencies change
