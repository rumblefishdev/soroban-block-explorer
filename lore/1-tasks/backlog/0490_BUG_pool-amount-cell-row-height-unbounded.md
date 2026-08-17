---
id: '0490'
title: 'BUG: the pool Amount cell grows without bound and its Event chip names no line'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0279', '0489', '0491']
tags: [frontend, layer-frontend-pages, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/371'
history:
  - date: '2026-08-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 0279 release review. An arbitrage bundle rendered ten
      stacked amount lines in one row under a single Trade chip. Deliberately
      scoped to the row-height cap only — the structural answer (one row per
      operation) is 0491, and doing both here would pull the API change
      forward for no gain.
---

# The pool Amount cell grows without bound

## Summary

The Amount column on the pool detail page renders one line per operation. A
transaction that runs ten operations against the same pool renders ten lines
in one cell, and the row's single `Event` chip describes none of them
individually. Cap the stack and give the overflow somewhere to go.

## Context

Task [0279](../active/0279_FEATURE_lp-op-details-amount-column.md) chose one
line per operation on purpose, and the reasoning holds: 8.2% of (pool,
transaction) pairs run several operations against one pool, and summing them
describes neither — a bundled deposit + path payment reads smaller than the
deposit its own chip names.

What the reasoning assumed was two or three lines. Production served ten, from
an arbitrage bot, within an hour of the release.

Two things compound it:

- **[0489](../active/0489_BUG_pool-amount-drops-credit12-leg.md) widens every
  line.** Once the dropped leg comes back, a line reads
  `0.0025 yXLM → 0.31 CETES` instead of `0.0025 yXLM`. The column is
  `width: 260`, so ten lines also means ten wrapped lines.
- **The `Event` chip is per transaction, not per line.** It comes from
  `classifyLpTx(row.operation_types)`, so a bundle of a deposit and a trade
  gets one chip that is wrong for at least one of the lines under it.

## Implementation

- Render at most three lines; collapse the rest into `+N more` linking to the
  transaction detail page, which already carries the full per-operation
  breakdown.
- Re-check the `260` column width against the two-leg form 0489 restores.
- The chip's dishonesty is **not** fixed here — one chip cannot describe a
  mixed bundle. It is fixed by 0491 making the row a single operation. If 0491
  lands first, this task is likely moot; check before starting.

## Acceptance Criteria

- [ ] A transaction with ten operations against one pool renders a bounded
      row — three lines plus an overflow affordance
- [ ] The overflow links to the transaction detail page
- [ ] The single-operation case (92% of rows) is visually unchanged
- [ ] Column width holds the two-leg `A → B` form from 0489 without wrapping
      at the standard desktop breakpoint
- [ ] **Docs updated** — N/A expected (presentation only, no shape change)
- [ ] **API types regenerated** — N/A (frontend only)
