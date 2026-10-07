---
id: '0623'
title: 'Non-SAC token balances from balance(), not the Balance storage key — measure first'
type: RESEARCH
status: backlog
related_adr: ['0061']
related_tasks: ['0620', '0331', '0415']
tags: ['effort-medium', 'priority-low', 'soroban', 'tokens', 'balances']
links: []
history:
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: 'Spawned from the 0617 audit (W244/W251); builds on the 0620 executor.'
---

# Non-SAC token balances from `balance()`

## Summary

Bespoke tokens' balances are read from a `Vec[Symbol("Balance"), Address]`
entry with a bare `i128` (`state.rs:202,259`, `ledger_value.rs:344,361`,
`backfill-runner/src/rpc_snapshot.rs`). A token keeping balances elsewhere
shows no holders. Decide whether SEP-41 `balance(holder)` via the 0620
executor should replace it.

## Questions to answer before building

- How many token contracts have transfers (SEP-41 events) but no balance
  rows — the size of the silent gap. One contract per program.
- Holder set: events name the holders; `balance()` gives the value at the
  ledger of each change. Cost per ledger at today's transfer rate.
- SAC balances are host-defined (`BalanceValue`) — keep them as they are.

## Acceptance Criteria

- [ ] Gap measured; a build task created or the idea rejected with numbers.
