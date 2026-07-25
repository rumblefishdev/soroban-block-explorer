---
id: '0429'
title: 'RESEARCH: how should the indexer learn state it never observed changing? (retire balance-seed)'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0425', '0421', '0331', '0214']
tags: [research, clickhouse, indexer, balances, effort-medium, priority-low]
links:
  - crates/backfill-runner/src/balance_seed.rs
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Deferred out of the 0425 audit rather than decided in-line. 0425 sorted every
      backfill subcommand by "can live produce these rows by itself?" and
      `balance-seed` was the only survivor of the "no, and never will" bucket once
      `bootstrap` was measured into the mop category instead. The options for it are
      genuinely open and range from "do nothing" to "re-architect ingest around a
      state snapshot", which is too wide a spread to settle inside a cleanup task.
---

# RESEARCH: seeding state the indexer never observed changing

## Question

An event-driven indexer only learns about an entity when something **happens** to
it. A token holder whose last movement predates the parser emits nothing further,
so live ingest will never see them — not because logic is missing, but because
there is no event to see.

`backfill-runner balance-seed` fills that gap today by reading current state from
Soroban RPC. It is a manual, one-shot, non-replayable pass. **Should it stay that
way, or is there a shape where the system never needs it?**

## Why this is not urgent

Measured 2026-07-21 — the live path is healthy and the gap does not reopen:

|                                        |                                                 |
| -------------------------------------- | ----------------------------------------------- |
| `balances` rows / distinct holders     | 89,035,435 / 14,140,360                         |
| newest write                           | ledger 63,583,709 — the chain tip               |
| writes in the last 80k ledgers         | 2,874,370                                       |
| defaulting fallback in the row builder | **none** — both build sites carry a real amount |

That last row is the important one. Unlike `accounts` (see 0421), nothing here
rewrites a row with placeholder values, so existing balances are never emptied.
The unobserved population is a **fixed historical residue that shrinks** — every
holder who moves tokens after the parser shipped is picked up by live.

**So the honest default is "do nothing".** This task exists to decide that
deliberately, with a number, rather than by omission.

## Options

Roughly in ascending order of cost. None is chosen yet.

1. **Do nothing.** Keep `balance-seed` as a manual tool for the rare re-seed.
   Cheapest, and defensible given the gap is bounded and shrinking. Risk: nobody
   knows how big the residue actually is.

2. **Measure and monitor.** Quantify the unobserved population once, then alert if
   it grows. Turns option 1 from an assumption into an informed decision. Should
   probably happen first regardless of what else is chosen.

3. **Recover from events already in ClickHouse.** Scan historical `soroban_events`
   for token transfers and reconstruct the holder set in-DB — no RPC, replayable,
   fits the 0425 rule that a one-off should reuse live logic. Ceiling: only
   recovers holders who appear in an _indexed_ event; anyone whose only movement
   predates the indexed range stays invisible.

4. **Lazy fill at read time.** When the API is asked about a holder or asset with
   no row, fetch from RPC and cache. Bounded by actual demand rather than by the
   size of the chain. Cost: RPC on a read path, and a cache-invalidation question.

5. **Scheduled RPC top-up in the enrichment Lambda.** Automates the existing mop.
   Cheap to build, but the result is not replayable (it reads "now", so two runs
   disagree) and it makes a manual pass into a permanent background system.

6. **Snapshot-seeded ingest.** Seed full state from a Stellar history-archive
   checkpoint at the backfill start ledger, then apply deltas — the standard
   snapshot+deltas shape, which is how captive-core catches up. Deterministic and
   replayable, and it would retire `balance-seed` _and_ the historical residue in
   `accounts` at once. **Unverified:** availability, format and size of the bucket
   files have not been checked, and it is a different source from the AWS public
   dataset the backfill uses today.

7. **Third-party API (Horizon or similar) as the seeding source** instead of
   Soroban RPC. Same shape as 5 or 6 with a different dependency; listed for
   completeness, and it trades a self-hosted dependency for someone else's uptime.

## What would decide it

Measure the residue first (option 2). Concretely: how many holders exist on chain
that `balances` has never seen, and is that number falling? If it is small and
shrinking, option 1 wins and this task closes with a recorded decision. If it is
large or static, options 3 and 6 are the two that actually remove the problem
rather than automate it.

Do this **after** 0421 lands — that fix removes the account-skeleton residue,
which is the larger and better-measured half of the same question, and it may
change what is left to solve here.

## Acceptance Criteria

- [ ] The unobserved-holder residue is measured, not estimated.
- [ ] One option is chosen with the reasoning written down, including why the
      others were not.
- [ ] If the answer is "do nothing": `balance-seed` keeps its entry in
      `crates/backfill-runner/README.md` with the decision linked, so the next
      audit does not re-open the question.
- [ ] If the answer is anything else: the implementation task carries the
      criterion that `balance-seed` is deleted when it lands (lore 0425 clause 4).
