---
id: '0468'
title: 'BUG: pool participants show "Since ledger 0" and link to a ledger that does not exist'
type: BUG
status: active
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
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Activated. UI half first (explicit absence, no dead link), then the
      root cause of the zeros, read-only.
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

## Root cause — measured on production (2026-09-25)

**The zeros are written by `repair-tier1`, not by the indexer, and the values
are recoverable.**

- Every zero sits in ONE part, `all_1_1_1` (107,728 rows, one bulk write,
  2026-07-16 — the `repair-tier1` EXCHANGE). The live writer produced 0 zeros
  since: 5,532 later rows, all with a real ledger. It cannot write 0 —
  `stage.rs:1656` falls back to `last_updated_ledger`.
- `rebuild_lp_positions` (`repair_tier1.rs:217`) takes the earliest deposit
  from `operations_appearances` joined on the OP's `source_id`. That column is
  NULL when the operation has no source of its own — the depositor is then the
  TRANSACTION's source — which is 664,198 of 1,594,568 deposit ops (42%).
  Those positions find no match.
- A missed LEFT JOIN does not yield NULL: `min(ledger_sequence)` is not
  Nullable and the read-only profile cannot set `join_use_nulls`, so the miss
  arrives as `0`, and `ifNull(0, existing)` keeps the 0. The fallback the code
  intends never fires; the correct value already in the row was overwritten.
- Of the 102,693 zero positions, 102,691 have no deposit matched on the op
  source. Sample of 12 zero positions with shares > 0, matched on
  `coalesce(op source, tx source)`: **12 of 12 have a deposit**, earliest at
  ledgers 50,474,915 – 61,081,979. A full count times out under the 30 s read
  cap; the sample is the evidence.
- Sibling rebuilds in the same file use the same `ifNull` over a LEFT JOIN,
  but their keys always match: `soroban_contracts.deployed_at_ledger = 0` for
  0 of 153,882 rows, `nfts.minted_at_ledger = 0` for 1 of 14,044,
  `accounts.first_seen_ledger = 0` for 40 of 25,414,838 (raw rows, not
  deduplicated).

**Consequence:** every future `repair-tier1` run — mandatory after a parallel
or `--reindex` backfill — re-zeroes every position whose deposit carried no
op source, including the correct rows written live since 2026-07-16.

**Fix, three parts:** (1) the rebuild matches on the op source, else the
transaction source, and a miss keeps the existing value; (2) a one-off data
repair of the zeroed rows on production (operator); (3) the UI renders an
unknown first deposit as an explicit absence, for whatever the repair cannot
recover.
