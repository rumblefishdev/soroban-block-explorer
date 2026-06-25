---
id: '0323'
title: 'FEATURE: table loading skeleton on pagination / filter change (+ FE UX polish)'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: ['frontend', 'ux', 'react-query', 'layer-web', 'priority-medium']
links:
  - web/src/pages/detail/DataListCard.tsx
  - libs/ui/src/table/ExplorerTable.tsx
  - web/src/router/AppShell.tsx
history:
  - date: 2026-06-24
    status: active
    who: fmazur
    note: >
      Spawned from a UX gap: list tables refetch on next/prev and on filter
      change, but the user gets no feedback. `listPolicy`/`detailPolicy` use
      `placeholderData: keepPreviousData`, so old rows stay and `isLoading`
      stays false — no skeleton. Show a skeleton while the new page/filter
      data is in flight.
  - date: 2026-06-25
    status: completed
    who: fmazur
    note: >
      Shipped the loading-skeleton-on-reload plus a broader FE-UX polish batch
      (~35 web + libs/ui files). nx typecheck + lint (web + ui) green;
      frontend-only, no API/types change. Verified visually incl. mobile
      (iPhone 14 Pro Max). Core decision: render the skeleton via the REAL
      `ExplorerTable` in a new `loading` mode (real headers + fixed px columns)
      so it matches the populated table at every viewport — no height jump, no
      column shift, real headers during reload.
---

# FEATURE: table loading skeleton on pagination / filter change (+ FE UX polish)

## Summary

When a user clicks next/prev or changes a filter on a list table, the data
refetches silently — old rows stay on screen (`placeholderData:
keepPreviousData`) and `isLoading` is `false`, so there is no loading feedback.
Show a loading skeleton while the new page/filter data is being fetched. The
task then grew into a broader frontend UX polish pass on the tables + shell.

## Context

- React Query v5; the precise "key changed, fetching new data" signal is
  `isPlaceholderData` (true only while showing previous data for a changed
  query key; false on the first load and false on a same-key background
  refetch like window-focus / polling — so no spurious flashing).
- Keep `keepPreviousData` so `data.page` (next/prev cursors) stays available to
  drive the pagination buttons while the skeleton shows.

## Acceptance Criteria

- [x] Next/prev and filter changes show a skeleton until the new data arrives
      (`isPlaceholderData` → `DataListCard isReloading` / inline sub-list guard).
- [x] No skeleton flash on background refetch (window focus / live poll) — live
      home feeds use `livePolicy` (no `keepPreviousData`).
- [x] Pagination buttons keep working (cursors preserved during the fetch).
- [x] Skeleton matches the populated table 1:1 at every viewport (height,
      columns, headers, alternating row bg) — responsive, no jump/shift.
- [x] `nx typecheck` + `lint` for `web` + `@rumblefish/...-ui` pass.
      **Docs / API types**: N/A (frontend-only, no API contract change).

## Implementation Notes

**Skeleton = the real table in a `loading` mode (the key idea).**

- `ExplorerTable` (`libs/ui`) gained `loading?` + `skeletonRows?`. When loading
  it renders the REAL `<TableHead>` + N placeholder body rows in the same
  table / container / column layout. Reusing the real structure (not a separate
  `TableSkeleton`) is what makes the skeleton match the populated table at every
  viewport — same headers, same column widths, same horizontal-scroll behaviour.
- `DataListCard` gained `renderSkeleton?: () => ReactNode`; the 7 list pages
  pass `renderSkeleton={() => <XxxTable rows={[]} loading skeletonRows={N} />}`.
  The generic `TableSkeleton` stays only as a fallback + the route Suspense
  fallback (`ListPageSkeleton`).
- Reload trigger: `DataListCard isReloading={isPlaceholderData}`; detail
  sub-lists guard `if (isLoading || isPlaceholderData)` and render the same
  `ExplorerTable loading`.
- Each table component (`*Table` + detail sub-lists) got `loading?`/
  `skeletonRows?` pass-through.

**Responsive columns (no truncation, no shift).**

- `ExplorerTable` uses `table-layout: fixed`; every `ExplorerTableColumn` has a
  PIXEL `width` sized to its content; `ExplorerTable` computes the table
  `minWidth` as the SUM of those px. Result: columns are content-sized (no
  truncation), the table scrolls horizontally on narrow screens (instead of
  squeezing/wrapping), and the skeleton columns line up with the data columns.
