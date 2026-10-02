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

## Lead (2026-10-02)

The lifetime `total_volume` may be the wrong yardstick. The protocol also
serves hourly per-pool statistics, `amm-api.aqua.network/statistics/pool/<address>/`
(`volume_usd`, scaled by 1e7), and every pool event with amounts,
`/events/pool/<address>/`. On the five three-leg pools, our trades priced
with our hourly closes came within 0.3–0.4 % of the hourly `volume_usd`, and the
swap counts per pool were equal over the same window (13,498 / 13,498). The
same comparison on the two-leg pools of this task, window by window, should
separate a method difference from missing events.

## Measured (2026-10-02)

Our API on production data (branch of #598) against the protocol's per-pool
hourly `volume_usd`, all 356 Aquarius pools, 2026-09-03 → 2026-10-01 16:00
UTC (688 hours): $98,430,530 ours vs $98,396,948 theirs over the hours we
price (+0.034 %); the hours we leave `null` hold 0.79 % of their volume, all
in pools with a leg our prices service does not price (pure Soroban tokens,
e.g. `CCNXGPE4…`, $768k) or without published decimals. Per pool, 68 of 86
pools with ≥ $1,000 are within 0.5 %, 14 within 2 %; the outlier `CD2ZV2IM…`
(BTC/XLM, +7.5 %) is a leg-price difference. We report no volume in any hour
they report none.

The protocol's own daily total (`statistics/totals`, also what DefiLlama
publishes) is 5.9 % BELOW the sum of its per-pool hourly statistics over
2026-09-03..30 ($90.79M vs $96.54M); ours, at daily grain, is −0.41 % against
the per-pool sum. So the lifetime-volume gap this task started from likely
sits between the protocol's own two figures (a routed swap counted once in
the total and per hop in the pools is one hypothesis, unverified), not in
`pool_movements`.

Soroswap against DefiLlama (a Dune query; zero from 2026-05 on, so
2025-10..2026-04 only): +1.78 % over 212 days, monthly −2.1 %..−0.2 % except
2025-10 at +11.9 % (unexplained).
