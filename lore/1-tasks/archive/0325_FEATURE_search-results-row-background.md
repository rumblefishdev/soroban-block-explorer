---
id: '0325'
title: 'FEATURE: search page UX tweaks (result row background)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0323']
tags: ['frontend', 'ux', 'layer-web', 'priority-low']
links:
  - web/src/search/SearchResultRow.tsx
  - web/src/pages/SearchResultsPage.tsx
  - web/src/search/GlobalSearchBar.tsx
  - libs/ui/src/layout/SearchInput.tsx
  - libs/ui/src/layout/TopNav.tsx
  - web/src/router/AppShell.tsx
history:
  - date: 2026-06-25
    status: active
    who: fmazur
    note: >
      FE UX follow-up after 0323. Search results page rows render on a
      transparent background (only a bottom-border separator), so found items
      don't read as a surface. Give them a surface background like the tables.
  - date: 2026-06-25
    status: completed
    who: fmazur
    note: >
      Done. Wrapped results in a rounded outlined Paper (surface background +
      table-border color, NOT the nav dropdown's accent border) on
      SearchResultsPage; rows stay transparent on top of it so the found data
      reads as one block, header + rows on the same surface. typecheck + lint
      (web) pass. 2 files touched.
  - date: 2026-06-25
    status: completed
    who: fmazur
    note: >
      Follow-up: thinned the nav-bar search dropdown (GlobalSearchBar) border
      from 2px to 1px, keeping the yellow accent color (stroke.action). 1 file
      touched.
  - date: 2026-06-25
    status: completed
    who: fmazur
    note: >
      Follow-up: re-focusing the nav search field while it already holds a query
      now re-opens the results dropdown (it stayed dismissed after a click-away
      even though the text remained). Added SearchInput onFocus +
      data-search-input marker, plumbed onSearchFocus through TopNav to AppShell,
      and guarded GlobalSearchBar's ClickAwayListener so the focusing click
      doesn't immediately close the dropdown. 4 files touched.
---

# FEATURE: search page UX tweaks

## Summary

On `/search` the result rows sat on a transparent background — they didn't
stand out as a surface. Wrap the results (tab headers + rows) in a rounded,
outlined surface card so the found data reads as a distinct block, matching the
look of the nav-bar search dropdown but using the explorer's standard
table-border color instead of the accent (yellow) border.

## Implementation

- `SearchResultsPage`: wrap `<SearchResultsView>` in a `<Paper variant="outlined"
elevation={0}>` with `surface.grayMain` background, `stroke.default` border
  (1px), and `overflow: hidden` (so the rounded corners clip the rows).
- `SearchResultRow`: rows kept on a `transparent` background — the `Paper`
  wrapper provides the surface; hover / keyboard-highlight still lift to
  `grayHover`, bottom-border separators retained.

## Acceptance Criteria

- [x] Search result rows have a visible surface background (via the Paper wrapper).
- [x] Background extends over the tab headers too (whole block on one surface).
- [x] Rounded outlined border in the explorer's table-border color (not yellow).
- [x] Hover / keyboard highlight still distinguishable.
- [x] `nx typecheck` + `lint` (web) pass. Frontend-only, no API change.

## Design Decisions

### From Plan

1. **Surface lives on a wrapper, rows transparent**: rather than giving each
   row its own background, a single `Paper` provides the surface so headers and
   rows read as one block — same pattern as `GlobalSearchBar`'s dropdown.

### Emerged

2. **Table-border color, not the dropdown's accent border**: the nav dropdown
   uses `stroke.action` (yellow, 2px); on the full page the user wanted the
   quieter explorer table border, so the page Paper uses `stroke.default` (1px).

## Issues Encountered

- None. (A separate experiment — revealing the second navbar on dashboard
  scroll — was prototyped and then fully reverted at the user's request; it was
  out of scope for this task and left no residue.)
