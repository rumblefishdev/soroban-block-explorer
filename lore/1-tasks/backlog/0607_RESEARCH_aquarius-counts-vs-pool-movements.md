---
id: '0607'
title: 'Aquarius pool tx counts and volume vs pool_movements'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0374']
tags: ['effort-medium', 'priority-medium', 'data-completeness']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-01
    status: backlog
    who: karolkow
    note: 'Spawned from 0374: oracle comparison against the protocol API.'
---

# Aquarius pool tx counts and volume vs pool_movements

## Summary

The protocol's own API (`amm-api.aqua.network/api/external/v1/pools/`, 355
pools, read 2026-10-01) reports `tx_count` and `total_volume` per pool. Our
`pool_movements` agrees on counts for most pools but not the largest, and our
volume is consistently higher. Find out whether we miss events or the API
counts differently.

## Context

Counts (346 pools with data): our distinct pool events equal `tx_count` for
170, within 1 % for 48 more; the busiest pools are 1–9 % below the API
(`CCY2PXGM…` 276,056 vs 254,947; `CA6PUJLB…` 1,428,409 vs 1,416,211).

Volume (15 pools, lifetime): `total_volume` / 10^7 against our trade leg-A
volume priced at daily closes gives ratios 0.79–1.50, typically 1.10–1.40
(ours higher). Ruled out on three pools (XLM/USDC, USDC/sUSD, XLM/SHX):

- which side is counted: leg A both ways, leg B both ways, all inflow and all
  outflow agree within 0.5 % of each other;
- a common start date: the date from which our sum equals theirs differs per
  pool (2025-05-11, 2025-03-14, 2025-08-18);
- router-only counting: the share of volume invoked through the pool's router
  (6.5 %, 61 %, 6.2 %) does not match the gap.

Our underlying amounts match raw XDR token transfers for 397 of 519
operations sampled over 6 days; the rest are explained classes (Phoenix
commission, surplus).

## Implementation Plan

- Counts: for one pool with a gap, diff our events against raw XDR for a
  window (RPC `getTransaction`, last ~7 days) and classify what the API counts
  that we do not (non-amount events such as reward claims?).
- Volume: ask whether `total_volume` is USD and at which price; or recompute
  their figure for one pool with hourly prices at trade time.
- Record the outcome in this task; a real gap in `pool_movements` becomes a
  bug task.

## Acceptance Criteria

- [ ] The count gap is classified (ours missing vs theirs extra), with numbers.
- [ ] The volume gap is attributed to a method difference or a data gap.
