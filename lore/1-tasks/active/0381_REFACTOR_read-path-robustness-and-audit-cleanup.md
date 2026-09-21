---
id: '0381'
title: 'Read-path robustness + architecture-audit cleanup (poison-pill, overscan, dead index/dictionary)'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0359', '0541']
tags: [priority-medium, effort-large, layer-api, robustness]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker §11 (G-architecture-audit MAJOR/MINOR items not otherwise covered).'
  - date: 2026-07-13
    status: active
    who: karolkow
    note: 'Promoted to active to begin implementation.'
---

# Read-path robustness + architecture-audit cleanup

## Summary

Address the read-path robustness + cleanup items from the 0359 architecture audit
(G-architecture-audit) not covered by the other spawned tasks.

## Context

Spawned from 0359 §11. These are query-engine robustness + dead-weight cleanup
items surfaced by the audit; independent of the write-side re-model.

## Implementation

**MAJOR (read robustness):**

- Poison-pill quarantine — a single bad row/partition shouldn't fail a whole read.
- Partition-pinned filtered global lists — global lists that pin to one partition
  miss cross-partition results.
- Overscan ×4 without refill — over-fetch factor never refills to fill a page.

**MINOR (cleanup / small perf):**

- `ledgers` `LIMIT 1 BY` read-in-order check.
- Cursor-to-filter binding.
- Dead dictionary + `idx_tx_hash_bloom` removal.
- Muxed-id dropped in details JSON (preserve the muxed memo-id).
- Sibling-wildcard canary tests for `emit_asset_appearances` /
  `extract_counterparties` / `claim_atoms` (guard against a silent `_` regression).

## Measured instances (task 0541 review, 2026-09-21)

The review of 0541 swept the lists for the short-page defect: a page that comes
back under `limit` for any reason but the end of the data reads as "no next
page", because the envelope infers the end from the row count. Measured on
production 2026-09-20.

| list                                                       | cause                                                                                 | reach                                                                                      |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| transactions, every filtered variant                       | pages inside one partition (canonical SQL 02)                                         | a list ends at a partition boundary, every 500,000 ledgers                                 |
| liquidity-pool participants (`liquidity_pools/queries.rs`) | a row dropped for a dangling account surrogate takes the `limit + 1` sentinel with it | 82 of 26,489 pools hold more than one page, the largest 684; only when a surrogate dangles |
| assets (`assets/queries.rs`, `SEEK_OVERFETCH`)             | over-fetch, then version dedup; warns and still returns short                         | latent: at most 6 versions per key                                                         |
| ledgers (`ledgers/queries.rs`, `LEDGER_OVERFETCH`)         | over-fetch ×3, no guard                                                               | latent: at most 1 row per sequence                                                         |

0541 fixed the same defect in the contract-filtered transaction list (804
transactions, 13 shown, no next page) by giving contracts a presence index,
`contract_transactions`, like the ones accounts, assets and pools have. All four
entities can now seek a transaction list across partitions through an index.

The rule the fix followed: a bounded search that stops before exhausting its
input hands its bound to the caller; the envelope never infers "end of list"
from a count the search itself capped.

## Acceptance Criteria

- [ ] poison-pill quarantine · partition-pinned lists · overscan-refill fixed
- [ ] MINOR cleanup items resolved or explicitly deferred with reason
