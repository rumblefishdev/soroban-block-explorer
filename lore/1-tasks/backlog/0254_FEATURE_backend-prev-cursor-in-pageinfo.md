---
id: '0254'
title: 'Backend `prev_cursor` in `PageInfo` — make refresh+Prev work after pasted deep links'
type: FEATURE
status: backlog
related_adr: ['0043']
related_tasks: ['0238']
tags: [priority-medium, effort-small, layer-backend, layer-frontend, phase-future]
milestone: 2
links: []
history:
  - date: 2026-05-22
    who: karolkow
    status: backlog
    note: >
      Spawned from 0238 Future Work. Eliminates the client-side
      prev-cursor stack hack in `useCursorPagination` (libs/ui) and
      makes "refresh + Prev" work after a user pastes a deep link.
---

# Backend `prev_cursor` in `PageInfo`

## Summary

Add a `prev_cursor: Option<String>` field to `PageInfo` (the pagination
envelope shared by every list / detail endpoint). Frontend can then
drop the client-side prev-cursor stack introduced by task 0238 and
support "refresh + Prev" out of the box — currently broken when a user
opens a pasted deep link `?cursor=X` and clicks Prev (stack is empty
on mount, so Prev disables).

Tiny backend change. Big UX upgrade. Removes a real defect in 0238's
URL-cursor migration.

## Context

Per ADR 0043 (`TsIdCursor` — compound key `(ts, id)`), every list
endpoint emits a forward cursor pointing at "after the last row in
this response". The frontend uses that to fetch the next page.

What is missing: a backward cursor pointing at "before the first row
in this response", so the client can step back without remembering
where it came from.

Task 0238 worked around this with a client-side stack
(`useState<string[]>` capped at `MAX_HISTORY = 50`) that remembers
the cursors the user walked forward through. The stack survives
clicks on a single mount but resets on refresh / remount, which
breaks the deep-link-then-Prev flow:

1. Alice pages forward to page 4 (`?cursor=GHI`).
2. Alice shares the URL with Bob.
3. Bob opens it. `useCursorPagination` mounts with an empty stack.
4. URL shows page 4, but Bob's Prev button is disabled — the hook
   has no idea what cursor page 3 was on.

With `prev_cursor` in the response, each page is self-describing:

```rust
struct PageInfo {
    cursor: Option<String>,       // next page token (already there)
    prev_cursor: Option<String>,  // previous page token (NEW)
    has_more: bool,
    limit: u32,
}
```

The client takes `prev_cursor` straight from the current response —
no stack, no edge cases on refresh.

## Implementation Plan

### Phase 1 — backend (`crates/api`, `crates/db`)

1. Extend `PageInfo` (probably in `crates/api/src/openapi/schemas.rs`)
   with `prev_cursor: Option<String>` and `#[serde(skip_serializing_if =
   "Option::is_none")]`.
2. For each list endpoint that builds a `PageInfo`, compute the
   `prev_cursor` from the FIRST row of the returned slice instead of
   the LAST row (mirror of how `cursor` is currently computed).
   - `crates/db` cursor helpers already expose `to_cursor((ts, id))`
     for `TsIdCursor`. Reuse for the prev side.
   - First page: `prev_cursor = None`.
   - Empty page: `prev_cursor = None`.
3. Regenerate OpenAPI: `cargo run -p api --bin extract_openapi >
   libs/api-types/src/openapi.json` then
   `npx nx run @rumblefish/api-types:generate`.
4. Backend unit tests: extend existing list-endpoint tests to assert
   `prev_cursor` is correct for first / middle / last page.

### Phase 2 — frontend (`libs/ui`, `web/src/api`)

1. `libs/ui/src/table/useCursorPagination.ts`:
   - Add `prevCursor?: string | null` parameter (or expose via a new
     `setPrevCursor(c)` setter the page can call with `data.page.prev_cursor`).
   - Drop the in-memory stack + `MAX_HISTORY` once backend ships.
   - `goPrev()` reads the supplied `prevCursor` and navigates via
     `setCursor(prevCursor)`.
2. `web/src/pages/*` 13 paginated pages: pipe
   `data?.page.prev_cursor` into `useCursorPagination`. Most call
   sites already destructure `data?.page` for `usePageHandlers`, so
   the change is one extra line per page.
3. Remove the prev-stack code paths from `useCursorPagination`.
   Leave a single forward path (URL has both `?cursor=` and the
   response now carries `prev_cursor` for the back action).

### Phase 3 — verify

- `nx typecheck` + `nx lint` + `nx build` green on libs/ui + web.
- Playwright CLI smoke for "deep-link + Prev" on any paginated
  list and any detail-page section (e.g. `/transactions?cursor=X`,
  `/pools/:id?cursor_p=Y`, `/contracts/:id?cursor_e=Z`). Verify
  Prev works on first click after mount.

## Acceptance Criteria

- [ ] `PageInfo` includes `prev_cursor: Option<String>`.
- [ ] Every list endpoint populates `prev_cursor` correctly.
- [ ] `libs/api-types` regenerated and staged in the same PR
      (per `API types freshness` CI gate).
- [ ] `useCursorPagination` no longer maintains an in-memory stack;
      `MAX_HISTORY` constant removed.
- [ ] Deep-link + Prev works: open `/transactions?cursor=X` and
      click Prev → navigates to the page before X.
- [ ] All 13 paginated pages still typecheck / lint / build.
- [ ] Docs updated: any pagination paragraph in
      `docs/architecture/frontend/frontend-overview.md` that
      mentions the prev-stack hack is reworded.

## Reused (no new code)

- `TsIdCursor::to_cursor()` (per ADR 0043).
- `useCursorPagination` (libs/ui) — modified, not replaced.
- `usePageHandlers` (libs/ui) — unchanged.
- `CURSOR_PARAMS` registry (web) — unchanged.

## Risks / Open Questions

- **Detail-page embedded lists** (e.g. `LedgerDetail.transactions`)
  also need `prev_cursor` on their inner `PageInfo`. Trace each
  embedded list and confirm.
- **Mixed-server periods:** during rollout the frontend may briefly
  see responses without `prev_cursor`. Keep a one-line fallback
  ("if prev_cursor missing, disable Prev") behind a feature flag or
  ship strictly after backend is in production.

## Notes

- Acknowledged in 0238 Future Work + Risks ("No backend
  `prev_cursor`: stack only survives same-session forward walks.
  Refresh + Prev = stack empty + Prev disabled. Real fix would be
  backend `prev_cursor` (separate task).").
- ADR 0043 (`TsIdCursor`) covers the compound-key cursor design;
  adding `prev_cursor` is a strict superset, no ADR amendment
  needed.
