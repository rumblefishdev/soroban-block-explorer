# AL — State separation + EXTRA `useTableUrlState` analysis (Wave 4 1.12)

## Part 1 — State separation per-check

| Check | Result | Evidence |
|---|---|---|
| Server state (TanStack) vs UI state (useState) vs URL state (useTableUrlState) — clean separation | ✓ | Sampled 4 pages (TransactionsListPage, LedgerDetailPage, AccountDetailPage, transaction-detail/index.tsx). Clean separation: TanStack hooks own server data, useState owns transient UI (selectedIndex in tx-detail), useCursorPagination owns URL. No mixing observed. |
| Global state justified | ✓ | Only ColorModeContext (theme light/dark) — single context, top-level provider, leaf consumer. Justified. |
| Local state that should be URL | ⚠ | tx-detail's `selectedIndex` (`useState(0)`) for operation picker is **deliberately** local — operation index resets on hash change. Acceptable but borderline. See F-AL-1. |
| URL state that should be local | ✗ | None observed (URL state is correctly only used for cursor/filter/sort/mode). |
| Prop drilling >3 levels | ✓ | Max 2 in sampled pool-detail (per X-coupling F-X-1). |
| `useReducer` vs `useState` consistent | ✓ | Zero `useReducer` in `web/src/` — all simple `useState`. No complex state machines requiring reducer. |
| `useDetailMode` aligned with `useTableUrlState` | ✗ | Diverges — see F-U-5 (in U-component-reuse.md) and EXTRA part 2 below. |

## Findings

### F-AL-1 [Class C, Severity 🟡] — `selectedIndex` in tx-detail uses `useState` not URL

- **Location:** `web/src/pages/transaction-detail/index.tsx:30`: `const [selectedIndex, setSelectedIndex] = useState(0);`
- **Behavior:** Operation picker selection is in-page state — refresh / deep-link resets to op #0.
- **Trade-off:**
  - Keep local: Deep-link `/transactions/<hash>` always lands on op #0. Predictable for shareable links of "the transaction" rather than "the n-th operation".
  - Move to URL (`?op_idx=N`): Refresh preserves. Better for deep-link to specific op.
- **Class:** C — defer to Gate B with E3 visual audit; depends on Figma intent.

### F-AL-2 [Class A, Severity 🟢] — `useDetailMode` parallel URL-state pattern

- Already in F-U-5 + F-X-3. Defer / document.

---

## Part 2 — EXTRA: `useTableUrlState` justification analysis

Per user 2026-05-24 EXTRA request. Senior-fresh-eye question: do we need `useTableUrlState` at all, given TanStack Query is already in stack and React Router provides `useSearchParams`?

### What `useTableUrlState` actually does

Reading `libs/ui/src/table/useTableUrlState.ts:1-126`:

1. **URL ↔ typed state mapping.** Reads `?cursor=`, `?sort=`, `?dir=`, `?<filterKey>=` from `useSearchParams()`, returns typed `TableUrlState { cursor, sortBy, sortDir, filters }`.
2. **Setter primitives.** `setCursor`, `setSort`, `setFilter`, `resetCursor` — each one a `URLSearchParams` patch using `replace: true` (no history pollution).
3. **Side-effect rules.** `setFilter` and `setSort` automatically drop the cursor (filter/sort change invalidates pagination cursor). `setCursor(null)` clears.
4. **Reference stability optimization.** `filterKeysKey = filterKeys.join('|')` collapses inline-array prop into stable string for `useMemo` deps (lines 49-58, well-commented).
5. **Configurable cursor key.** `cursorParam: string` lets caller use `'cursor_p'` / `'cursor_t'` on multi-section detail pages.

### Decision matrix

| Concern | useSearchParams direct | useTableUrlState | TanStack-native URL |
|---|---|---|---|
| Lines of code per page | ~20-30 boilerplate per page (parse, type, setters, cursor reset) | ~5 lines (hook call) | N/A — TanStack has no URL persistence built-in |
| Type safety on cursor / sort | None (string \| null everywhere) | Typed `TableUrlState` ✓ | N/A |
| Side-effect rule "filter change drops cursor" | Easy to forget per page | Enforced centrally ✓ | N/A |
| Multi-section cursor keys | Manual per page | Supported via `cursorParam` ✓ | N/A |
| Memo reference stability | Manual `useMemo` per page | Built in ✓ | N/A |
| Lock-in / replaceability | Zero | Low — 130-line file, easy to inline | High — bind URL to query keys = big rewrite |
| Discoverability for new contributors | Familiar React Router API | One extra abstraction to learn | TanStack stores URL nowhere — requires bespoke plugin |
| DX | Verbose | Concise ✓ | N/A |
| Coupling to React Router | Indirect (used internally) | Indirect (same) | N/A |
| Test coverage | Per-page boilerplate to test | Single hook tested once | N/A |

### What TanStack actually provides (for the record)

TanStack Query has **no native URL persistence**. It manages cache for server state, keyed by `queryKey`. The convention `queryKey: ['transactions', cursor]` does mean "different cursor = different cache entry" but the cursor still needs to come from somewhere — and a refresh-survivable cursor lives in the URL, not in `useState`.

Therefore the choice is **not** "useTableUrlState vs TanStack-native"; it's "useTableUrlState vs raw useSearchParams". TanStack Query is orthogonal — server state cache hits stack on top of whatever URL-source the page chooses.

### Verdict: **KEEP** `useTableUrlState`

**Rationale (1 paragraph):** The abstraction is **thin (127 lines), justified, and pays back**. It centralizes (a) a typed URL → state mapping, (b) the "filter change drops cursor" invariant that's easy to forget per-page, (c) the `useMemo` reference-stability dance that's non-obvious, and (d) the multi-section cursor key convention (`cursor_p` / `cursor_t`) that 0254 made first-class via `cursorParam`. The alternative — inlining ~20-30 lines of `useSearchParams` boilerplate across 13 paginated pages — would be ~250-400 LOC of duplicated logic across pages, much of it subtly wrong (the "filter resets cursor" rule is exactly the kind of cross-page invariant that drifts when copy-pasted). Lock-in is minimal: the hook is a 1-file abstraction with a typed surface, replaceable in 1 PR if a better pattern emerges. The `useDetailMode` hook for tab-mode shows that NOT every URL-state needs this abstraction — it's reserved for the table-pagination invariant, which is the right scope. Recommendation **keep as-is**.

### Optional follow-up (Phase 3)

- Document the `useDetailMode` vs `useTableUrlState` decision in `lore/3-wiki/`: "When to reach for which URL-state hook" — clarifies for new contributors.
- Consider unifying `useDetailMode` into a thin generic over `useTableUrlState`, but only if a 3rd URL-state surface emerges (current 2 don't justify the abstraction).

## Summary

2 1.12 findings + EXTRA analysis verdict KEEP.

Total Wave 4 1.12 findings: 0 🔴, 0 🟠, 1 🟡, 1 🟢. Plus EXTRA decision artifact.
