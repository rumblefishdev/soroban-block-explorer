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

**Current state:** all five steps implemented and tested on branch
`fix/0559_release-review-follow-ups`; PR open, not deployed.

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

- [x] `+N` opens a keyboard-reachable list of every change with the same
      rendering; test covers opening by keyboard (Tab to `+2` on a row whose
      first entry is unlinkable → Enter → Tab lands on the first piece link →
      Escape returns focus)
- [x] The external-management warning is visible without hover
- [x] The assets-list cursor encodes the rank the key query sorted by; test
      covers a hydration value that differs from the key value
- [x] A known reserve key with an unreadable value logs an error naming the
      pool; test covers the key rule (the log line itself is not captured —
      the crate has no tracing test harness)
- [x] Branch checkout with an out-of-date `node_modules` re-syncs it; an
      up-to-date tree stays a fast no-op (measured 0.9 s)
- [x] **Docs updated** — N/A: no endpoint, schema, pipeline step or data
      contract changes; the cursor keeps its `(holder_rank, id)` shape
- [x] **API types regenerated** — run; the diff is empty

## Implementation Notes

- **Step 1** — `web/src/pages/accounts/BalanceChangeCell.tsx`: `+N` is a
  `Link component="button"` (`aria-label="+N more balance changes"`,
  `aria-expanded`, `aria-controls`) opening a MUI `Popover`. The `inverted`
  colour variants of `ChangeAmount` / `AssetLink` and `Muted`'s `variant` prop
  existed only for the dark tooltip surface and are removed.
- **Step 2** — the sentence moved from the chip's tooltip into the summary's
  Executable row (`ContractSummary.tsx`); the chip keeps its label.
- **Step 3** — `assets/queries.rs`: the list seek projects
  `coalesce(ba.holder_count, -1) AS holder_rank` and orders by the alias;
  `fetch_list` returns `ListedAsset { row, holder_rank }`, and
  `listed_asset_cursor` in the handler encodes that rank. Verified on
  production ClickHouse (read-only): the alias decodes as `Int32`.
- **Step 4** — `pool_state.rs`: `unread_reserve_keys` + one `tracing::error!`
  with the pool and the key names.
- **Step 5** — `.husky/post-checkout` no longer skips when `node_modules`
  exists. Measured: 0.9 s on an up-to-date tree (includes husky's `prepare`),
  0.01 s on a file checkout; on an npm-layout tree without `.modules.yaml`
  pnpm rebuilds without a prompt (probe project in a scratch directory).
- Tests: web 370 passed, `api` lib 283 passed, `xdr-parser` `pool_state` 16
  passed, clippy `-D warnings` clean on both crates. The 4 web lint warnings
  predate this branch.

## Design Decisions

### From Plan

1. **Popover, not a focusable tooltip trigger**: a tooltip's content lives in
   a portal Tab does not reach, so focusing the trigger alone would still
   leave the links unreachable.

### Emerged

2. **`+N` opens on click, no longer on hover**: one surface for mouse and
   keyboard instead of a hover tooltip plus a popover. Mouse users now click.
3. **The chip's tooltip is gone rather than made focusable**: the sentence is
   visible text in the summary, so a second copy on hover added nothing.
4. **Tests of `assets/queries.rs` moved to `queries_tests.rs` and
   `queries_decode_smoke.rs`**: the file was 1,504 lines, and the repo rule
   requires extracting tests from an over-limit file that a PR touches. It is
   now 1,196 lines — still over the limit, tracked by task 0525.
5. **A decodable empty `Reserves` is a read, not a refusal**: found while
   testing the rule — without it a pool with no liquidity yet would log an
   error on every write.
6. **The hook fix is `pnpm install` on every branch checkout, not
   `verifyDepsBeforeRun`**: narrower (only branch checkouts, not every
   `pnpm run` / `pnpm exec` everywhere including CI).

## Issues Encountered

- **Hook change cannot be exercised through `git switch` here**: worktrees
  start the MAIN checkout's hook copy, and the main checkout is on a branch
  from before the hand-over (task 0554 decision 13, bootstrap limit), so the
  new script was run directly with branch-checkout arguments instead.

## Notes

Rejected review findings and why are recorded in the PR description.
