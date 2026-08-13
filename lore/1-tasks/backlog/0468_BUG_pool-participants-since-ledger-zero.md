---
id: '0468'
title: 'BUG: pool participants show "Since ledger 0" and link to a ledger that does not exist'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0377']
tags: [frontend, liquidity-pools, data-quality, priority-medium, effort-small]
links: []
history:
  - date: '2026-08-10'
    status: backlog
    who: karolkow
    note: >
      Found by the regression sweep over pages outside the 2026-07-25 →
      2026-08-07 release window. Measured on production after deduplication:
      102 693 of 108 304 liquidity-pool positions carry
      `first_deposit_ledger = 0` — 94.8 %. The column renders the value as a
      clickable ledger identifier, so it links to `/ledgers/0`, which answers
      "Ledger not found".
---

# BUG: pool participants show "Since ledger 0"

## Summary

The "Since ledger" column on the liquidity-pool detail page renders
`first_deposit_ledger` through `IdentifierDisplay type="ledger"`
(`web/src/pages/pool-detail/PoolParticipants.tsx:63`). For 94.8 % of positions
that value is `0` — not a ledger, and the resulting link is a dead end.

Two separate faults sit on top of each other: the data is absent for almost
every position, and the UI presents the absence as a fact and invites a click
on it. Either alone would be a defect; together they read as a broken page.

## Context

Measured on production (`lp_positions`, deduplicated by `(pool_id,
account_id)`):

| `first_deposit_ledger` | positions | share      |
| ---------------------- | --------- | ---------- |
| `0`                    | 102 693   | **94.8 %** |
| a real ledger          | 5 611     | 5.2 %      |

The column is fed straight through: `queries.rs:477` selects
`lpp.first_deposit_ledger`, `dto.rs:46` types it `i64` (not nullable), and the
cell stringifies it. Nothing along the path can express "not known".

## Implementation

Two halves, and the second is worth doing even if the first is slow:

- **Data** — establish why the field is zero for almost every position. Either
  the indexer never sets it (positions observed from a snapshot rather than
  from the deposit that created them) or it is written as a default. If the
  value cannot be recovered for historical positions, the wire type must be
  able to say so (`Option<i64>` / nullable) rather than defaulting to `0`.
- **UI** — a position with no known first deposit must render an explicit
  absence, never a linked `0`. Follow the 0377 rule: say "unknown", do not
  render a plausible-looking value the reader will take as measured.

## Acceptance criteria

- [ ] No pool participant renders a link to a ledger that does not exist
- [ ] Absent first-deposit ledger renders as an explicit absence, not `0`
- [ ] Root cause of the missing value established and recorded (indexer gap
      vs. default-on-write)
- [ ] If the value is recoverable, historical positions backfilled; if not,
      the wire type carries the absence
- [ ] **Docs updated** — LP detail contract under `docs/architecture/**` if
      the wire shape changes
- [ ] **API types regenerated** — required if `first_deposit_ledger` becomes
      nullable
