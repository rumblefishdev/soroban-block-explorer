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
  - date: '2026-10-01'
    status: active
    who: karolkow
    note: >
      Measured on the whole population (110,066 positions): 3.8% correct.
      Decided 389 B / 393 A / 394 A / 395 A: first deposits kept as a min the
      engine maintains in a new table lp_first_deposits (ADR 0056 section 4's
      MV failed its gate, 152 GiB per refresh); the column reads
      "First deposit". PRs #572 (move) and #573 (table + writer) open.
  - date: '2026-10-01'
    status: active
    who: karolkow
    note: >
      Reversed to 399 B: the column is dropped, not stored — a table for one
      column judged not worth it, the pool page's Activity lists deposits.
      #573 closed unmerged; #572 and #578 (removal) merged. Operator set
      DEFAULT 0 on the column. Open: deploy, then DROP COLUMN.
---

# BUG: pool participants show "Since ledger 0"

## Summary

The pool participants table shows when each account joined the pool. It reads
`lp_positions.first_deposit_ledger`, which is wrong for almost every position:
the table keeps the latest row per (pool, account), so each later change
overwrote the first deposit with its own ledger, and `repair-tier1` wrote `0`
over most of the rest (a link to the non-existent `/ledgers/0`).

## Stan teraz

- Measured 2026-10-01, every position: 110,066 → **4,164 correct (3.8%)**,
  86,102 wrong, 19,800 with no successful deposit in our history.
- **Decided 399 B (karolkow, 2026-10-01): drop the column.** It replaces the
  earlier 394 A (a `lp_first_deposits` table with a `min`, PR #573 — closed
  unmerged, branch `feat/0468-lp-first-deposits` kept). Reasons: a table for
  one column; the pool page's Activity section already lists each provider's
  deposits. Rejected on cost: a per-pool read without an index — 14.75 bn rows
  / 107 GiB / 18 s per page view (busiest pool, measured).
- Merged: #572 `[structure only]` (move `lp_positions` staging out of
  `stage.rs`), #578 (removal: UI column, API field, parser/indexer,
  `repair-tier1` LP rebuild, `init.sql`, docs, ADR 0056 amendment).
- Operator, 2026-10-01: `MODIFY COLUMN first_deposit_ledger Int64 DEFAULT 0`
  — confirmed `DEFAULT 0` in `system.columns`.
- Next: deploy Compute + frontend → operator `ALTER TABLE lp_positions DROP
COLUMN first_deposit_ledger` → close the task.
- [notes/G-backfill.md](notes/G-backfill.md) is obsolete (it served 394 A).

## Measurements behind the decisions (2026-10-01, production, read-only)

- Failed transactions keep their operations in `transaction_operations`:
  298,794 of 1.63 M deposits (18.4%). Counting them made 3,610 phantom
  (pool, depositor) pairs; filtered, 4 remain without a position row.
- A full recompute of the first deposit over the operations reads 16.0 bn
  rows / 152 GiB (15 ledger slices, 23 s CPU) — why the ADR's MV is out.
- Fee-bump: `envelope_source` is the inner transaction's source, so the
  depositor is right for the 211,866 fee-bump deposits.
- With the backfill SELECT: 90,270 pairs; 90,266 positions (82.0%) get their
  true first deposit, 19,800 (18.0%) stay unknown.

## Acceptance criteria

- [ ] No pool participant renders a link to a ledger that does not exist
      (#578 removes the column — done at deploy)
- [ ] ~~Absent first deposit renders as an explicit absence, not `0`~~ —
      superseded by 399 B: no first deposit is shown
- [x] Root cause established and recorded (writer overwrite + repair-tier1
      zeros; [notes/R-root-cause-2026-09-25.md](notes/R-root-cause-2026-09-25.md))
- [ ] ~~Recoverable values backfilled~~ — superseded by 399 B
- [x] **Docs updated** — schema, backend API, backfills, ADR 0056 (#578)
- [x] **API types regenerated** — #578
- [ ] Column dropped on production after the deploy
