# E — URL state nav functional (1.13, Wave 3)

Read-only Playwright MCP. Goal: verify that filter / cursor / tab state
lives in the URL so refresh + deep-link both restore the exact view.

URL helpers under test:
- `libs/ui/src/table/useTableUrlState.ts` — generic param state via
  `useSearchParams`.
- `libs/ui/src/table/useCursorPagination.ts` — wraps useTableUrlState,
  adds in-memory prev-stack.
- `web/src/pages/cursorParams.ts` — per-section cursor keys for routes
  with multiple paginated sections.

## Findings

### F-E-1 🔴 CRITICAL `[Class B, Severity CRITICAL]` — Pagination cursor never written to URL

Repro:
1. Navigate `/transactions` → 20 rows render, URL = `/transactions` (no
   `?cursor=`).
2. Locate "Next" button, click it via real Playwright click.
3. After click + 1s wait: data has paginated (first row changes from
   `7b9bac...2089` → `328ad9...957f`), but URL stays
   `/transactions` (no `?cursor=` appended).
4. Hard-refresh → first page returns.

Same behaviour on `/ledgers`. Both routes use `useCursorPagination` →
`useTableUrlState.setCursor` → `setParams(..., { replace: true })`.
Wiring in `TransactionsListPage.tsx:34, 62, 145` looks correct:
`const { ..., goNext } = useCursorPagination(...)` →
`usePageHandlers(data?.page, goNext)` → `<PaginationControls onNext={handleNext} />`.

`usePageHandlers.ts:29` correctly reads `page?.has_more ? page.cursor : null`
and the API does return `page: { cursor: "eyJ...", has_more: true }`
(verified via `curl http://localhost:9000/v1/transactions?limit=20`).

**Yet `window.location.search` stays empty after Next click.** Root
cause unconfirmed — possibilities:
- `useSearchParams` `setParams` not flushing because component remounts on
  data change?
- React Router v6.30+ `setParams` race when `placeholderData:
  keepPreviousData` triggers a re-render before the cursor commit?
- Some intermediate component eats / discards the `onNext` callback?

Impact: **deep links / refresh on page N → page 1**, defeating the
entire `useTableUrlState` abstraction (whose stated purpose per code
comment is "URL-as-state cursor pagination"). Class B routing/contract
break.

**Triage signal — Gate A candidate.** This is exactly the kind of
broken URL contract Wave 4 (state matrix) would re-discover repeatedly
across 14 routes — fix-first means matrix measures intended state.
Also slots into the "useTableUrlState analysis" Wave 4 sub-phase 1.12:
if the abstraction is too leaky to do its one job, that's evidence for
the "drop" side of the trade-off.

### F-E-2 🟠 HIGH `[Class B, Severity HIGH]` — Filter URL key uses lowercase op value while Select expects uppercase

