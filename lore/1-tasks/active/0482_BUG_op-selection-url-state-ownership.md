---
id: '0482'
title: 'BUG: `#op-N` past the end blanks the operation — URL state has no owner'
type: BUG
status: active
related_adr: []
related_tasks: ['0453', '0460', '0462', '0377']
tags: [frontend, transaction-detail, url-state, priority-high, effort-small]
links: []
history:
  - date: '2026-08-10'
    status: active
    who: karolkow
    note: >
      Found during the post-deploy frontend verification sweep of the
      2026-07-25 → 2026-08-07 release window. Live on production: a
      `#op-99` fragment on a single-operation transaction replaces the
      operation card with a message that points at a picker which is not
      rendered for that transaction. Measured on production ClickHouse
      (ledgers 63 680 000–63 700 000, deduplicated): 5 369 984 of
      6 349 043 transactions carry exactly one operation — 84.6 %. Root
      cause is not the message: `useSelectedOp` normalises only the lower
      bound of a user-supplied index, so an out-of-range value escapes the
      state layer and each consumer copes differently.
  - date: '2026-08-13'
    status: active
    who: karolkow
    note: >
      REACHABILITY CORRECTED on review — the entry above overstates this.
      "84.6 %" is the share of transactions that render no picker, i.e. how
      bad the result looks once you land on it. It is NOT how often anyone
      lands on it, and the first entry reads as though it were. Checked
      afterwards: `#op-N` is produced in exactly ONE place in the whole app
      (`useSelectedOp`'s `setSelected`, from a picker click, always a valid
      index), no navigation carries the fragment between pages, and an
      operation count is immutable on-chain — so a shared `#op-3` link that
      worked once works forever. The out-of-range path is therefore reachable
      only by hand-editing the URL or pasting a fragment onto a different
      transaction's address.
      What is worth shipping is smaller and honest: the always-visible
      `1 CALLS` plural (a one-call trace is the common shape on a failed
      transaction), two unreachable branches, seven tests where there were
      none — and undoing the regression `4a31a2f7` introduced. Before that
      commit the rare path silently showed operation 1; that commit made it
      blank the section instead. Dropping this task would leave production
      in the worst of the three states, which is the only reason the
      edge-case handling still ships.
---

# BUG: `#op-N` past the end blanks the operation — URL state has no owner

## Summary

`useSelectedOp` reads the user-supplied `#op-N` fragment and clamps only the
lower bound (`Math.max(0, N - 1)`). It cannot check the upper bound because it
does not know how many operations the transaction has, so an out-of-range index
leaves the hook and every consumer defends against it on its own. The visible
consequence is that `#op-99` on a single-operation transaction hides the only
operation behind a message telling the reader to "pick one from the list" —
a list that is not rendered for single-operation transactions, which is 84.6 %
of them.

**How often does anyone hit it?** Rarely, and that is worth stating up front
rather than burying. `#op-N` has exactly one producer in the app — a picker
click, always a valid index — nothing carries the fragment across pages, and
an operation count never changes, so a link that worked once keeps working.
Reaching the bad path means hand-editing the URL. The 84.6 % above is the
blast radius, not the incidence.

The case still ships fixed because `4a31a2f7` made it WORSE than it was:
before that commit a bad fragment silently showed operation 1, after it the
section went blank. Leaving this alone means leaving production in the worst
of the three states. The changes with everyday value are the ones that ride
along: the `1 CALLS` plural, two unreachable branches, and the first tests
this hook has ever had.

## Context

The repo already has a convention for user-supplied URL state, and this hook is
the one place that breaks it. `useTableUrlState` normalises at the point of
reading (`libs/ui/src/table/useTableUrlState.ts`):

```ts
const sortDir: SortDirection =
  rawDir === 'asc' || rawDir === 'desc' ? rawDir : defaultSortDir;
```

`?dir=sideways` becomes the default **inside the hook**. Nothing downstream sees
an invalid direction, and nothing downstream carries a guard for one.

