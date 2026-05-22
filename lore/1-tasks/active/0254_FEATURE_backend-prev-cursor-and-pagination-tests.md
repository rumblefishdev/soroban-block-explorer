---
id: '0254'
title: 'Backend `prev_cursor` in `PageInfo` + pagination test suite (unit + Playwright e2e)'
type: FEATURE
status: active
related_adr: ['0043']
related_tasks: ['0238', '0226']
tags:
  [priority-medium, effort-medium, layer-backend, layer-frontend, phase-future]
milestone: 2
links: []
history:
  - date: 2026-05-22
    who: karolkow
    status: backlog
    note: >
      Spawned from 0238 Future Work. One task covers both the
      backend `prev_cursor` (eliminates client-side prev-stack hack)
      AND the test suite (unit + e2e) that 0238 explicitly deferred.
      Bundled per the team's "larger tasks, not micro-decomposition"
      convention.
  - date: 2026-05-22
    who: karolkow
    status: active
    note: Activated via /promote-task.
---

# Backend `prev_cursor` + pagination test suite

## Summary

Two related follow-ups from task 0238 (URL-cursor pagination
migration) bundled into one work item:

1. **Backend `prev_cursor`** in `PageInfo` — small `crates/api`
   change that eliminates the client-side prev-cursor stack hack in
   `useCursorPagination` and makes "refresh + Prev" work after a
   user pastes a deep link.
2. **Pagination test suite** — vitest units for the libs/ui
   primitives + Playwright CLI e2e across all 13 paginated routes.
   Closes the AC gap in 0238 ("Manual QA on 11+ pages — deferred").

Phase ordering matters: backend `prev_cursor` lands first so the
test suite can lock the post-cleanup behavior (no prev-stack) as
the regression baseline.

## Status: Backlog

Ready to start the backend phase any time. Unit-test phase is
blocked on **0226** (vitest infra for libs/ui); the e2e track is
independent and can land alongside the backend phase.

## Context

### Why `prev_cursor`

Per ADR 0043 (`TsIdCursor` compound `(ts, id)`), every list
endpoint emits a forward cursor pointing at "after the last row
in this response". The frontend uses that for Next. What is
missing: a backward cursor pointing at "before the first row in
this response", so the client can step back without remembering
the path it came from.

Task 0238 worked around this with an in-memory stack
(`useState<string[]>` capped at `MAX_HISTORY = 50`) in
`useCursorPagination`. The stack survives clicks within one
mount but resets on refresh / remount, which breaks the deep-
link-then-Prev flow:

1. Alice pages forward to page 4 (`?cursor=GHI`).
2. Alice shares the URL with Bob.
3. Bob opens it. Stack mounts empty.
4. URL shows page 4 but Prev is disabled — the hook has no idea
   what cursor page 3 was on.

With `prev_cursor` in the response every page is self-describing:

```rust
struct PageInfo {
    cursor: Option<String>,       // next page token (already there)
    prev_cursor: Option<String>,  // previous page token (NEW)
    has_more: bool,
    limit: u32,
}
```

The client takes `prev_cursor` straight from the response — no
stack, no edge cases on refresh.

### Why bundle the tests

Task 0238 shipped 13 paginated pages with zero unit tests and
manual QA deferred. The pagination primitives are about to change
again (stack removed when `prev_cursor` ships). Writing tests
once, after both changes settle, avoids rewriting them mid-flight.

## Implementation Plan

### Phase 1 — backend `prev_cursor` (`crates/api`, `crates/db`)

1. Extend `PageInfo` (likely `crates/api/src/openapi/schemas.rs`)
   with `prev_cursor: Option<String>` and `#[serde(skip_serializing_if
= "Option::is_none")]`.
2. For each list endpoint, compute `prev_cursor` from the FIRST
   row of the returned slice instead of the LAST (mirror of how
   `cursor` is currently built).
   - `crates/db` cursor helpers already expose `to_cursor((ts,
id))` for `TsIdCursor` — reuse for the prev side.
   - First page: `prev_cursor = None`.
   - Empty page: `prev_cursor = None`.
