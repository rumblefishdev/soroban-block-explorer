# AA — Overengineering (Wave 5 1.10b)

**Wave:** 5 (Tier 4 subjective)
**Stance:** "is this abstraction earning its keep, or could simpler suffice?"
**Date:** 2026-05-25
**Baseline SHA:** `81928602`.

## Per-check table

| # | Check | Verdict | Evidence | Severity | Class |
|---|---|---|---|---|---|
| AA-1 | Abstractions used only once | ⚠ | See per-abstraction inventory below | 🟢 / 🟡 | D |
| AA-2 | Generic types that could be concrete | ✓ | 1 generic in user code: `useIntersectionObserver<T extends Element = HTMLDivElement>` — justified (ref typing). No other unjustified generics. | — | — |
| AA-3 | Design patterns (Factory/Strategy/Observer) without justification | ✓ | 0 hits for Factory/Strategy/Observer named patterns. Only `Provider` instances are `RouterProvider` (react-router), `QueryProvider` (TanStack), `ExplorerThemeProvider` (MUI theme) — all framework-required. | — | — |
| AA-4 | State management library (Redux / Zustand) | ✓ | None. `package.json` has no `redux` / `zustand` / `jotai` / `mobx` / `recoil`. Only state libs: TanStack Query (server state), React Router (URL state via `useSearchParams` + `useTableUrlState` wrapper). **Senior choice — correctly resisted Redux defaultism.** | — | — |
| AA-5 | Custom hooks where inline is clearer | partial | See F-AA-1 — `useDebounced.ts` is 5 LOC (inline-or-extract toss-up). Most other hooks have ≥2 consumers and earn their keep. | 🟢 | D |
| AA-6 | Wrapper components without value | ✓ | No "<children/>"-only wrappers identified. `SectionCard` adds layout (chrome + padding + heading); `LazySection` adds intersection-observer gating; both have content. | — | — |
| AA-7 | Utility functions called only once | partial | See per-utility consumer-count inventory | 🟢 | D |

## Per-abstraction consumer-count audit

### Custom hooks

| Hook | Location | Consumer count | Justified? |
|---|---|---|---|
| `useTableUrlState` | `libs/ui/src/table/useTableUrlState.ts` | 1 direct (via `useCursorPagination`) + 13 indirect via paginated list/tab pages | ✓ — Wave 4 EXTRA verdict KEEP; centralizes URL ↔ typed state + cursor invariant |
| `useCursorPagination` | `libs/ui/src/table/useCursorPagination.ts` | ~13 pages (every paginated list + tab section) | ✓ |
| `usePageHandlers` | `libs/ui/src/table/usePageHandlers.ts` | ~13 pages (same set) | ✓ — Wave 4 F-X-3 positive note (extracted shared chunk post-0254) |
| `useTabUrlState` | `libs/ui/src/visualization/useTabUrlState.ts` | 5 consumers (`ContractDetailPage`, `Tabs.tsx`, etc.) | ✓ |
| `useIntersectionObserver` | `libs/ui/src/visualization/useIntersectionObserver.ts` | 1 direct (`LazySection.tsx`) | ⚠ See F-AA-1 |
| `useNow` | `libs/ui/src/timestamps/useNow.ts` | 2 (RelativeTimestamp, PollingIndicator) | ✓ |
| `useDebounced` | `web/src/search/useDebounced.ts` (5 LOC) | 1 (`useSearchResults.ts`) | ⚠ See F-AA-1 |
| `useDetailMode` | `web/src/pages/transaction-detail/useDetailMode.ts` | 1 (`transaction-detail/index.tsx`) | ✓ — page-local concern |
| `useTxHashParam` | `web/src/pages/transaction-detail/useTxHashParam.ts` | 1 (`transaction-detail/index.tsx`) | ✓ — page-local validation/param helper |
| `useColorMode` | `libs/ui/src/theme/ThemeProvider.tsx:65` | TBD (theme toggle) | ✓ — required by Provider/Consumer pattern |
| 26× `useXxx` API hooks | `web/src/api/hooks/*.ts` | 1 each (per page) | ✓ — 1-per-endpoint is the convention; each is a thin TanStack `useQuery` wrapper, not overengineering |

**Verdict:** every custom hook has clear consumers. Single-consumer hooks (`useDetailMode`, `useTxHashParam`, API hooks) are page-local helpers — colocation is correct, not over-abstraction.

### Generic types

Only 1 generic in user code:

```ts
// libs/ui/src/visualization/useIntersectionObserver.ts:23
export function useIntersectionObserver<T extends Element = HTMLDivElement>(...)
```

Justified — `Element` constraint allows the caller to type the ref correctly (`HTMLImageElement`, `HTMLDivElement`, etc.). Single legitimate use.

### Context providers

- `libs/ui/src/theme/ThemeProvider.tsx` → `ColorModeContext` (light/dark mode). Single context. Framework requirement (MUI theme).
- Plus framework Providers (`RouterProvider`, `QueryProvider`). Required.

**Verdict:** zero ad-hoc app-level context. **No global state abuse.**

## Findings

### F-AA-1 [Class D, Severity 🟢] — Single-consumer abstractions: trim candidates

