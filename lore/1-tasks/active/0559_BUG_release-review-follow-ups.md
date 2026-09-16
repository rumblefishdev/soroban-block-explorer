---
id: '0559'
title: 'BUG: release review follow-ups — keyboard access to balance changes, cursor snapshot, silent reserve layout, stale node_modules'
type: BUG
status: active
related_adr: []
related_tasks: ['0540', '0547', '0548', '0374', '0554']
tags:
  [
    frontend,
    api,
    xdr-parsing,
    tooling,
    accessibility,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: '2026-09-16'
    status: active
    who: karolkow
    note: >
      Task created from the automated review of release PR #453
      (production-2026.09.16-1). Each finding was verified against the code
      before it was accepted; the documentation-only findings landed directly
      on develop, these five change behaviour and go through a PR.
---

# BUG: release review follow-ups

## Summary

Five findings from the review of release PR #453 that survived verification
against the code and change behaviour: two keyboard-access gaps in the
frontend, a cursor built from a different snapshot than the page it closes, a
silent reserve-layout refusal in the pool parser, and a post-checkout hook that
leaves an out-of-date `node_modules` in place.

## Status: Active

**Current state:** filed; implementation not started.

## Context

The review raised 25 findings. Seven were rejected or already handled, eleven
were documentation corrections committed straight to `develop` (task 0548,
0540 notes, ADR 0058, 0325, 0520, 0530, `docs/backfills.md`,
`docs/backups.md`, the XDR parsing overview). The backfill sink's lost
executable updates were measured and recorded in task 0548's follow-ups, and
task 0210's stale plan is corrected on its own PR #460. The five below change
what the system does.

## Implementation Plan

### Step 1: `+N` balance changes reachable by keyboard (task 0540 cell)

`web/src/pages/accounts/BalanceChangeCell.tsx` hides every change after the
first in a hover `Tooltip`. Its asset and NFT links cannot be reached by
keyboard, and a row whose first change has no link has nothing focusable at
all. Make `+N` a button that opens a `Popover` with the same list; the Popover
moves focus in and restores it on close.

### Step 2: "Externally managed" warning visible (task 0548 chip)

`web/src/pages/ContractDetailPage.tsx` puts "changing it there changes every
contract that uses the same tag" only in a hover tooltip on a non-focusable
chip. Show the sentence as text where the owner and tag are already shown.

### Step 3: assets-list cursor from the page's own snapshot (task 0547)

`crates/api/src/assets/handlers.rs` builds the `(holder_rank, id)` cursor from
`holder_count` re-read in the hydration query, while the page was selected by
`coalesce(ba.holder_count, -1)` in the key query. A `balance_aggregates`
rebuild between the two queries moves the cursor off the page boundary and
duplicates or skips rows in the tie-heavy tail. Carry the rank from the key
query into the cursor.

### Step 4: log a present-but-unreadable reserve key (task 0374)

`crates/xdr-parser/src/pool_state.rs` treats a known reserve key with an
unexpected value type the same as a missing key: no row, no log unless the
plane happens to write in the same ledger. Log it with the pool identity.

### Step 5: post-checkout re-syncs a stale `node_modules` (task 0554)

`.husky/post-checkout` exits when `node_modules` exists, so a lockfile change on
branch switch, or an npm-layout tree left from before the migration, is never
repaired and the pre-commit gate fails inside nx. Measure pnpm's no-op cost
first; install when out of date.

## Acceptance Criteria

- [ ] `+N` opens a keyboard-reachable list of every change with the same
      rendering; test covers opening by keyboard
- [ ] The external-management warning is visible without hover
- [ ] The assets-list cursor encodes the rank the key query sorted by; test
      covers a hydration value that differs from the key value
- [ ] A known reserve key with an unreadable value logs an error naming the
      pool; test covers it
- [ ] Branch checkout with an out-of-date `node_modules` re-syncs it; an
      up-to-date tree stays a fast no-op (measured)
- [ ] **Docs updated** — N/A unless a step changes a described contract
- [ ] **API types regenerated** — `crates/api/**` changes; run
      `pnpm nx run @rumblefish/api-types:generate`, expected empty diff

## Notes

Rejected review findings and why are recorded in the PR description.
