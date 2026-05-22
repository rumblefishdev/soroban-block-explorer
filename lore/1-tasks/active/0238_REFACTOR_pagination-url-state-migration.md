---
id: '0238'
title: 'Migrate pagination from useState pageIndex to URL cursor (finish 0061)'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0061', '0230']
tags: [priority-medium, effort-medium, layer-frontend, phase-future]
milestone: 2
links: []
history:
  - date: 2026-05-20
    status: backlog
    who: karolkow
    note: >
      Spawned from 0230 (Copilot nits) after a fresh look at the
      pagination story. Working draft of the migration is on branch
      `refactor/0238_pagination-url-state-migration` (local; commit
      `d5f5014`), parked there so 0230 can land as the small task it
      was meant to be.
  - date: 2026-05-22
    status: active
    who: karolkow
    note: Activated to pick up draft and finish URL-cursor migration.
---

# Migrate pagination from useState pageIndex to URL cursor (finish 0061)

## Summary

Bring the frontend's pagination story in line with the original 0061
spec: **URL-as-state, cursor in `?cursor=`**. Eight list/detail pages
currently keep `pageIndex` in `useState` and rely on `useInfiniteQuery`'s
`data.pages[]` array. Refresh resets to page 0, links are not shareable,
and the abstraction (`useInfinitePager`) duplicates what
`useCursorPagination` was meant to be.

This task finishes what 0061 started: revive `useCursorPagination`,
switch the eight infinite hooks to `useQuery` per cursor, and remove the
`useInfinitePager` middle layer.

## Context

0061 (archived) delivered `useTableUrlState` + `useCursorPagination` +
`ExplorerTable` + `PaginationControls`. It defined the URL-as-state
pattern but did not wire it into the data layer — the spec stopped at
"hook reads cursor from URL". Subsequent frontend tasks (0068, 0072,
0075, NFTs, transactions, accounts, assets) ignored the URL story and
picked a different pattern: `useInfiniteQuery` + `useState pageIndex`,
later extracted as `useInfinitePager`.

Result: filters and sort live in the URL (via `useTableUrlState`), but
the cursor doesn't. Refresh resets paging. Links lose paging state.
Industry-standard explorers (stellar.expert, Etherscan) all use the
URL-state pattern.

`useCursorPagination` has been dead since 0061 merged — no consumer
imports it. This task makes it the primary entry point.

## Working Draft

A complete first pass exists on:

- **Branch:** `refactor/0238_pagination-url-state-migration`
- **Commit:** `d5f5014` (`wip(lore-0237): C-pattern pagination migration draft`)
- **Touches:** 3 `libs/ui/src/table/` files, 8 web hooks, 8 pages, 1
  file deleted (`useInfinitePager`).
- **Status:** local typecheck + lint + build green (post `npm install`
  in worktree). **No tests, no manual QA.**

Use the draft as the starting point — but treat it as untrusted until
the verification plan below is executed.

## Implementation Plan

### Phase 1 — primitives (`libs/ui/src/table/`)

- Rewrite `useCursorPagination`:
  - Forward `UseTableUrlStateOptions` (`filterKeys`).
  - Maintain a client-side prev-cursor stack in `useState<string[]>`
    (the API gives only `next` cursor; the stack remembers where we
    came from).
  - Expose `{ state, cursor, canPrev, goNext, goPrev, setFilter, reset }`.
  - `setFilter` clears the stack (cursors are filter-scoped on server).
  - `goPrev()` takes no argument — pops from the stack. Diverges from
    the original 0061 spec (`goPrev(cursor)`) on purpose: the spec
    assumed a magically available prev cursor that doesn't exist.

### Phase 2 — hooks (`web/src/api/hooks/`)

For each of the eight: `useInfiniteQuery` → `useQuery`, codegen
`*InfiniteOptions()` → `*Options()`, drop `initialPageParam` +
`getNextPageParam`, accept `cursor` as a function argument.

