---
id: '0060'
title: 'UI lib: global search bar component'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0086']
tags: [priority-high, effort-medium, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-15
    status: active
    who: FilipDz
    note: 'Promoted on feat/0060_0086_search — bundled with 0086 since both consume the same /v1/search endpoint and same redirect-on-redirect logic.'
  - date: 2026-05-19
    status: completed
    who: FilipDz
    note: 'Implemented as web/src/search/ connector mounted via new TopNav `searchOverlaySlot` prop. Tabbed dropdown per Figma node 2147:895 (file siumLgKOc9LLepEfbimyp3). API extended in the same PR — `SearchHit.successful` + `SearchHit.last_activity_at` added to `crates/api/src/search/dto.rs` and joined from `transactions` via `(hash, created_at)` for transaction hits; other entity types pass NULL until per-entity activity joins land.'
---

# UI lib: global search bar component

## Summary

Implement the global search bar component in `libs/ui/src/search/`. This is the primary entrypoint for "known identifier lookup" journeys. It accepts diverse identifier types, provides debounced type-ahead suggestions, and on submit either redirects to an exact-match detail page or navigates to the search results page.

## Status: Backlog

**Current state:** Not started.

## Context

The global search bar is the most prominent interactive element in the explorer, visible on every page via the header. Users need to quickly look up transaction hashes, contract IDs, account addresses, token codes, ledger sequences, pool IDs, and NFT identifiers. The search bar must handle both exact-match lookups (direct redirect) and fuzzy/broad queries (search results page).

API endpoint consumed: `GET /search?q=&type=transaction,contract,token,account,nft,pool`

Accepted input types:

- Transaction hashes (64-char hex)
- Contract IDs (C... format)
- Account IDs (G... format)
- Token codes/names (string)
- Ledger sequences (positive integer)
- Pool IDs (string)
- NFT identifiers (string)

## Implementation Plan

### Step 1: Search bar UI component

Create `libs/ui/src/search/GlobalSearchBar.tsx`:

- Text input with search icon, prominent styling
- Placeholder text indicating accepted types (e.g., "Search by tx hash, account, contract, token...")
- Responsive width, fills available header space

### Step 2: Debounced type-ahead suggestions

- Debounce input at approximately 300ms before firing suggestion queries
- Display suggestion dropdown with entity type indicators
- Suggestions fetched from `GET /search?q=` with the current input
- Each suggestion shows identifier (truncated) and entity type badge

### Step 3: Submit behavior and navigation

- On form submit (Enter key): navigate immediately without waiting for suggestions
- Exact match logic: if API returns a single high-confidence result, redirect to the detail page:
  - Transaction: `/transactions/:hash`
  - Contract: `/contracts/:id`
  - Account: `/accounts/:id`
  - Token: `/tokens/:id`
  - Ledger: `/ledgers/:seq`
  - Pool: `/liquidity-pools/:id`
  - NFT: `/nfts/:id`
- Otherwise: navigate to `/search?q=<encoded_query>`
- On suggestion click: navigate directly to that entity's detail page

### Step 4: Keyboard interaction

- Arrow keys navigate suggestions
- Enter selects highlighted suggestion or submits raw query
- Escape closes suggestion dropdown
- Tab moves focus out of search bar

### Step 5: Exports

Export `GlobalSearchBar` from `libs/ui` barrel.

## Acceptance Criteria

- [x] Search bar renders in header — Karol's `SearchInput` mounted in `TopNav`; the suggestions overlay (this task) sits beneath it via the new `searchOverlaySlot` prop on TopNav.
- [x] Accepts all identifier types — `/v1/search` handles classification; frontend doesn't pre-validate.
- [x] 300ms debounce — `useDebounced(q, 300)` in `web/src/search/useDebounced.ts`, consumed by `useSearchResults`.
- [x] Form submit navigates immediately — AppShell's existing `handleSearchSubmit` still fires; the connector intercepts via an `enterHandlerRef` only when a row is highlighted, otherwise falls through to `navigate(routes.search(q))`.
- [x] Exact-match redirect — `data.type === 'redirect' ⇒ navigate(routeForHit(data), { replace: true })` in `GlobalSearchBar` + same logic in `SearchResultsPage` for direct `/search?q=` links.
- [x] Non-exact → `/search?q=` — AppShell submit handler.
- [x] Keyboard a11y — `ArrowDown` / `ArrowUp` cycle hits in the active tab, `Enter` selects highlighted hit (or falls through to submit), `Escape` dismisses the dropdown, `Tab` leaves naturally. MUI `<Tabs>` handles tab keyboard nav.
- [x] Dropdown closes on blur or Escape — `ClickAwayListener` wraps the dropdown body; Escape handler in `GlobalSearchBar.handleKeyDown`.
- [x] Component lives in `web/src/search/` — connector knows about React Router + API codegen, so it's web-app-side (not libs/ui). The reusable `SearchResultsView` could move to libs/ui later if a second consumer emerges; for now both call sites are in web/.

### Tabbed dropdown additions (Figma node 2147:895, beyond original spec)

- [x] MUI `<Tabs>` row across dropdown top with one tab per `EntityType`
- [x] Per-tab count badge using `<Chip size="sm" color="accent/neutral">`
- [x] Active tab highlighted via existing `MuiTabs` theme override
- [x] Auto-switch to first tab with hits when results arrive (UX: don't land on empty Transactions when user searched for an asset)
- [x] Yellow-bordered `Paper variant="outlined"` with `borderColor: stroke.action` matches Figma's yellow card frame

## Notes

- The search results page is task 0086; this component handles the input and navigation trigger.
- The layout shell (task 0059) provides the slot where this component is rendered.
- API interaction should use TanStack Query setup from task 0066 for suggestion fetching.