3. Backend integration tests — **cursor-focused** matrix per list
   endpoint:
   - **First page** (no `?cursor=`): `prev_cursor = None`,
     `cursor = Some(<after-last-row>)`, `has_more = true`.
   - **Middle page** (after one Next): `prev_cursor` =
     `to_cursor(first_row)`, `cursor` = `to_cursor(last_row)`,
     `has_more = true`.
   - **Last page**: `prev_cursor = Some(...)`, `cursor = None`,
     `has_more = false`.
   - **Empty page**: `prev_cursor = None`, `cursor = None`,
     `has_more = false`.
   - **Round-trip:** GET page 1 → take its `cursor` as P2; GET
     `?cursor=P2` → take its `prev_cursor` as P1; GET `?cursor=P1`
     returns the same rows as the original first request. Cursor
     symmetry holds.
   - **Forward 3× then backward 3×:** the rows on each page must
     match the rows seen during the forward walk (same ordering,
     same content) — guards against off-by-one in `prev_cursor`
     row selection.
   - **Filter scope:** cursor obtained under `filter[x]=A` rejected
     (or yields different content) when sent with `filter[x]=B`.
     Documents the "cursors are filter-scoped" contract.
   - **Endpoint coverage:** matrix above run against every list
     endpoint, including embedded sub-lists (`LedgerDetail.
transactions`, `LiquidityPool.{participants,transactions}`,
     `Contract.{events,invocations}`, `Account.transactions`,
     `Asset.transactions`, `Nft.transfers`).
4. Regenerate OpenAPI:
   `cargo run -p api --bin extract_openapi > libs/api-types/src/openapi.json`
   followed by `npx nx run @rumblefish/api-types:generate`. Stage
   the generated diff alongside the Rust change (`API types
freshness` CI gate).

### Phase 2 — frontend simplification (`libs/ui`, `web/src`)

1. `libs/ui/src/table/useCursorPagination.ts`:
   - Change `goPrev()` to `goPrev(prevCursor: string)` — symmetric
     with `goNext(nextCursor: string)`. Internally calls
     `setCursor(prevCursor)`.
   - Drop the in-memory stack + `MAX_HISTORY` + `FIRST_PAGE`
     sentinel. `canPrev` now derives from `prevCursor !== null`
     passed into `usePageHandlers` (see step 2), not from internal
     stack length.
2. `libs/ui/src/table/usePageHandlers.ts` — **extend to symmetric
   API** (was only `handleNext` because backend had no prev
   cursor; now both sides need an extract wrapper):
   - Signature: `usePageHandlers(page, goNext, goPrev) → { canPrev,
canNext, handlePrev, handleNext }`.
   - `handlePrev` reads `page?.prev_cursor` and calls
     `goPrev(prevCursor)`. `canPrev = prev_cursor !== null`.
   - `handleNext` unchanged from current behavior.
3. `web/src/pages/*` (13 paginated pages):
   - Destructure `canPrev` + `handlePrev` from `usePageHandlers`
     instead of `useCursorPagination`.
   - Pass `goPrev` as third arg to `usePageHandlers`.
   - `<PaginationControls onPrev={handlePrev} onNext={handleNext} />`
     — now both wrapped, symmetric.
4. `libs/ui/src/table/PaginationControls.tsx` — unchanged (already
   boolean `canPrev` / `canNext` + void callbacks since 0238).
5. Verify no callers still depend on `useCursorPagination`'s
   `canPrev` / removed `goPrev()` no-arg form.

### Phase 3 — unit tests (blocked on 0226)

Once vitest + `@testing-library/react` lands in `libs/ui`
(via 0226), add:

- `libs/ui/src/table/useCursorPagination.test.ts`
- `libs/ui/src/table/usePageHandlers.test.ts`
- `libs/ui/src/table/useTableUrlState.test.ts` (cursorParam path)

Cases:

**`useCursorPagination`**:

- Mount with pasted `?cursor=ABC` deep link + no `resetKey` change
  → cursor preserved on first render (regression test for the
  `useRef` skip-initial-mount fix from 0238).
- `goNext("ABC")` → URL updates.
- `goPrev()` with `prevCursor: "XYZ"` → URL becomes `?cursor=XYZ`.
- `goPrev()` with `prevCursor: null` → URL clears cursor.
- `setFilter("q", "abc")` → URL drops cursor.
- `resetKey` flip → URL drops cursor.
- `resetKey` set but not flipped → no reset.
- `cursorParam: "cursor_p"` → namespaced read/write, default
  `cursor` untouched.

**`usePageHandlers`** (symmetric prev / next after this task):