`useSelectedOp` half-applies the same idea and then hands the problem on. The
decision then landed in `OperationsSection`, where it became a _content_
branch — render a message instead of the operation — rather than a _state_
correction.

### History of the regression

Before `4a31a2f7` the section read `entries[selectedIndex] ?? entries[0]`: a bad
fragment silently showed operation 1 while the URL and the card label claimed
another number. That was a genuine defect (a silent substitute reads as an
answer). `4a31a2f7` replaced it with an explicit out-of-range message and the
comment "leave the picker to recover from" — without checking that the picker
exists. Eight lines below, in the same function, sits the comment stating that
87 % of transactions have one operation and therefore get no picker.

The fix traded a silent substitution for a silent omission. This task fixes the
ownership instead of either symptom.

## Symptoms, all from the one root

| #   | Symptom                                                                                                                                                                                 | Site                    |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| 1   | Out-of-range fragment blanks the only operation                                                                                                                                         | `OperationsSection.tsx` |
| 2   | `selectedIndex < 0` guard is unreachable — the hook already clamps low                                                                                                                  | `OperationsSection.tsx` |
| 3   | `OperationPicker`'s "No operations in this transaction." branch is unreachable — the section returns `UnavailableSection` at `entries.length === 0` and only renders the picker above 1 | `OperationPicker.tsx`   |
| 4   | On a multi-operation transaction a bad fragment highlights nothing (`index === selectedIndex` never matches)                                                                            | `OperationPicker.tsx`   |
| 5   | No test covers either the hook or the out-of-range branch                                                                                                                               | —                       |

## Investigated and rejected

**"`entries` can be shorter than `operation_count`, so the message states a
false count."** Refuted. `buildOperationEntries` maps 1:1 over
`tx.heavy.operations`, and the API drops an operation only when its index
exceeds `i16::MAX` (`to_i16_index`, `extractors.rs`). Stellar caps a
transaction at 100 operations, so the path is unreachable. The `filter_map`
does log a warning if it ever fires, so it is observable rather than silent.

## Implementation

`useSelectedOp(count)` becomes the owner of the index:

- a pure `resolveOp(hash, count)` does the work and is directly testable
- returns `{ index, missing }` — `index` always addresses an existing
  operation, `missing` carries the 1-based number the URL asked for when that
  operation does not exist
- `count <= 0` reports nothing: 0 also means "still loading" and "archive fetch
  failed", and answering it would assert a count nobody measured (0377)
- the fragment is **not** rewritten — the address bar keeps what the reader
  asked for, and the section states the miss, so the correction is visible
  rather than silent

`OperationsSection` drops both the range guard and the message-instead-of-card
branch, renders the operation, and shows the miss as a notice above it.
`OperationPicker` loses its unreachable empty branch.

## Acceptance criteria

- [x] `#op-N` past the end renders the operation AND names the number that does
      not exist — nothing hidden, nothing silently substituted
- [x] Single-operation transactions (84.6 % of traffic) recover without a
      picker; no copy points at a control that is not rendered
- [x] `count === 0` (loading / archive unavailable) makes no claim about
      whether the operation exists
- [x] Both unreachable branches removed (`selectedIndex < 0`, picker's empty
      state)
- [x] `resolveOp` unit-tested: absent, valid, past-the-end, zero,
      non-numeric, and unjudgeable (`count === 0`) fragments
- [ ] Verified live on production against the reported transaction
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md` §6.4
      describes the `#op-N` contract
- [x] **API types regenerated** — N/A, nothing under `crates/api/**`

## Future Work

- The assets list answers an API `400` (an unrecognised `filter[type]` value
  reached from a stale or hand-edited URL) with "An unexpected error occurred
  while rendering this section", which blames rendering for a rejected filter.
  Same family — user-supplied URL state with no validation at the boundary —
  but a different surface; spawn separately rather than widening this task.