- `useLedgersList(cursor)`
- `useLedgerDetail(sequence, cursor, enabled)`
- `useTransactionsList(cursor, filters)`
- `useAssetsList(cursor, filters)`
- `useNftsList(cursor, filters)`
- `useNftTransfers(id, cursor, enabled)`
- `useAccountTransactions(accountId, cursor)`
- `useAssetTransactions(id, cursor)`

Each cursor produces a distinct queryKey, so revisiting a cursor is a
cache hit (the new pattern's "instant Prev").

### Phase 3 — pages (`web/src/pages/`)

Drop `useState pageIndex`, drop `useInfiniteQuery` return-shape
handling, drop the manual `fetchNextPage().then(setPageIndex(+1))`
dance. Use `useCursorPagination` directly.

Detail pages with a path param (Ledger/NFT/Account/Asset) add a
`useEffect(reset, [id])` because cursors are scoped to the parent
entity and must drop on entity change.

### Phase 4 — cleanup

- Delete `web/src/pages/useInfinitePager.ts` (dead under new pattern).
- Verify barrel exports unchanged in `libs/ui/src/index.ts`.

## Acceptance Criteria

- [ ] `?cursor=X` survives refresh on every paginated list page
- [ ] Sharing a deep-page link opens the same page for another user
- [ ] Browser back/forward navigates page-by-page (or, explicitly: it
      doesn't, because `replace: true` is intentional — decide and
      document)
- [ ] `Previous` is instant after walking forward (RQ cache hit)
- [ ] Changing filters resets cursor AND stack
- [ ] Switching detail entities (ledger/NFT/account/asset) drops the
      cursor
- [ ] `useInfinitePager` removed from the codebase
- [ ] `useCursorPagination` is the single pagination entry point
- [ ] Manual QA on all eight pages (smoke test checklist below)
- [ ] `nx typecheck` + `nx lint` + `nx build` green for both
      `libs/ui` and `web`

## Manual QA Smoke Test

Before merge, click through each page:

| Page                 | Smoke                                                   |
| -------------------- | ------------------------------------------------------- |
| `/ledgers`           | Next 3×, Prev 3×, refresh on N=2, share link            |
| `/ledgers/:sequence` | Inner Next 2×, prev/next ledger, refresh                |
| `/transactions`      | Filter change resets cursor, refresh on filtered cursor |
| `/assets`            | Same as transactions                                    |
| `/nfts`              | Same as transactions                                    |
| `/nfts/:id`          | Switch NFT id, transfer history pagination              |
| `/accounts/:id`      | Switch account, transactions pagination                 |
| `/assets/:id`        | Switch asset, transactions pagination                   |

## Risks

- **Memory profile:** per-cursor cache vs single `data.pages[]`. RQ
  `gcTime` from policies (`listPolicy`, `detailPolicy`) should bound it.
  Verify in DevTools that gcTime fires.
- **Header duplication:** `useLedgerDetail` echoes the ledger header
  in every page response. Per-cursor cache duplicates it. Marginal.
- **`replace: true` vs browser history:** current `useTableUrlState`
  uses `replace: true` for URL writes — back-button doesn't paginate.
  Confirm if that's still wanted or switch to `push` for cursor changes.
- **No backend `prev_cursor`:** stack only survives same-session
  forward walks. Refresh + Prev = stack empty + Prev disabled. Real
  fix would be backend `prev_cursor` (separate task).
- **No tests:** `libs/ui` has no test infra (0226). `web` has no
  pagination tests. Manual QA is the only safety net.

## Notes

- Pure frontend refactor. No schema / API / docs-architecture change
  (ADR 0032 N/A).
- 0226 (libs/ui vitest test infra) blocks adding real tests; this task
  can land without them but should be revisited after 0226.
- Working draft branch already named `refactor/0238_pagination-url-state-migration`;
  cherry-pick `d5f5014` onto a fresh branch if cleaner history is wanted.