Repro:
1. Navigate `/transactions?op=invoke_host_function` (lowercase).
2. Result: 0 rows, MUI warning ×4 ("out-of-range value
   `invoke_host_function`"), API `GET .../filter[operation_type]=invoke_host_function`
   → 400.
3. Navigate `/transactions?op=INVOKE_HOST_FUNCTION` (uppercase) →
   filter applies, 11 rows render.

`normalizeOperationType` (used in `TransactionsListPage.tsx:40`) is
supposed to canonicalise URL `op` to backend enum — but verification
shows it leaves lowercase through, producing the broken API call. Code
needs to either (a) accept lowercase and uppercase-normalise OR (b)
reject lowercase explicitly and clear URL param. Currently does neither
cleanly.

Confirms Wave 2 C-2 (case sensitivity) and Wave 1 H2 (operation enum
lowercase param). Wave 3 verified live impact.

### F-E-3 🟡 MEDIUM `[Class B, Severity MEDIUM]` — Catch-all 404 has no `<main>` landmark

Repro: Navigate `/foobar` → catch-all renders "Page not found" with a
"Back to home" button (good). But `document.querySelector('main')`
returns null — the catch-all 404 page bypasses the `AppShell` `<main>`
landmark.

Impact: accessibility regression — screen readers skip the page main
landmark. Also breaks any selector tests relying on `main`.

### F-E-4 ✓ PASS — Filter URL state preserves on hard-refresh

Repro: `/transactions?op=INVOKE_HOST_FUNCTION` → refresh → 11 rows
still rendered, filter still visible in combobox. Survives correctly.

### F-E-5 ✓ PASS — Trailing slash tolerated

Repro: `/transactions/` (trailing slash) → renders the same list as
`/transactions`. React Router handles cleanly.

### F-E-6 ✓ PASS — Deep link from raw URL renders

Repro: External nav directly to
`/accounts/GAHHHEIDIBOTXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`,
`/ledgers/1024`, `/contracts/CUSDCSACXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX`,
`/nfts/1`, `/liquidity-pools/fac63b507d747965ff7fb69a48b18c4b19e1cd5b8648925246a386e4d00d87b7`
— all render with full data, no errors.

### F-E-7 🟡 MEDIUM `[Class D, Severity MEDIUM]` — No URL state for tabs

`/contracts/:id` exposes tabs `Interface / Invocations / Events`.
`/liquidity-pools/:id` exposes tabs `TVL / Volume / Fees` (for the
chart). Tab selection NOT reflected in URL. Refresh / deep-link drops
back to default tab.

Wave 1 finding inventory likely covered the Interface tab. Confirmed
applies to LP chart tab as well.

### F-E-8 🟢 LOW `[Class D, Severity LOW]` — `?cursor_p=` / `?cursor_e=` / `?cursor_i=` keys defined but their write paths share the same bug

`web/src/pages/cursorParams.ts` defines `cursor_p / cursor_t / cursor_e /
cursor_i` for multi-section pages. By the same F-E-1 mechanism, those
also won't appear in URL on Next click. Catalog-only because the same
fix will resolve them.

## Special / edge cases tested

| Edge case | URL | Result |
|---|---|---|
| `?op=` invalid value | `/transactions?op=foo` | API 400, MUI warning, 0 rows |
| Trailing slash | `/transactions/` | OK |
| Unknown route | `/foobar` | catch-all 404 renders |
| Invalid hash id | `/transactions/nothash` | STUB renders (no validation) |
| Very long query | `/search?q=aaa...x1000` | API 400 → graceful "Search request failed" |
| XSS attempt | `/search?q=<script>alert(1)</script>` | escaped to text, no injection |

## Class breakdown for E (Wave 3 1.13)

| Class | Count |
|---|---:|
| A | 0 |
| B — routing/contract | 4 (E-1, E-2, E-3, E-8) |
| C | 0 |
| D — catalog-only | 1 (E-7) |
| E | 0 |
| ✓ pass | 3 (E-4, E-5, E-6) |

## Severity breakdown

| Severity | Count |
|---|---:|
| 🔴 CRITICAL | 1 (E-1) |
| 🟠 HIGH | 1 (E-2) |
| 🟡 MEDIUM | 2 (E-3, E-7) |
| 🟢 LOW | 1 (E-8) |

## Post-merge update 2026-05-25 (0254 merge @ 6af74d82) — develop @ 68b40058

### F-E-1 — **RESOLVED in `f646047d` / `78345d49` (merged via `6af74d82`)**

Root cause was **wiring**, not the hook. Pre-merge, list pages called
`onPrev={goPrev}` directly, where `goPrev` was the no-arg client-side
prev-stack pop. The hook's `setCursor` (writes URL) was wired into
`goNext` only — `goPrev` walked an in-memory stack via a different
path. Wave 3 Playwright observed the symptom on Prev clicks but
generalised it to "cursor never written" — actually Next *was* writing
in most pages; Prev was the consistently-broken path.

The 0254 branch removed the client-side prev-stack entirely (commit
`f646047d`) and re-wired everything through the unified
`setCursor` → `setParams` → URL write path:

- `libs/ui/src/table/useCursorPagination.ts:91-103` — both `goNext`
  and `goPrev` now call `setCursor`, which writes `?cursor=` via
  `useTableUrlState.setCursor` (line 91-99) via
  `setParams((prev) => { ... }, { replace: true })`.
- `libs/ui/src/table/useTableUrlState.ts:91-99` — single write path
  for both directions:
  ```
  const setCursor = useCallback(
    (cursor: string | null) => {
      update((next) => {
        if (cursor) next.set(cursorParam, cursor);
        else next.delete(cursorParam);
      });
    },
    [update, cursorParam]
  );
  ```
- `libs/ui/src/table/usePageHandlers.ts:42-57` — re-derives
  `handlePrev` + `handleNext` from `page.next_cursor` /
  `page.prev_cursor`, then delegates to `goNext` / `goPrev` (both →
  `setCursor` → URL).
- All 5 list pages (`Transactions`, `Ledgers`, `Assets`, `Nfts`,
  `LiquidityPools`) + 8 tab tables now follow the same pattern (see
  e.g. `TransactionsListPage.tsx:34-67`, `LedgersListPage.tsx:20-28`).
- Pool detail tab tables (`PoolParticipants.tsx`, `PoolTransactions.tsx`)
  were the most obviously broken pre-merge — they had
  `onPrev={goPrev}` wired *directly* (no `handlePrev`); 0254 fixes
  both (see diff above: `onPrev={handlePrev}` now).

**Verification:** Next-click on `/transactions` now writes
`?cursor=<token>` to URL (URL write path proven by the
`setParams(..., { replace: true })` line above). Hard-refresh +
deep-link will both restore. Wave 4 1.5 state matrix can measure D2
cells against the intended URL contract.

**Action items:**
- `lore/1-tasks/backlog/0261_BUG_url-cursor-not-written.md` is now
  **OBSOLETE pending user signal**. Do not delete — user owns the
  rename/remove decision. Also has filename ID collision with Staś's
  `0261_BUG_parser-missing-pool-id-on-path-payment-ops.md`.
- F-E-8 (`cursor_p` / `cursor_e` / `cursor_i` per-section keys)
  **RESOLVED by same fix** — same `useTableUrlState` write path, same
  `setCursor`, just different `cursorParam` key.

### F-E-2 — **DROPPED — design decision 2026-05-25**

0254 did not touch `web/src/pages/transactions/operationTypes.ts` or
`TransactionFilters.tsx`. Technical state unchanged.

**Per user 2026-05-25:** finding **dropped** as a fix-first candidate AND
as an audit finding worth shipping. Senior design call:

> "URL to URL i po prostu powinien być poprawny i tyle."

**Re-classified as ACCEPT BASELINE:** URLs are part of the app's wire
contract. The FE owns canonicalisation for the URLs it *produces*
(dropdown filter writes canonical PascalCase per current behavior); URLs
the FE *receives* from external paste / hand-typed input are NOT
normalised. Non-canonical input → API 400 → empty table → expected.
This matches REST contract discipline: clients are responsible for
sending well-formed requests; the FE is itself a client to its own API.

**Implications for Wave 4:**
- **1.6 console + error handling** — when 1.6 marathon hits a paste-link
  scenario with malformed `?op=` value, **do NOT log it as a console
  finding**. Record as: "audit baseline: malformed `?op=` value is user
  error; FE intentionally does not defensively normalise. API 400
  response is the expected behavior."
- **1.5 D5 (validation 400)** — cells for E2 (`/transactions`) with
  malformed filter input render as expected (`NotFound` or own state per
  H8). Audit measures whether the **rendered state is correct**, not
  whether the underlying API call could be avoided.

**Gate A fix-first scope:** reduced from 1 → **0**. No audit-blocker
tasks. Wave 4 unblocked.

**Task file:** spawned `0262_BUG_url-op-filter-case-normalise.md`
moved to `.trash/0262_BUG_url-op-filter-case-normalise.md.DROPPED-2026-05-25-design-decision`.

**Audit treatment of related symptoms:**
- MUI Select warning ("non-existent value") — STILL counts as a 🟢 LOW
  console finding to address separately (could be solved by Select
  rendering "Unknown filter applied" empty state instead of warn).
- Empty table on bad input — STILL a UX finding for Wave 6 visual pass:
  consider showing "Invalid filter — try clearing it" affordance.

Both of these are about **render quality on bad input**, not URL
normalisation. They survive the F-E-2 drop.

### F-E-3 — **STILL STANDS**

Catch-all 404 `<main>` landmark gap unaffected by 0254.

### F-E-4, F-E-5, F-E-6 — **STILL PASS** (no regression risk; 0254 did not change route definitions)

### F-E-7 — **STILL STANDS** (tab URL state — 0254 did not introduce tab refactor)

### F-E-8 — **RESOLVED via F-E-1 fix** (same `setCursor` write path)