- `page === undefined` → `canPrev: false`, `canNext: false`.
- `has_more: true, cursor: "X"` → `canNext: true`, `handleNext()`
  calls `goNext("X")`.
- `has_more: true, cursor: null` → `canNext: false` (graceful
  degradation if backend violates contract).
- `has_more: false` → `canNext: false` regardless of cursor.
- `prev_cursor: "Y"` → `canPrev: true`, `handlePrev()` calls
  `goPrev("Y")`.
- `prev_cursor: null` (first page) → `canPrev: false`.
- Page navigation back to first → `canPrev: false` again.

**`useTableUrlState`** (cursorParam):

- Two hooks on the same route, `cursor_p` + `cursor_t` →
  independent keys, no collision.
- `setSort` / `setFilter` clear only the namespaced cursor, not
  the other.

### Phase 4 — Playwright CLI e2e (not blocked)

Per the team's `[[feedback_playwright_mcp_vs_cli]]`: MCP for
exploration, CLI for regression / CI.

Add a Playwright spec under wherever the project keeps Playwright
(verify existing infra first — check `web/e2e/` or root).

#### Cursor-flow scenarios — must pass on every paginated route

These are the **regression net for the cursor mechanism itself**.
Per-route variations follow in the route matrix below.

1. **Round-trip:** load page 1, capture row hashes; Next, capture
   page 2 row hashes; Prev, assert page 2's Prev landed back on
   page 1 with the same row hashes (no off-by-one in
   `prev_cursor`).
2. **Refresh preserves URL cursor:** Next 2×, F5, assert URL
   still has `?cursor=` and page renders the same row hashes.
3. **Deep-link + Prev (the regression test for the prev-stack
   hack removal):** open route with `?cursor=<mid-page-token>`
   in a fresh browser context (no prior history); click Prev;
   assert it navigates to the previous page successfully —
   under the old in-memory stack this was the broken case.
4. **Deep-link + share:** copy the URL after Next 3×, open in
   second browser context, assert identical rendered rows.
5. **First page = no Prev:** load with no `?cursor=`, assert
   Prev button disabled (`prev_cursor === null` from backend).
6. **Last page = no Next:** walk Next until `has_more === false`,
   assert Next button disabled (`cursor === null`).
7. **Filter change resets cursor:** Next 2× → set a filter →
   assert URL drops the cursor key and rows reflect filter
   from page 1.
8. **Switch parent entity resets cursor (detail pages):** Next
   2× inside a detail-section, navigate to a different parent
   id, return — assert cursor URL key dropped and rows reflect
   page 1 of the new parent.
9. **Network failure on Next:** intercept the Next request,
   force 5xx, assert the page shows `TransientErrorState` (not
   blank) and the URL cursor remains unchanged (no broken
   intermediate state).
10. **Multi-section namespacing on the same route:** on LP
    detail and on contract detail, Next in one section must NOT
    affect the other section's cursor. Concretely: Next on
    Participants → URL gains `?cursor_p=`, Transactions section
    still on its own page (no `?cursor_t=` change).

#### Per-route table

| Route                  | Variants on top of the cursor-flow scenarios                                                         |
| ---------------------- | ---------------------------------------------------------------------------------------------------- |
| `/ledgers`             | Next 3×, Prev 3×, refresh on N=2, share link → new context same page                                 |
| `/ledgers/:sequence`   | Inner Next 2×, prev/next ledger nav resets cursor                                                    |
| `/transactions`        | Filter change resets cursor, refresh on filtered cursor, lowercase op normalized                     |
| `/assets`              | Next + Prev, refresh, share link                                                                     |
| `/assets/:id`          | Switch asset → cursor drops                                                                          |
| `/nfts`                | Next + Prev, refresh                                                                                 |
| `/nfts/:id`            | Switch NFT → cursor drops                                                                            |
| `/accounts/:id`        | Switch account → cursor drops                                                                        |
| `/liquidity-pools`     | Next + Prev, filter change resets                                                                    |
| `/liquidity-pools/:id` | Both `?cursor_p=` and `?cursor_t=` independent; switch pool → both drop                              |
| `/contracts/:id`       | Tab switch Events ↔ Invocations; `?cursor_e=` / `?cursor_i=` namespaced; switch contract → both drop |

Common assertions per scenario:

- URL contains expected cursor key.
- Rendered row count matches API `data.length`.
- Refresh preserves URL state.
- Deep-link + Prev works (regression for the prev-stack hack
  removal).
