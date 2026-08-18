---
id: '0491'
title: 'FEATURE: pool activity is a list of operations, with a trades filter'
type: FEATURE
status: completed
related_adr: ['0032']
related_tasks: ['0279', '0482', '0489', '0490']
tags:
  [
    api,
    frontend,
    layer-backend,
    layer-frontend-pages,
    priority-medium,
    effort-medium,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/371'
history:
  - date: '2026-08-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 0279 release review, and deliberately scoped to hold
      BOTH remaining halves of issue #371 — the row unit and the trades filter
      — because they need the same API change and splitting them would pay for
      that migration twice.
  - date: '2026-08-17'
    status: active
    who: stkrolikiewicz
    note: >
      Activated, with the sequencing decided at the same time. 0490 is NOT done
      first — this task's AC already requires its cap to be removed or proven
      dead, so patching the row height beforehand writes code only to delete it.
      0279 stays open deliberately: its remaining data-side criteria (multi-pool
      Horizon check, `read_rows` measurement) validate figures this task merely
      re-presents, while its "Docs updated" and "API types regenerated" items
      are left for this task to satisfy — the response shape changes here, so
      doing them under 0279 would mean writing them twice.
  - date: '2026-08-18'
    status: completed
    who: stkrolikiewicz
    note: >
      All three steps shipped and verified on prod data (PR #421, 15 commits).
      284 rust / 280 web / 82 ui tests green. The read_rows criterion earned
      its place: it rejected the first implementation (GROUP BY pivot, 2.60M
      rows vs the old endpoint's 159k) and forced the key-order read + pair-in-
      Rust shape (115k / 9 ms, medians of 3 — cold runs of either shape read
      0.7-1.0M). Comparing the rendered page against stellar.expert caught
      source_account carrying the transaction's source — wrong for the 41% of
      ops that declare their own; fixed off the appearance seek, which also
      yields pools_crossed for the route badge. UX pass added linked assets,
      the execution rate and hideBelow on ExplorerTable (1020→860px below sm).
      0490 archived as superseded: the stacking cell it capped was deleted
      whole, its width criterion carried here. Failed explicit LP ops are no
      longer listed (driver-table change) — deliberate, flagged in the PR.
---

# Pool activity is a list of operations, with a trades filter

## Summary

Change the unit of a "Recent transactions" row on the pool detail page from a
transaction to an operation, and add the trade / deposit / withdrawal filter
issue #371 asked for. One change, because the filter cannot be built honestly
on the current row unit.

## Context

[0279](../active/0279_FEATURE_lp-op-details-amount-column.md) answered the
first third of issue #371 — the amounts are on the list now, no detail-page
hop. Two thirds are open, and both are blocked on the same thing.

The reporter's ask was to match stellar.expert's pool view, and pointed at
`?filter=trades`. That view lists **trades**, not transactions holding trades.

Everything awkward about the current table traces back to the row unit:

- **The `Event` chip cannot be honest.** `classifyLpTx(row.operation_types)`
  collapses a transaction's operation types into one chip; a bundled deposit +
  trade gets a chip that is wrong for one of them.
- **The Amount cell has to stack** (see
  [0490](./0490_BUG_pool-amount-cell-row-height-unbounded.md)) because one row
  holds several figures that must not be summed.
- **A trades filter is not expressible.** What does "trades only" return for a
  transaction that deposits and trades? Every answer is a lie: include it and
  the list is not trades, exclude it and a real trade vanishes.
- **The count in the pager means transactions**, which is not the number the
  page is about.

Per-operation rows dissolve all four. And the identity that makes it navigable
already shipped: [0482](../archive/0482_BUG_op-selection-url-state-ownership.md)
gave every operation a URL-addressable `#op-N` anchor on the transaction detail
page, so each row has a real destination.

## Implementation

### Step 1: API — the page is operations

`/liquidity-pools/:id/transactions` returns one item per (transaction,
operation) against the pool, with the cursor keyed on
`(ledger_sequence, transaction_id, application_order)` rather than on the
transaction. `lp_operation_amounts` and `operation_pools` are already keyed
that way, so this removes the per-transaction grouping rather than adding
work. Decide at this point whether the path gets a new name — `/activity`
reads truer than `/transactions` — and whether the old shape needs a
deprecation window.

### Step 2: API — the filter

`filter[event]` over operation type: trades, deposits, withdrawals. One
predicate on a per-operation row.

### Step 3: Frontend

One line per row, so the Amount cell stops stacking and the `Event` chip
becomes accurate by construction. Row links to the operation's `#op-N` anchor.
Filter control on the section header.

## Acceptance Criteria

- [x] One row per operation; `Event` chip describes exactly that operation —
      verified live: tx `bacf6237` renders its 4 pool ops as 4 rows, one chip
      each
- [x] Amount cell renders a single figure — the stacking case is gone, not
      merely capped: the per-transaction component was deleted whole; DOM
      check shows one line per cell
- [x] Each row links to its operation's `#op-N` anchor on the transaction
      detail page — verified `#op-1/3/4/5` on one hash, with the gap where
      op 2 did not touch the pool
- [x] `filter[event]` returns trades / deposits / withdrawals, and the mixed
      bundle that motivated this appears under each type it actually contains
      — by construction on per-operation rows (the chip's event IS the filter
      predicate); verified live over `?event=` on prod data
- [x] Pagination is stable across the new cursor, including at a page boundary
      that falls inside a multi-operation transaction — verified on prod, see
      [Verification](#verification-on-prod-2026-08-18)
- [x] Read path stays a PK-prefix seek with the same partition prune — measure
      `read_rows` before and after; more rows per page must not mean a scan —
      measured, and it caught a regression; see below
- [x] 0490's cap is removed or confirmed dead, not left as unreachable code —
      stronger: the cell it would have capped no longer exists. 0490 archived
      as superseded; its column-width criterion (two-leg form, no wrap) carried
      here and verified at width 320
- [x] **Docs updated** — canonical SQL `24_get_liquidity_pools_activity.sql`
      (header block, the do-not-reintroduce-GROUP-BY measurement table, the
      op-source step), reference-set README index, FINAL-discipline row for
      `lp_operation_amounts`; SQL 20 retired to `.trash/`
- [x] **API types regenerated** — at every shape change; post-merge regen
      produces no diff

## Verification on prod (2026-08-18)

Run against `sorban-prod` / `app-clickhouse-1` as read-only SELECTs, on the
pool with the most recent activity (`7a042a04…0e6e`, 1.68M leg rows).

### read_rows — the measurement rejected the first implementation

Returning 21 operations. **Medians of 3**: a cold run of _either_ shape reads
0.7–1.0M rows, so one sample each inverts the comparison.

| shape                                            | read_rows   | ms    | memory     |
| ------------------------------------------------ | ----------- | ----- | ---------- |
| `GROUP BY` pivot (first cut)                     | 2 597 380   | 109   | 182 MiB    |
| + `optimize_aggregation_in_order`                | 2 597 297   | 253   | 230 MiB    |
| + `FINAL`                                        | 3 174 852   | 110   | 195 MiB    |
| **read-in-order + pair in Rust (shipped)**       | **114 888** | **9** | **11 MiB** |
| per-transaction endpoint 20 (what this replaces) | 159 021     | 11    | 11 MiB     |

The first implementation was a **regression** against the endpoint it
replaces: a `GROUP BY` must consume the pool's whole slice before
`ORDER BY … LIMIT` can pick the newest 21. Reading in sort-key order stops at
the window, because `asset_id` is the last key component and an operation's two
legs are therefore adjacent — Rust folds them without an aggregation.

Two hypotheses died here and are recorded so they are not retried: `FINAL` was
never the cost (+22%, not an order of magnitude), and
`optimize_aggregation_in_order` bought nothing while doubling latency.

### Cursor at a boundary inside a multi-operation transaction

Transaction `4849775023734824275` in ledger `64007288` runs **11 operations**
against this pool, occupying positions 4–14, so any page shorter than 13 splits
it. Walking 4 pages of 5 with the shipped keyset returned 20 rows, 20 distinct,
byte-identical to the top-20 taken in one shot — no duplicates, no gaps. Pages 3
and 4 both open and close _inside_ that transaction (cursors at `ao` 14, 9, 1).

The test is not a tautology. The same walk with a transaction-level keyset
`(ledger_sequence, transaction_id)` — the shape the retired endpoint's cursor
used — jumps from `ao = 13` straight to the next ledger, silently dropping the
remaining 12 operations of that transaction. Carrying `application_order` in the
cursor is what the per-operation row unit requires.

## Implementation Notes

- **API** (`crates/api/src/liquidity_pools/`): `/activity` replaces
  `/transactions` outright — handler, DTO, query and canonical SQL 20 deleted
  in the same PR that moved the frontend, so no dead route shipped. Driver is
  `lp_operation_amounts` read in sort-key order (no GROUP BY, no FINAL), legs
  paired in Rust; one bounded `operations_appearances` seek resolves the op
  source and `pools_crossed`; `resolve_accounts` covers both source kinds in
  one call.
- **Frontend** (`web/src/pages/pool-detail/PoolActivity.tsx`): parts-based
  Amount cell (digits + `AssetIcon` + `legHref`-linked code), execution rate
  line, `1 of N pools` chip, `Select`-with-All filter in the URL, rows keyed
  `hash-application_order`. `classifyLpTx` is gone.
- **Shared** (`libs/ui`): `ExplorerTableColumn.hideBelow` — filtered from all
  render sites and the `minWidth` sum.
- Tests added: 16 on the component, 3 on `PoolEvent`, 2 on `hideBelow`
  (matchMedia stubbed).

## Issues Encountered

- **The GROUP BY regression** — first cut aggregated the pool's whole slice
  to return 21 rows (2.60M read_rows / 182 MiB). `optimize_aggregation_in_order`
  did not help (same rows, 253 ms); `FINAL` was never the cost (+22%). The AC's
  before/after measurement is what caught it.
- **Cold-run variance** — either shape reads 0.7–1.0M rows cold, so single
  samples invert comparisons; every recorded figure is a median of 3.
- **`source_account` named the wrong account** — carried the transaction's
  source onto per-operation rows; 41% of ops in a recent window declare their
  own. Found only by comparing the rendered page against stellar.expert.
- **Serde-typed filter param** — deserializing `filter[event]` into the enum
  made axum answer plain-text 400s instead of this API's `ErrorEnvelope`;
  validated in the handler like the chart's `interval`.
- **Worktree symlink resolution** — `node_modules` links through the primary
  checkout, so typecheck/dev consumed stale `api-types`/`ui` until the
  symlink was flipped per commit and restored after.
- **`format:check --all` red from develop** — an archived task file arrived
  unformatted (`e3bcdad9`), reddening every downstream PR; fixed here.
- **Browser-pane emulation artifact** — a live viewport resize without reload
  did not re-add the hidden column; real Chrome behaves (user-verified).

## Design Decisions

### From Plan

1. **`/activity` replaces `/transactions` with no deprecation window** — the
   API sits behind the Turnstile gate with no external consumers; the old
   shape died in the same PR the frontend moved.
2. **Cursor carries `application_order`** — a stale `/transactions` cursor
   fails deserialization into the new payload and answers `invalid_cursor`
   with no explicit source guard.

### Emerged

3. **Driver table = `lp_operation_amounts`** — `operation_pools` has no
   `application_order`, so it cannot page per operation. Consequence, flagged
   as a product call in the PR: a failed explicit LP op is no longer listed
   (it moved nothing; narrows a known CH-vs-Horizon breadth difference).
4. **Sign pair classifies, in Rust** — `+/+` deposit, `-/-` withdrawal, else
   trade (`PoolEvent::from_signs`), one implementation for chip and filter;
   moved out of SQL by the perf rewrite, which made it unit-testable.
5. **Key-order read + pair over SQL aggregation** — 22× the first cut, and
   slightly under the endpoint it replaces.
6. **Filter fills by geometric window growth** — matching rate is unknowable
   before pairing; O(log) round trips, no schema change.
7. **Op-source with tx fallback** — absent XDR `sourceAccount` means "the
   transaction's", so the fallback is semantics, not a guess.
8. **Event icon per event, not per Stellar op type** — a type icon needs a
   join this task measured itself out of, and would contradict the chip.
9. **`Select` with "All events"** after a `ToggleButtonGroup` first cut — the
   app's filter convention, with a visible way back.
10. **`pools_crossed` badge instead of inline route** — the route stays on
    the op detail page the row links to.
11. **`hideBelow` at the lib level** — honest `minWidth`, opt-in for every
    table; card rows deliberately deferred to 0366.

## Future Work

Surfaced, deliberately not spawned (task creation is user-gated in this
project): per-row USD value (needs a measured price join), inline route
rendering (needs indexed path data — runtime XDR per page was measured out),
expandable rows (belongs with 0366's card work), `hideBelow` adoption on the
other wide tables. The mobile card-row variant is already noted on
[0366](../active/0366_REFACTOR_detail-tables-onto-datalistcard.md).
