---
id: '0521'
title: 'BUG: the snapshot seed re-emits every signers row on every pass, so one of its four reported numbers carries no information'
type: BUG
status: completed
related_adr: ['0057']
related_tasks: ['0463', '0503', '0515']
tags:
  [
    snapshot,
    backfill-runner,
    clickhouse,
    data-integrity,
    effort-small,
    priority-medium,
  ]
links: []
history:
  - date: '2026-08-27'
    status: active
    who: karolkow
    note: >
      Filed from the 0463 idempotency measurement (S1, checkpoint 64,132,415):
      `balances` corrections fell 44,834,785 → 1 on the second pass, while
      `account_entry_state` emitted 10,872,072 rows again — the full live-account
      set, unchanged.
  - date: '2026-09-30'
    status: active
    who: karolkow
    note: >
      PR #446 now conflicts with develop: task 0210 moved the seed passes into modules and made account_entry_state a full rewrite. The gate must be re-applied on that structure.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      #568 merged (replaces #446); production dry run 0 written, 11,004,932
      unchanged of 11,004,932 live accounts.
---

# BUG: the snapshot seed re-emits every signers row on every pass

## Summary

`snapshot-seed`'s pass 4 iterates every live account in the checkpoint snapshot
and emits an `account_entry_state` row **unconditionally**, with no comparison
against what we already hold. The other three passes all compare first. The rows
are byte-identical at the same ReplacingMergeTree version, so the DATA is
unaffected — but the number the operator reads in `summary.txt` is a constant,
not a measurement.

## Stan teraz

- Done: #568 merged 2026-09-30 — the gate rebuilt as
  `snapshot/entry_state.rs` on 0210's module layout (thread 377 A), using
  the pool pass's rule `pools::need`. #446 closed as superseded.
- Production dry run 2026-09-30 (checkpoint 64,699,071, read-only):
  `account_entry_state 0 (0 new, 0 changed; 11004932 unchanged)` — the sum
  equals the snapshot's 11,004,932 live accounts; the old code would have
  written all 11,004,932. `--execute` not needed for signers.

## Design Decisions

### Emerged

1. **Rule shared with the pool pass** (`pools::need`, thread 378): the same
   version comparison, already tested on all four cases, instead of a second
   copy; debt: the shared rule lives in `pools.rs`.
2. **Move and gate in one PR** (thread 379): the 20-line pass moved into its
   own module where it changed; `move-split-guard` did not object.

## Acceptance Criteria

- [x] Pass 4 emits only accounts whose snapshot ledger is above what we hold
- [x] `summary.txt` reports written and skipped-as-unchanged, summing to the
      live-account count
- [x] A dry-run against production reports **~0 written** and ~10.9M unchanged —
      hit exactly (2026-09-02, checkpoint 64,237,951): **0 written,
      10,909,433 unchanged**, and the sum equals the snapshot's live-account
      count to the row
- [x] The version read is sliced and its missing floor is explained in place
- [x] Regression test for the four gate cases
- [x] **Docs updated** — `docs/backfills.md` describes the seed pass; the
      `account_entry_state` line in its summary walkthrough changes shape.
      `docs/architecture/**` — N/A, no schema or contract change.
- [x] **API types regenerated** — N/A, nothing under `crates/api/**`.

Context, the site, the plan and the 2026-09-02 measurement are in
[notes/R-task-record.md](notes/R-task-record.md).
