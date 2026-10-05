---
id: '0616'
title: 'Name the protocol of every Soroban pool whose deployer is known'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0374', '0599', '0613']
tags: ['effort-small', 'priority-medium', 'liquidity-pools']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned while closing #405: 195 of 779 Soroban pools show no protocol.'
---

# Name the protocol of every Soroban pool whose deployer is known

## Summary

`protocol_labels.rs` names three deployments (one Aquarius router, one
Soroswap factory, one Phoenix factory). 195 of 779 Soroban pools come from
~20 other deployments and show no protocol. Name the ones whose operator is
known; leave the rest unnamed (decided 2026-10-05: no name without a known
operator).

## Context

Production 2026-10-05, deployments grouped by deployer account and WASM:

- **Aquarius, 77 pools:** older routers deployed by the same account as the
  named Aquarius router (`GAV5FBMK…`): `CAZREK5U…` (41), `CC2B3GFL…` (13),
  `CANMWW5D…` (8), `CDT6GQYR…` (6), `CBVSLUYH…` (3), `CDVTDAUA…` (3),
  `CALJOHJU…` (3).
- **Phoenix, 4 pools:** older factories of the named Phoenix factory's
  deployer (`GCNPDMUM…`): `CDL4NNRQ…`, `CCTBN3ON…`, `CDESO7ZN…`.
- **Normal Finance, 7 pools:** `CCPHUHQY…`, deployer `GAOATRJG…` whose
  `home_domain` is `normalfinance.io`, also named in the StellarExpert
  directory.
- **Unnamed, 107 pools:** the Aquarius router WASM deployed by `GDS2MUB7…`
  (`CA7RQDMM…`, 84, not listed by the Aquarius API), the Soroswap factory WASM
  deployed by `GAGMTYLC…` (`CCIQM2O3…`, `CDBRTEJM…`, `CCDATRT2…`, 21), and
  `GCHM6Y4B…` (2). No home domain, no directory entry.

## Implementation Plan

- Add the deployments above to the label table; a test per family.
- Record the unnamed ones in the file as known-unknown, so the next reader
  does not research them again.

## Acceptance Criteria

- [ ] 88 more pools carry a protocol on production; the 107 unnamed keep
      none.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (protocol label), or `N/A — reason`.