- No console errors.

### Phase 5 — CI wiring

Add Playwright job to `.github/workflows/` extending the existing
TypeScript pipeline. Gate on `Detect changes` filter so it skips
for backend-only PRs.

## Acceptance Criteria

- [ ] `PageInfo` exposes `prev_cursor: Option<String>`; every
      list endpoint populates it correctly.
- [ ] `libs/api-types` regenerated and staged in the same PR.
- [ ] `useCursorPagination` no longer maintains an in-memory
      stack; `MAX_HISTORY` and `FIRST_PAGE` removed; `goPrev`
      signature changes to `goPrev(prevCursor: string)`.
- [ ] `usePageHandlers` returns a symmetric shape
      `{ canPrev, canNext, handlePrev, handleNext }`. The current
      next-only shape was a consequence of `goPrev()` being no-arg
      under the prev-stack hack; with backend `prev_cursor` both
      sides need the same extract-from-`page` wrapper.
- [ ] Backend integration tests cover the cursor matrix per list
      endpoint: first page (`prev_cursor=None`), middle (both set),
      last (`cursor=None`), empty (both `None`), round-trip
      symmetry (Next then Prev returns the same rows), forward-
      then-backward 3× consistency, filter-scope rejection. Matrix
      runs against every list endpoint including embedded sub-lists.
- [ ] Playwright e2e cursor-flow scenarios (round-trip, refresh
      preserves URL, **deep-link + Prev** regression for prev-stack
      hack removal, deep-link share, first-page no-Prev, last-page
      no-Next, filter-resets-cursor, parent-entity-switch resets,
      network-failure preserves URL, multi-section namespacing
      isolation) pass on every paginated route.
- [ ] Deep-link + Prev works on every paginated route.
- [ ] All 13 paginated pages still typecheck / lint / build.
- [ ] Vitest unit suite for the 3 primitives, covering the cases
      above (deferred until 0226 lands; track separately if 0226
      slips).
- [ ] Playwright CLI spec for all 13 routes, green locally
      against `npx nx serve web` + a backend dev server.
- [ ] CI job runs Playwright on every PR touching `web/` or
      `libs/ui/`.
- [ ] Docs: `docs/architecture/frontend/frontend-overview.md`
      pagination section reworded (no more prev-stack mention);
      references the test suite as the regression net.

## Reused (no new infra)

- `TsIdCursor::to_cursor()` (ADR 0043) — for `prev_cursor` build.
- `useCursorPagination` — modified, not replaced.
- `usePageHandlers` — extended to symmetric `{ canPrev, canNext,
handlePrev, handleNext }` (was next-only because backend lacked
  `prev_cursor`).
- `CURSOR_PARAMS` registry — unchanged.
- Vitest + `@testing-library/react` — once 0226 ships.
- Playwright — verify project already has `node_modules` entry
  before planning install.
- Existing dev-server setup (`pnpm nx serve web`).

## Risks / Open Questions

- **Detail-page embedded lists** (e.g. `LedgerDetail.transactions`)
  also need `prev_cursor` on their inner `PageInfo`. Trace each
  embedded list and confirm.
- **Rollout window:** during deploy, frontend may briefly see
  responses without `prev_cursor`. One-line fallback ("if
  `prev_cursor` missing, disable Prev") behind a feature flag, or
  ship strictly after backend lands.
- **0226 slip:** if vitest infra takes longer than expected, ship
  Phase 1+2+4+5 (backend, FE simplification, Playwright, CI) and
  spawn a thin follow-up for Phase 3 (units) when 0226 lands.

## Notes

- Acknowledged in 0238 Future Work + Risks: "No backend
  `prev_cursor`: stack only survives same-session forward walks.
  Refresh + Prev = stack empty + Prev disabled. Real fix would be
  backend `prev_cursor` (separate task)."
- Acknowledged in 0238 AC: "Manual QA on all 11+ pages (deferred
  — Playwright dev server smoke not run in worktree; gate moved
  to PR review / 0226 vitest infra follow-up)." This task closes
  that deferral.
- ADR 0043 (`TsIdCursor`) covers the compound-key cursor design;
  adding `prev_cursor` is a strict superset, no ADR amendment
  needed.
- Memory profile sanity (per-cursor cache + `gcTime`) — fold into
  Playwright spec: open 10+ cursors, assert `gcTime` evicts old
  entries (or skip if hard to assert deterministically).
