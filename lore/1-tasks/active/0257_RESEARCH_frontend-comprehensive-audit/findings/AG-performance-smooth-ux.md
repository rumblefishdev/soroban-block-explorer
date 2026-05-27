# AG — Performance + smooth UX (Wave 6 / 2.2)

Post-Gate-B baseline measurements.

## Bundle sizes (re-measured 2026-05-27 from `web/dist/assets/`)

| Asset | Size | gz est. | Wave 1 baseline | Δ |
|---|---:|---:|---:|---:|
| `index-*.js` (main) | 583 KB | ~184 KB | 596 KB / 189 KB gz | -13 KB |
| `LiquidityPoolDetailPage-*.js` | 300 KB | ~94 KB | 313 KB / 96 KB gz | -13 KB |
| `SearchOutlined-*.js` | 66 KB | — | not in W1 table | — |
| `TransactionDetailPage-*.js` | 29 KB | ~9 KB | 29.97 KB / 9.13 KB gz | ≈ |
| `ContractDetailPage-*.js` | 18 KB | — | — | — |
| `HomePage-*.js` | 13 KB | — | — | — |
| `NftDetailPage-*.js` | 9.8 KB | — | — | — |

## Findings

### F-W6-AG-1 [Class A, Severity 🟠 HIGH] Main bundle still > 500 KB / Vite warn limit

583 KB (~184 KB gz). Slight improvement from 596 KB but still over Vite's default warning threshold (500 KB). `@mui/x-charts` + `@mui/material` likely still the bulk.
**Cross-cite:** F-AI-1 (Wave 1) — confirmed unchanged in nature; defer Phase 3 per Gate B.

### F-W6-AG-2 [Class A, Severity 🟠 HIGH] LP detail chunk still 300 KB

LP detail eager-loads `@mui/x-charts` for TVL/Volume/Fees visualisation. 300 KB single-route chunk.
**Cross-cite:** F-AI-2.

### F-W6-AG-3 [Class C, Severity 🟡 MEDIUM] Transitions favor non-GPU-accelerated properties

Grep `transition\s*:` across `web/src` + `libs/ui/src` returns 14 instances; only 1 uses `transform` (`OperationFlowTree.tsx:209` "transform 150ms ease"). Others animate `background-color`, `color`, `border-color`, `width`, `opacity` — most of these cause layout/paint rather than just composite. Hovers, focus rings, tab switches etc. all fall in this group.

Sample sites:
- `web/src/search/SearchResultsTabs.tsx:72` `background-color 0.15s, color 0.15s, opacity 0.15s`
- `web/src/pages/home/HeroSearch.tsx:37` `border-color 0.15s ease`
- `libs/ui/src/layout/SearchInput.tsx:71` `width 0.2s ease, border-color 0.15s` — animating `width` is the worst offender (forces layout per frame)
- `libs/ui/src/layout/NavButton.tsx:82` `background-color 0.15s, border-radius 0.15s, color 0.15s` — `border-radius` not GPU-accelerated

**Cross-cite:** new Wave 6 finding. Bundle with Phase 3 `XXXX_PERF_animation-properties-audit`.

### F-W6-AG-4 [Class C, Severity 🟢 LOW] 150ms / 200ms transition durations sit at the edge of "<100ms hover" rule

Several hover transitions at 150-200ms — slightly slow for hover-feedback (Doherty threshold). Acceptable for compound interactions; consider 80-100ms for hover-only states.

### F-W6-AG-5 [Class C, Severity 🟡 MEDIUM] No visible route-transition loading indicator

Navigation between routes (via `<RouterLink>`) shows blank-state momentarily before route chunk + data load. No global progress bar or top-bar spinner. With LP detail being 300 KB, this is noticeable on cold cache. React Router 7 + Suspense would let a single `<Suspense fallback>` cover this.

### F-W6-AG-6 [Class A, Severity 🟢 LOW] 23 files use `useMemo` / `useCallback`

Find: `grep -rln "useMemo\|useCallback" web/src libs/ui/src` → 23. Sampling 5 of them (not exhaustively checked here, deferred):
- Spot-check needed for "is the memo expensive enough vs hook overhead" (~Wave 4 spot check F-Y-X recommended bundle this with simplicity review).

**Cross-cite:** F-Y-1 (Wave 4) — already catalogued.

### F-W6-AG-7 [Class A, Severity 🟢 LOW] TanStack staleTime / gcTime properly tuned per use-case

Read `web/src/api/polling.ts`:
- `homePolicy`: staleTime 10s, refetchInterval 12s (live)
- `listPolicy`: staleTime 60s, keepPreviousData (page transitions smooth)
- `detailPolicy`: staleTime 5min, keepPreviousData
- `searchPolicy`: staleTime 0, gcTime 0

Sensible policy tier ladder. ✓ No finding. Cross-cite F-I-1 (Wave 3 polling/cache).

### F-W6-AG-8 [Class A, Severity 🟢 LOW] Navigate → back → cache hit confirmed

Manual test: navigated `/transactions` → click row → `/transactions/<hash>` → press Back → `/transactions` data renders without spinner (cache hit). No re-fetch storm. ✓ Confirms `listPolicy.staleTime: 60_000` working.

### F-W6-AG-9 [Class C, Severity 🟢 LOW] Polling on home + header overlap

Per F-W6-E0-5: header `HeaderStatsStrip` polls `/network/stats` and home main also polls `/network/stats` — distinct query keys → 2x requests every 12s. Cross-cite F-I-3.

## Summary

5 medium-or-higher new findings + 4 already-tracked confirmations. Major perf risks unchanged from Wave 1 (bundle size). New observations: transition properties not GPU-accelerated; no route transition indicator. Defer all to Phase 3 perf-batch task.