Three single-consumer abstractions are inline-vs-extract toss-ups:

| Abstraction | Consumer | Trim? |
|---|---|---|
| `useIntersectionObserver` | only `LazySection.tsx` | Either keep as exported (anticipates future consumers) or inline into `LazySection.tsx`. Since `LazySection` is itself a libs/ui-exported utility, future consumers may want raw observer access. Subjective. **Keep recommendation.** |
| `useDebounced` (5 LOC) | only `useSearchResults.ts` | Trivial — 5 LOC, single call. Could be inlined into `useSearchResults`. But F-Y-2 finding identifies 4 NEW debounce-pattern sites that could consume an enriched `useDebounced` — keep + repurpose. |
| `useTxHashParam` | only `transaction-detail/index.tsx` | Page-local helper; correctly colocated; not exported. ✓ keep |

**Class:** D (informational; no fix).

**Subjective:** none of these are overengineering — they're "future-proofed at trivial cost" which is acceptable.

### F-AA-2 [Class A, Severity 🟢] — Zero Redux / Zustand: positive design choice (RECAP)

- **Evidence:** package.json has no global state library.
- **Why this is correct:** server state owned by TanStack Query; URL state owned by `useTableUrlState`; transient UI state owned by `useState` per component. Three layers, three correct tools.
- **Cross-cite:** Wave 4 AL-state-separation EXTRA verdict.
- **Class:** A — positive baseline.

### F-AA-3 [Class D, Severity 🟢] — `useDebounced` is single-consumer but Phase 3 F-Y-2 refactor will give it 4 more consumers

- **Location:** `web/src/search/useDebounced.ts`
- **Current consumer:** `useSearchResults.ts`
- **Phase 3 plan (per F-Y-2):** extract `useDebouncedDraft` hook (different signature — value+onChange+delay), reused across 4 filter components.
- **Decision:** keep file, possibly broaden it (one combined home for debounce helpers).
- **Class:** D — Phase 3 refactor.

### F-AA-4 [Class D, Severity 🟢] — `useIntersectionObserver` single-consumer, kept for libs/ui extensibility

- **Location:** `libs/ui/src/visualization/useIntersectionObserver.ts`
- **Sole consumer:** `LazySection.tsx` (same folder).
- **Justification:** exported from `libs/ui/src/index.ts` (line 118) as a public utility. Future LP detail chart sections / NFT lists may want intersection-observed lazy mount.
- **Decision:** keep + note in Phase 3 wiki that "single consumer today, public API for future".
- **Class:** D.

### F-AA-5 [Class A, Severity 🟢] — Provider count is minimal: ColorMode + Router + Query + Theme

- **Inventory:**
  - `<RouterProvider>` — React Router DOM v6 required
  - `<QueryProvider>` — TanStack Query required
  - `<ExplorerThemeProvider>` — MUI ThemeProvider + custom ColorModeContext (light/dark mode)
- **Verdict:** 3 providers; all framework-required. Zero "context-just-because" providers.
- **Class:** A — positive baseline.

### F-AA-6 [Class A, Severity 🟢] — Hook proliferation correctly bounded by colocation rule

- 26 `useXxx` API hooks in `web/src/api/hooks/` — 1-per-endpoint.
- 4 list/tab/page hooks in `libs/ui/src/table/` + `libs/ui/src/visualization/`.
- 3 utility hooks (`useNow`, `useDebounced`, `useIntersectionObserver`).
- 2 page-local hooks in `web/src/pages/transaction-detail/`.
- **Total: ~35 custom hooks.** No "useEverythingButton" / "useUtil" / "useHelpers" anti-patterns. Each has a single responsibility (cross-cite Wave 4 F-X table).
- **Class:** A — positive baseline.

## Cross-cites

- AA-4 ↔ Wave 4 X-coupling F-X-table row "Global state usage justified" (only ColorModeContext).
- AA-5 ↔ Wave 4 X-coupling F-X-4 (three-tier hook organization).
- AA-1 — first inventory of consumer counts; new in Wave 5.

## Net 1.10b finding count

6 findings: 0 🔴 / 0 🟠 / 0 🟡 / 6 🟢.

**Class breakdown:** A=3 / D=3.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. **F-AA-1 (trim candidates)** — none are clear-cut over-abstractions; user may agree or want to inline `useDebounced` into `useSearchResults` and re-extract a fresh hook for F-Y-2's debounce-draft pattern. Trade-off: minimal code reuse vs slight clarity.
2. **No subjective concerns of consequence.** Codebase is **not overengineered** — quite the opposite. **The audit's biggest risk in this area is under-abstraction** (formatter duplication per Wave 4/Wave 5 F-U / F-J / F-Y), not over-.

## Overall AA verdict (subjective)

**Codebase is admirably under-abstracted.** No design-pattern parade, no global state, no premature generics. The 1 generic + 3 single-consumer hooks are all defensible. **The team's discipline against overengineering is one of the audit's positive senior-craft findings.** The Phase 3 refactors identified across Wave 4 (component reuse) + Wave 5 (formatter unification, debounce extract) are anti-redundancy moves, not anti-overengineering moves.

## Top findings

None Class A/B/C; all D positive baselines or trim-toss-up notes.
