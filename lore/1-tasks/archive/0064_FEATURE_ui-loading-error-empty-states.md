---
id: '0064'
title: 'UI lib: loading skeletons, error states, empty states'
type: FEATURE
status: completed
related_adr: []
related_tasks: []
tags: [priority-high, effort-small, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-14
    status: completed
    who: FilipDz
    note: 'Implemented on feat/0058_ui-mui-theme alongside 0058. All 9 ACs satisfied.'
---

# UI lib: loading skeletons, error states, empty states

## Summary

Implement loading, error, and empty state components in `libs/ui/src/states/`. These components ensure every data-dependent section in the explorer degrades gracefully during loading, on failure, and when no data exists. Critically, each section has an independent error boundary so one failed section does not collapse unrelated sections on the same page.

## Status: Backlog

**Current state:** Not started.

## Context

Explorer pages frequently load multiple independent data sections (e.g., account summary + balances + transactions). Each section can be in a different state: loading, loaded, errored, or empty. The architecture requires that partial failures are isolated -- a failed "Recent Transactions" section must NOT collapse the "Account Summary" section.

State types:

- **Loading**: skeleton loaders per section type (tables, cards, details). Spinner specifically for search.
- **404 errors**: entity-type-specific messages ("Transaction not found", "Account not found", "Contract not found")
- **Transient failures**: distinguish retryable errors from invalid identifiers. Show retry button for retryable.
- **Rate limit**: "Too many requests. Please try again shortly."
- **Partial failure**: each section has independent error boundary. Failed sections show error inline without affecting siblings.
- **Empty states**: contextual suggestions per entity type (e.g., "No transactions found for this account")

## Implementation Plan

### Step 1: Skeleton loader components

Create `libs/ui/src/states/skeletons/`:

- `TableSkeleton.tsx`: shimmer rows matching table column layout
- `CardSkeleton.tsx`: shimmer card matching summary card layout
- `DetailSkeleton.tsx`: shimmer blocks matching detail page header/sections
- `SearchSpinner.tsx`: centered spinner specifically for search loading state

### Step 2: Error state components

Create `libs/ui/src/states/errors/`:

- `NotFoundState.tsx`: props accept entity type, renders "Transaction not found", "Account not found", etc.
- `TransientErrorState.tsx`: shows error message + retry button for retryable failures. Distinguishes retryable from invalid identifier.
- `RateLimitState.tsx`: renders "Too many requests. Please try again shortly." with optional auto-retry countdown.
- `GenericErrorState.tsx`: fallback for unclassified errors.

### Step 3: Empty state components

Create `libs/ui/src/states/empty/`:

- `EmptyState.tsx`: generic empty state with icon, message, and optional suggestion
- Entity-specific variants or messages passed via props: "No transactions found", "No tokens match your filters", "No invocations recorded for this contract"

### Step 4: Section error boundary

Create `libs/ui/src/states/SectionErrorBoundary.tsx`:

- React error boundary scoped to a single page section
- Catches render errors within the section, displays inline error state
- Does not propagate to parent -- sibling sections remain functional
- Reports error info (component stack, section name) for observability

### Step 5: Error classification utility

Create `libs/ui/src/states/classifyError.ts`:

- Takes an error/response and classifies: 404 (not found), 429 (rate limit), 5xx (transient/retryable), network error (retryable), validation error (invalid input)
- Used by error state components to select the correct display

### Step 6: Exports

Export all state components from `libs/ui` barrel.

## Acceptance Criteria

- [x] Skeleton loaders exist for tables, cards, and detail sections — `TableSkeleton`, `CardSkeleton`, `DetailSkeleton` in `libs/ui/src/states/skeletons/`. All take `rows`/`columns`/`lines`/`sections` props so page tasks can tune dimensions to match real layouts when those components land.
- [x] Search uses spinner, not skeleton — `SearchSpinner` wraps `MuiCircularProgress` with a min-height container to prevent layout shift.
- [x] 404 states show entity-type-specific messages — `NotFoundState` with `entity` prop covering transaction / account / contract / ledger / operation / asset / liquidity-pool / generic.
- [x] Transient failure state distinguishes retryable from invalid ID — `TransientErrorState` with `retryable` boolean. Retry button shown only when retryable + `onRetry` provided. Title swaps to "Invalid input" when non-retryable.
- [x] Rate limit state shows the exact spec copy — `RateLimitState` hard-codes "Too many requests. Please try again shortly." Optional `retryAfterSeconds` triggers an auto-retry countdown.
- [x] SectionErrorBoundary isolates failures — class component catching render errors, rendering `GenericErrorState` inline with a reset button. Verified during branch work with a throwing-child demo: siblings rendered untouched while the wrapped section flipped to the error state.
- [x] Empty states show contextual suggestions per entity type — `EmptyState` is generic with `icon`/`title`/`description`/`action`/`meta`/`variant` props. Per-entity copy passed at page call sites (spec's "OR messages passed via props" branch).
- [x] Error classification utility — `classifyError(err)` returns `'not-found' | 'rate-limit' | 'transient' | 'validation' | 'unknown'`. Covers 404, 429, 5xx, 400/422, and `TypeError` (network).
- [x] All components exported from `libs/ui` — re-exported via `libs/ui/src/states/index.ts` then the top-level barrel.

## Notes

- Section error boundaries are critical for detail pages that fetch multiple independent resources (account detail, contract detail, liquidity pool detail).
- The error classification utility is consumed by TanStack Query error handlers (task 0066).
- Skeleton dimensions should match the actual content layout to prevent layout shift on load.
