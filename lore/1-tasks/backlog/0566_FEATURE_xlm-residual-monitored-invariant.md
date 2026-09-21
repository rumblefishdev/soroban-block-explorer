---
id: '0566'
title: 'FEATURE: store total_coins / fee_pool per ledger and monitor the XLM residual'
type: FEATURE
status: backlog
related_adr: ['0055', '0057']
related_tasks: ['0210', '0503', '0514', '0565']
tags:
  [feature, indexer, data-integrity, monitoring, priority-medium, effort-medium]
links: []
history:
  - date: '2026-09-18'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0210 on closing. The identity is measured and its floor is
      known (the chain's own state is short of total_coins by 0.000073% of
      supply, constant across checkpoints); what is missing is running it on a
      schedule instead of by hand.
---

# FEATURE: the XLM residual as a monitored invariant

## Summary

Task 0210 measured the XLM identity once, by hand, and found a stable floor.
The value of the check is in its SECOND run, not its first: the residual should
sit still, and any movement means value appeared or disappeared from our index.
`ledgers` stores neither `total_coins` nor `fee_pool`, so today the check needs
a history-archive read and a laptop.

## Context — what 0210 established

- Every venue the protocol keeps XLM in sums, with `fee_pool`, to a constant
  distance from `total_coins`: **0.000073% of supply**, identical to the stroop
  at two checkpoints 64 ledgers apart while accounts, pools and contract
  balances all moved. So the floor is a property of the chain (entries deleted
  by expiry before CAP-62), not of our ingestion.
- Our own divergence at the same moment was **0.000057% of supply**, in both
  directions, and every part of it is attributed to a filed defect.
- Both figures only mean something when the two sides are read at the SAME
  ledger: SAC balances alone move about 0.0001% of supply per 64 ledgers.

## Scope

1. **Store the header fields.** `total_coins`, `fee_pool` — and, while the
   writer is being touched, `base_reserve` and `bucket_list_hash` (0210's
   2026-08-18 list; the last one serves task 0502). `ALTER … ADD COLUMN …
DEFAULT` first, then the writer, per the ADR 0055 deployment order and the
   ADD-COLUMN-without-DEFAULT incident recorded there.
2. **Express the residual as one query** over `ledgers` and
   `balance_aggregates`, evaluated at a single ledger on both sides.
3. **Alert on movement, not on size.** The residual rising means we started
   missing value; falling or going negative means we are counting value that is
   not there. The threshold is a delta against the previous run, with the floor
   above as the expected constant.
4. **Schedule it** next to the other standing checks (task 0503 owns the
   schedule; task 0564 has the same "the check exists, nothing runs it"
   problem — do both in one pass if they land together).

## Acceptance criteria

- [ ] `ledgers` carries `total_coins`, `fee_pool`, `base_reserve`,
      `bucket_list_hash`, written live and by the backfill
- [ ] The residual is one query, both sides at one ledger, with a test on a
      throwaway ClickHouse
- [ ] It runs on a schedule and alerts on movement, with the measured floor as
      its baseline
- [ ] A run whose residual moves names the asset and direction, so the alert is
      actionable without a laptop
