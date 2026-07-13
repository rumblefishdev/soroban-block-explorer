---
id: '0377'
title: 'Aggregate/detail hygiene: KPI windows, operation_count vs folded, nullable-aggregate 500 trap'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-api, aggregates]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles K4-1, K4-2/K1-4, K4-5.'
---

# Aggregate/detail hygiene

## Summary

Fix aggregate/detail divergences and the systemic nullable-aggregate 500 trap.
Read-side query fixes over existing data.

## Context

Spawned from 0359 (K4 cluster). The nullable-aggregate trap is systemic (see
memory: CH nullable-aggregate decode trap — an aggregate over a Nullable column
returns `Nullable(T)`, `fetch_one::<T>` on non-nullable → 500).

## Implementation

- **K4-1** — contract invocations KPI: reconcile 7d vs all-time window.
- **K4-2 / K1-4** — tx `operation_count` vs folded `operations[]` (identical ops
  fold so `len(operations[]) < operation_count`).
- **K4-5** — audit + fix the nullable-aggregate decode 500 trap across endpoints
  (decode `Option<T>` or coalesce).

## Acceptance Criteria

- [ ] invocations KPI window consistent — K4-1
- [ ] operation_count vs operations[] reconciled — K4-2 / K1-4
- [ ] nullable-aggregate 500 trap eliminated (systemic sweep) — K4-5
