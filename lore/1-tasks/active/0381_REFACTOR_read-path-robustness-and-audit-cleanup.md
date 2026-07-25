---
id: '0381'
title: 'Read-path robustness + architecture-audit cleanup (poison-pill, overscan, dead index/dictionary)'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0359']
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

## Acceptance Criteria

- [ ] poison-pill quarantine · partition-pinned lists · overscan-refill fixed
- [ ] MINOR cleanup items resolved or explicitly deferred with reason