- Cells are `whiteSpace: nowrap` + `overflow: hidden` + `textOverflow: ellipsis`.
- Skeleton bars are `display: inline-block` so they follow each column's
  `align` (right-aligned numeric columns get the bar on the right, under the
  data — not stuck on the left).
- Skeleton rows carry the same alternating `grayMain` / `grayMainAlt` background
  as the data rows.
- Two-line / media tables pinned to `EXPLORER_TABLE_ROW_HEIGHT_TALL` (56) so the
  skeleton row height matches; single-line stay 44.

**Dashboard (first-load) fixes.**

- The "N latest records" footer is rendered during loading too (skeleton count)
  so the card doesn't jump when it appears with the data.
- `HomeSkeleton` (route fallback) now renders the REAL `LatestTransactionsTable`
  / `LedgersTable` in `loading` mode + the `PollingIndicator` header line, so
  the "Updated just now" line and the row heights don't appear/jump on mount.

**Per-column width fixes from visual review.**

- Operation column widened (badge `OperationCell` was clipped) across the 4
  tables that use it. Fee column widened (`formatFee` appends " XLM"). Accounts
  `account` column widened for the home-domain chip, with the other 3 columns
  scaled by the same factor so the desktop proportions are unchanged.

**Other shell / UX changes done in the same batch.**

- Ledger HASH renders mono (`IdentifierDisplay` gained a `mono?` override) so
  the truncated `XXXX…XXXX` is fixed-width and the copy buttons line up.
- `AppShell`: both nav bars are `position: sticky; top: 0`.
- `AppShell`: scroll to top on route (path) change only — pagination/filter
  change just the `?search` params, so they keep the scroll position.
- Footer Rumblefish logo links to `https://www.rumblefish.dev` (new tab) via a
  new `href`/`external` on the shared `HomeLogo`.

## Design Decisions

### From Plan

1. **`isPlaceholderData` as the reload signal** — fires only on a query-key
   change (page/filter), not on same-key background refetch, so no flashing.
2. **Keep `keepPreviousData`** — cursors stay available for the pagination
   buttons while the skeleton shows.

### Emerged

3. **Skeleton via the real `ExplorerTable` (`loading` mode), not the generic
   `TableSkeleton`.** Started with a fixed-px-height generic skeleton; it
   drifted on small screens (headers wrap, row heights change). Rendering the
   real table structure is the only way to match responsively. (User confirmed
   full skeleton replacement, not a dim overlay.)
4. **`table-layout: fixed` + per-column PIXEL widths + summed `minWidth`.**
   Auto-layout = column shift (skeleton ≠ data widths); fixed % = truncation on
   mobile. Fixed px columns + horizontal scroll satisfy both (no shift, no
   truncation). Accounts columns scaled uniformly to preserve desktop
   proportions while fitting the domain chip on mobile.
5. **`mono?` override on `IdentifierDisplay`** — `type="ledger"` is non-mono
   (sequence numbers), but the ledger HASH cell needs mono for copy-button
   alignment.
6. **Sticky navs / scroll-to-top / footer link** — extra polish the user asked
   for in the same session; bundled here rather than spawning separate tasks.

## Issues Encountered

- **fixed-layout truncation:** moving to `table-layout: fixed` first used `%`
  widths → narrow columns (status/fee) truncated on mobile. Fixed by switching
  to content-sized PIXEL widths + `minWidth = Σpx` (horizontal scroll).
- **accounts column too wide on desktop:** bumping the `account` column to px
  for the mobile chip made it dominate on desktop (fixed-layout scales by px
  ratio). Fixed by scaling ALL accounts columns by the same factor (proportions
  unchanged).
- **only the ledgers hash misaligned:** `type="ledger"` skipped mono; other
  hashes (`type="transaction"`) were already mono. Hence the `mono?` override.

## Future Work

- Route Suspense fallback (`ListPageSkeleton`) still uses the generic
  `TableSkeleton` (skeleton-bar headers, approximate). One-time first-load only;
  could be switched to per-page real-table skeletons if desired (bundle cost).
