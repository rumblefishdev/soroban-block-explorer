---
id: '0527'
title: 'BUG: search results page — caret jumps to end while typing, and a single hit redirects on its own'
type: BUG
status: active
related_adr: []
related_tasks: ['0271', '0443']
tags: [frontend, search, ux, priority-medium, effort-small]
links: []
history:
  - date: '2026-09-01'
    status: active
    who: karolkow
    note: >
      Both found by hand on the running app while reviewing 0443 scope A.
      Neither is caused by that work — the caret bug predates it and the
      redirect is 0271 behaviour being withdrawn deliberately.
---

# BUG: two defects on the search results view

## 1 — the caret jumps to the end of the input

Editing in the middle of the query on `/search` moves the cursor to the end
after every keystroke, so a correction has to be retyped from scratch.

**Cause.** The input on that page is controlled straight off the URL:
`value={q}` where `q` comes from `useSearchParams`, and `onChange` writes back
through `setParams`. The router state lands in a later render than the
keystroke, so React re-assigns `input.value` after the fact and the browser
puts the caret at the end. The header search bar does not have this bug
because `AppShell` holds the text in local state.

**Fix.** Hold the text in local state on the page too and mirror it into the
URL, instead of reading it back out of the URL. The URL stays the shareable
value; the input stops round-tripping through the router on every character.

## 2 — a single hit navigates away by itself

Task 0271 added: broad search returns exactly one hit across all buckets →
navigate to it, `replace: true`. In use this takes the page away before the
result can be read, and `replace` means Back does not return to it — the
query has to be retyped to see what matched.

**Withdrawn deliberately.** Deterministic redirects are unaffected and stay:
a full transaction hash, a `G…`/`C…`/`L…` StrKey and a bare ledger sequence
all still go straight to their page, because those are exact-identity lookups
resolved before the search runs (`directRouteFor`, and the backend's own
redirect branch). What changes is only the fuzzy case — a name search that
happens to match one row now shows that row.

## Acceptance criteria

- [x] Typing in the middle of the query on `/search` leaves the caret where it was
- [x] The URL still carries the query (shareable, survives reload)
- [x] A search with exactly one hit renders the results table, no redirect
- [x] Exact-identity inputs (tx hash, StrKey, ledger sequence) still redirect
- [x] **Docs updated** — N/A, no architectural surface changes
- [x] **API types regenerated** — N/A, frontend only

## Implementation notes

Both fixes live in `web/src/pages/SearchResultsPage.tsx`; `routeForHit` is no
longer imported there. Three tests added in `SearchResultsPage.test.tsx`
(caret position, single-hit renders instead of navigating, plus the existing
federation cases). The caret test was checked against the old binding first —
it reports caret 5 instead of 3, so it fails when the defect is present.

Verified on the running app: editing mid-query keeps the caret and still
writes the URL (`?q=kaXle`), and the query `Kale Inferno`, which matches
exactly one contract, now renders that row at `/search` instead of jumping to
the contract page.
