---
id: '0257'
title: 'Frontend comprehensive audit (pre-launch)'
type: RESEARCH
status: active
related_adr: ['0032']
related_tasks:
  [
    '0063',
    '0064',
    '0065',
    '0066',
    '0068',
    '0070',
    '0071',
    '0072',
    '0073',
    '0074',
    '0075',
    '0076',
    '0077',
    '0246',
    '0249',
    '0250',
    '0251',
  ]
tags:
  ['frontend', 'audit', 'qa', 'pre-launch', 'priority-high', 'phase-research']
links:
  - 'Prior QA pass: 0251 (frontend QA fixes batch, 13 bugs across 5 clusters, merged on develop 2026-05-24)'
history:
  - date: '2026-05-24'
    status: backlog
    who: karolkow
    note: 'Task created. Scope built interactively via 9-batch Q&A walkthrough — every sub-phase explicitly confirmed. User chose EXHAUSTIVE depth on state matrix (14×9=126), responsive (14×3=42), Figma fidelity (no time-box). 42 sub-phases total (Track 1: 29, Track 2: 8, Phase 3: 5), ~96.5h. Expected to spawn 20-50 backlog tasks.'
  - date: '2026-05-25'
    status: active
    who: karolkow
    note: 'Promoted backlog → active via /promote-task. Ready to begin Wave 1 execution.'
---

# Frontend comprehensive audit (pre-launch)

## Summary

Post-0251 (frontend QA fixes batch landing), run a comprehensive senior-fresh-eye
audit across all 14 routes covering 42 sub-phases. Scope built interactively
with karolkow via 9-batch Q&A walkthrough on 2026-05-24 — every sub-phase
explicitly confirmed (Tak / Nie / modyfikacja). User chose EXHAUSTIVE depth
on state matrix (14×9 = 126 cells), responsive (14×3 = 42 cells), and Figma
fidelity (no time-box). Several merged phases split back to granular per
user request. Output: 20-50 spawned backlog tasks (mix bug-fix / refactor /
docs / Future Work follow-up), severity-ranked.

**Senior stance:** spec / Figma / docs / task body / code = all someone's
interpretation, any can be wrong. Flag every conflict.

**Sequencing:** Track 1 (code-level, Claude solo) → Track 2 (visual + UX,
user review) → Phase 3 (consolidation + spawning). Rationale: structural /
contract issues fixed in Track 1 might invalidate Track 2 visual findings
(reimplemented component changes visual output).

## Status: Backlog

> 0251 merged on develop 2026-05-24 — audit baseline ready. Ready to promote
> backlog → active.

Estimated effort: ~96.5h = ~8 working days @ 12h focused, or ~12 days @ 8h
normal pace, or 2-2.5 weeks part-time.

## Context

15 bugs found in post-0077 Playwright-MCP QA traversal. 0251 fixes 13 of them.
QA pass only covered render correctness, console errors, network status, basic
empty/not-found, spot-check correctness. Many dimensions NOT covered:

- OpenAPI strict adherence (no manual fetches, no extension fields, no reimpl)
- Performance + smooth UX (useMemo/useCallback proper, no unnecessary refetch,
  no layout shifts, no scroll jumps)
- File/folder structure consistency
- Figma fidelity 1:1
- Responsive (mobile/tablet/desktop)
- Live indicator logic (currently always shown — bug to confirm)
- Component reuse / no senseless reimplementation
- Dead code, overengineering, hallucination check
- Source consistency (spec ↔ task ↔ docs ↔ Figma ↔ code)
- Maintenance cost projection
- Senior craft quality
- Coupling / decoupling
- Project-wide consistency
- Lore process compliance

Plus archaeology: every FE-related lore task in archive has Future Work,
Issues Encountered, Design Decisions (Emerged) sections that may have
unspawned follow-ups.

This audit is the gate before declaring frontend ready for launch.

## Implementation Plan

### Track 1 — Code-level audit (29 sub-phases, ~63h, Claude solo)

#### MUST (1.0–1.6, ~27h)

**1.0 Archaeology (~2h)**

Sweep FE tasks (0063-0077, 0249-0251) for:

- `## Future Work` from each archived task → known deferred list
- `## Issues Encountered` → known broken/workaround state
- `## Design Decisions (Emerged)` → autonomous decisions worth audit
- Grep code for `TODO`, `FIXME`, `XXX`, `nit`, `follow-up`, `defer`
- Unchecked `[ ]` in `## Acceptance Criteria` across archived tasks
- Cross-ref: every deferred item should have spawned backlog task; flag gaps
- Blocked tasks never unblocked

Output: `findings/00-archaeology.md`.

**1.1 OpenAPI strict adherence (~3h)**

- Every fetch through `@rumblefish/api-types` generated client?
- No manual `fetch()` / `axios` outside generated client?
- No component extends response shape (adds fields not in schema)?
- No response reimplemented locally (manual types vs generated)?
- No `as PoolItem` / `as any` bypassing schema mismatch?
- Single layer abstraction over generated client (`web/src/api/`)?
- Query keys map 1:1 to endpoint paths?
- Response mappers (if any) documented + justified?
- Mock fixtures generated from OpenAPI (no drift)?
- CI gate "API types freshness" enforced on every PR?

**1.2 Spec / source consistency (~4h)**

- Each endpoint matches spec lore task? Response fields = spec fields?
- Spec updated after actual code changes (drift detection)?
- OpenAPI vs `crates/api/**` zero-diff?
- CI gate "API types freshness" green?
- FE uses generated types, no inline `any`?
- Each unchecked acceptance criterion has spawned task?
- ADR 0032 evergreen docs gate — every API-shape change has `docs/architecture/**` updated?
- Per feature: spec ↔ task ↔ Figma ↔ docs ↔ code consistent?
- Each inconsistency documented as deviation note in task body?
- Backend response shape vs spec promise consistent?
- FE response handling vs `@rumblefish/api-types` consistent?
- Test fixtures match prod response?

**1.3 File / folder structure (~2h)**

- Each file in sensible location per project convention?
- Folder name matches main component / concept?
- Shared utilities in `libs/`, feature-specific in `web/src/pages/`?
- No "lonely" files that fit nowhere?
- Filename matches exported symbol (PascalCase.tsx for component, camelCase.ts for utils)?
- No inconsistency (some features have subfolders, others don't)?
- `index.ts` barrel files used consistently?
- Tests next to code (`Component.test.tsx`) or in `__tests__/`?
- Assets in `public/` vs imported?
- `libs/ui` / `libs/api-types` / `web/` boundaries clear?
- Naming `page` vs `view` vs `screen` consistent?

**1.4 API consistency cross-checks (~3h)**

- Case sensitivity tolerance (per H2 lowercase op param bug)?
- Enum coverage (per H6 5 vs 27 ops)?
- Polymorphic IDs documented + consistent FE use (per H3 asset URLs)?
- Date/timezone semantics (UTC + RFC3339)?
- Number precision (NUMERIC stringification trim, per B2)?
- Null vs missing field handled (`?? '—'` everywhere)?
- Cursor pagination semantics consistent?
- Polling cache headers respected?
- CORS production-configured?

**1.5 State coverage matrix EXHAUSTIVE 14×9 = 126 cells (~8h)**

Full matrix: every endpoint × every state. CSV output.

**Endpoints (14):** E0 shell, E1 `/`, E2 `/transactions`, E3 `/transactions/:hash`,
E4 `/ledgers`, E5 `/ledgers/:seq`, E6 `/accounts/:id`, E7 `/assets`,
E8 `/assets/:id`, E9 `/contracts/:id`, E10 `/nfts`, E11 `/nfts/:id`,
E12 `/liquidity-pools`, E13 `/liquidity-pools/:id`, E14 `/search`.

**States (9):**

- D1 Loading skeleton (right shape, right rows, no white screen)
- D2 Success render
- D3 Empty (entity-specific message + action)
- D4 NotFound (404, entity-specific)
- D5 Validation (400, per H8 → NotFound or own state)
- D6 Transient (5xx + retry)
- D7 Rate limit (429 + retry)
- D8 CORS block (readable error)
- D9 Polling stale indicator

Output: `findings/D-state-coverage-matrix.csv`.

**1.6 Console + error handling (~3h)**

- Zero ERROR per route happy path?
- Zero WARN per route?
- React duplicate-key warnings (per B5)?
- Deprecated lifecycle warnings?
- Strict-mode double-render side effects?
- Source map present dev?
- Network 4xx/5xx only on legit error tests?
- Every `try/catch` has logger or user-feedback?
- No silent exception swallow?
- Every `throw` has fallback in boundary?
- Network errors → user-actionable message?
- Async hooks (TanStack) — error state properly propagated?
- Form validation — graceful, not blocking submit?
- Hard-fail decisions (`assetLegLabel`, `classifyLpTx`) documented?

#### HIGH (1.7–1.10d, 1.15, 1.18, ~24h)

**1.7 Cross-entity link integrity (~2h)**

Matrix N×N entity types — żaden dead link:

- Account in tx row → account page works?
- Account balance row → asset page works?
- Asset issuer link → account page works?
- Contract in tx → contract page works?
- Ledger in tx → ledger page works?
- Pool in tx → pool page works?
- NFT collection → NFT page works?
- Sender/receiver in tx → respective accounts works?

**1.8 Data formatting consistency (~2h, grep-driven)**

- Numbers — `formatAmount` / `formatCompactAmount` used everywhere?
- Stroop ↔ XLM divide by 1e7 in single util, used everywhere?
- Timestamps — relative + absolute UTC consistent?
- Addresses truncation `4+4` consistent everywhere?
- Hashes truncation consistent?
- Strkeys vs hex — display vs URL strategy consistent (per B1)?
- Asset labels with issuer disambig (per B4)?
- Percentages decimal places consistent (per B2)?
- Status badge colors consistent (Success=emerald, Failed=red)?
- Currency symbol XLM everywhere?

**1.9 U Component reuse (~2h, split from U+X+Y merged per user)**

- Basics (`Chip`, `IdentifierDisplay`, `ExplorerTable`, `SectionCard`, `EmptyState`, `*ErrorState`) used everywhere sensible?
- No component reimplemented locally instead of imported from `libs/ui`?
- Formatters used everywhere vs inline `Number(x).toFixed()`?
- Each detail page uses same pattern (breadcrumb + heading + SectionCard + SectionErrorBoundary)?
- Each list page uses same pattern (filter bar + ExplorerTable + PaginationControls + useTableUrlState + useInfinitePager)?

**1.9b X Coupling/decoupling (~2h, split)**

- `libs/ui` no dependency on `web/` (1-way)?
- `libs/api-types` no dependency on `libs/ui` or `web/`?
- Each page-level component extractable independently?
- Unnecessary props passed (prop drilling)?
- Global state not abused?
- Each custom hook single responsibility?
- No cycles between modules?
- API client single entry point?

**1.9c Y Simplicity (~2h, split)**

- Most complex component — justified?
- Longest file — should be split?
- Deepest conditional nesting — simplifiable?
- Copied blocks (3+ occurrences) — extract?
- `useEffect` used where `useMemo` / event handler suffices?
- `useState` that should be URL state?
- Local inline components where shared component fits?

**1.10 Z Senior craft (~1.5h, split from Z+AA+AB+AD merged per user)**

- Anything a senior FE-developer would write completely differently?
- Naming idiomatic (PascalCase components, use\* hooks, lowercase helpers, Type/Item suffix)?
- File structure discoverable (juniors find files in 30s)?
- Code smells (god components, magic numbers, deep prop drilling, exception swallowing)?
- Each public API has JSDoc?
- Comments explain **why** not **what** (per `assetLegLabel` rationale)?
- Error throws have informative messages?

**1.10b AA Overengineering (~1.5h, split)**

- Abstractions used only once?
- Generic types that could be concrete?
- Design patterns (Factory / Strategy / Observer) without justification?
- State management layer (Redux / Zustand) needed at all?
- Custom hooks where inline is clearer?
- Wrapper components without value?
- Utility functions called only once?

**1.10c AB Hallucination (~1.5h, split)**

- Each divergence from project convention — explicit in task or invented?
- Each Design Decision (Emerged) — justified or hallucinated?
- Each `as any` / `@ts-ignore` — justified?
- Implementations inconsistent with rest of project pattern?
- Spec says X but code does Y without task note?
- Comment-out leftover suggesting false starts?

**1.10d AD Maintenance cost (~1.5h, split)**

- Junior developer can change something without predecessor context?
- Bug fix requires changes in 5+ files (sign of leaked concern)?
- Each component has unit test protecting against regression?
- Implicit dependencies (components requiring specific parent context)?
- Magic strings / numbers without constants?
- Onboarding doc for FE exists?

**1.15 Stellar domain consistency (~3h)**

- Strkey vs hex strategy consistent across display + URL?
- XDR rendering — where decoded vs raw (flag TODOs)?
- Operation type → icon mapping consistent?
- Asset SEP-1 TOML enrichment handled per-page?
- Soroban-era ledger detection (>= 50,457,424) — where affects?
- Mainnet vs Testnet config single source?
- Network passphrase usage?
- Stroop ↔ XLM conversion central util?

**1.18 Q+AR Lore process + commit conventions (~3h, merged per user)**

- Each archived task has complete Acceptance Criteria marked?
- Each has Design Decisions (From Plan + Emerged)?
- Each has Issues Encountered?
- API-touching tasks have openapi regen committed?
- Schema-touching tasks have `docs/architecture/**` updated (ADR 0032)?
- ADRs cross-referenced in task frontmatter?
- Commit messages `feat(lore-NNNN): ...` enforced (commitlint)?
- PR descriptions reference lore task?
- Design Decisions (Emerged) have alternatives considered?
- Future Work converted to spawned backlog tasks?
- PR template used on every PR?
- Branch naming `feat/NNNN_slug` consistent?
- Husky pre-commit (lint-staged) working?
- Branch protection on develop?
- CHANGELOG.md updated per release?

#### MEDIUM (1.11–1.14, 1.16, 1.17, ~12.5h)

**1.11 P Code quality (~1.5h, split from P+AQ merged per user)**

- `tsc --noEmit` zero errors?
- ESLint zero warnings?
- No `any` casts (grep `as any`)?
- No `@ts-ignore` / `@ts-expect-error`?
- No `console.log` leftover?
- No dead exports (`knip` / `ts-prune`)?
- No leftover mocks (per H4 `MOCK_STATS`)?
- No commented-out blocks?
- Bundle dups?
- Cyclical imports?

**1.11b AQ Type safety depth (~1h, split)**

- `tsconfig.json` strict + `noUncheckedIndexedAccess` + `noFallthroughCasesInSwitch`?
- Exhaustive switches (`never` default)?
- Discriminated unions properly narrowed?
- Branded types (`AccountId` ≠ `AssetId` at type level or via zod)?
- Generic constraints sensible?

**1.12 State separation + EXTRA useTableUrlState analysis (~2.5h)**

- Server state (TanStack) vs UI state (useState) vs URL state (useTableUrlState) — clean separation?
- Global state — justified scope?
- Components with local state that should be URL?
- URL state that should be local?
- Prop drilling >3 levels — extract context or colocate?
- `useReducer` vs multiple `useState` consistent?

**EXTRA per user 2026-05-24:** analiza czy `useTableUrlState` w ogóle potrzebny
skoro TanStack jest. Różne warstwy (TanStack=server state, useTableUrlState=URL
state) — sprawdzić justification + alternatywy (TanStack native URL persistence?
React Router search params direct? custom minimal hook?). Output: trade-off
analysis + recommendation (keep / refactor / drop).

**1.13 URL state nav functional (~2h)**

- Filter state in URL → refresh preserves?
- Pagination cursor in URL → refresh preserves?
- Tab state preserves?
- Sort state preserves?
- Deep link from external renders correctly?
- URL encoding edge cases (special chars in query, slashes in IDs)?
- Trailing slash handling?
- Custom 404 catch-all route?

**1.14 Search functional (~1h)**

- Hash full-match → redirect tx?
- G... → account?
- C... → contract?
- L... → pool?
- Asset code partial → list?
- Empty query handled?
- Very long query (>500 chars) handled?
- Special chars no injection?
- Debounce no API spam?

**1.16 Bundle analysis (~2h)**

- `nx build` + `vite-bundle-visualizer` — biggest deps justified?
- Duplicate deps (multiple versions same lib)?
- Tree-shaking working (lodash-es per-method)?
- Code-split per route (lazy + Suspense)?
- Vendor chunk size?
- CSS chunk size?

**1.17 Security (subset, ~2h)**

- Console no secrets/env leak?
- XSS — user strings escaped (asset codes, contract names, memos)?
- Link injection guard (`javascript:`)?
- Iframe sandbox (NFT media)?
- Local storage / cookies content?

#### LOW quick wins (1.19–1.22, ~4h)

**1.19 Build & deploy hygiene (subset, ~1h)**

- `.env.example` documented vs actual `VITE_*` vars?
- No hardcoded `localhost:9000` in code?
- Production bundle — no `console.log` leftover?
- Secret scanning — no API keys in git history?

**1.20 Quick wins (DM+DN+CA, ~1h)**

- "All systems operational" footer indicator — connected to real status check or hardcoded? (bug if hardcoded)
- Build version / commit SHA displayed somewhere?
- Footer Terms of Service / Privacy / Cookies → real pages or dead links?
- Footer external links (GitHub, Stellar docs, Soroban docs) open in new tab?

**1.21 Dependency hygiene (~1h)**

- `npm outdated` — major versions available?
- `npm audit` — zero high/critical?
- Snyk integrated (if used)?
- `license-checker` — all deps compatible license?

**1.22 Polling / cache logic (~1h)**

- Which endpoints polled?
- Polling interval per volatility?
- Pause on tab inactive (`visibilitychange`)?
- Cache invalidation on user action?
- TanStack dedupes multiple components subscribing same endpoint?

### Track 2 — Visual + UX audit (8 sub-phases, ~25.5h, Claude + user review)

#### MUST (2.0, 2.3, ~4.5h)

**2.0 Playwright MCP full re-pass (~4h)**

Same methodology as 0251-birthing pass, against post-fix baseline. All 14 routes:

- Happy path snapshot
- Invalid id / 404 / 400 / 500 scenarios
- Empty filter / no data
- All cross-entity links (matrix)
- Network requests audit per route
- Console errors + React warnings (target: 0)
- Tabs + filters + pagination state preservation
- Search edge cases

Output: `findings/playwright-pass/EXX-route.md` per endpoint.

**2.3 V Live indicator logic (~30 min)**

User explicit: bug already identified — zawsze pokazuje.

- Where in UI is "live" / "active" indicator?
- Has actual freshness logic (latest ledger close < X seconds ago)?
- When backfill runs on historical data, "live" disables?
- Current behavior confirm: always shown.

#### HIGH (2.1, 2.2, 2.2b, 2.4, ~21h)

**2.1 Figma fidelity BEZ time-box (~6-8h, user upgraded from time-boxed)**

Per `feedback_figma_first`. Pixel-perfect 1:1.

- Each view 1:1 with Figma — spacing/typography/colors?
- Components match Figma design system tokens?
- Dark mode (if in Figma) implemented?
- Missing Figma components — what wasn't built?
- Components without Figma source — justified (deviation note)?
- Hover/focus/active states match?
- Empty state mockups vs real?
- Loading skeletons match Figma shape?

**2.2 AG Performance + smooth UX (~3h, split from AG+AP merged per user)**

- `useMemo` only where expensive (overhead < savings)?
- `useCallback` only where ref-stability matters?
- `React.memo` only on components with stable props?
- **No unnecessary refetch** — TanStack `staleTime` / `gcTime` tuned per endpoint?
- Navigate to detail → back → query cache hit, no refetch?
- **No layout shifts (CLS)** — skeleton heights match real content?
- Image dimensions known before load?
- Smooth scroll — virtualization on long lists (>50 rows)?
- Animations use only `transform` + `opacity` (GPU)?
- No `will-change` overuse?
- Loading transitions without visual jank?
- Hover transitions <100ms?
- Route transition has loading indicator (no blank screen)?
- React DevTools Profiler — components rendering >2× per interaction?
- Bundle initial JS size <250KB gz per route?

**2.2b AP Loading patterns (~1h, split)**

- Skeleton vs spinner choice consistent per context?
- Inline loading vs overlay vs full-page consistent?
- Error retry → re-loading state same as initial?
- Polling refresh visual indicator vs silent?

**2.4 Responsive EXHAUSTIVE 14×3 = 42 cells (~7h, user upgraded from sampled)**

Full matrix: every route × every breakpoint.

**Routes (14):** all from state matrix.

**Breakpoints (3):** mobile 375px / tablet 768px / desktop 1280px.

Per cell:

- Layout breaks?
- Tables responsive (horizontal scroll / card layout)?
- TopNav responsive (hamburger menu)?
- Touch targets >44px?
- Modals/dialogs full-screen on mobile?
- Charts responsive?

Output: `findings/R-responsive-matrix.csv`.

#### MEDIUM (2.5, 2.6, ~3h)

**2.5 F+CH A11y visual + auto (~2h)**

Lighthouse a11y audit per route + manual focus check.

- ARIA roles correct (Lighthouse + manual)?
- Heading hierarchy?
- Keyboard nav — tab order logical, focus visible?
- Screen reader labels?
- Color contrast (WCAG AA, Lighthouse)?
- Focus trap in modals?
- Form labels?
- Reduced-motion respect (`prefers-reduced-motion`)?
- **Color blindness compliance** — status badges have icon + label fallback, not just color (protanopia / deuteranopia)?

**2.6 AK CSS theme consistency (~1h, grep + visual sample)**

- Theme tokens used everywhere?
- No hardcoded hex — all from palette?
- Spacing scale (4/8/16/24/32) consistent?
- Z-index strategy (constants: header=100, modal=1000, toast=2000)?
- Border-radius scale (4/8/12) consistent?
- Shadow scale (sm/md/lg) consistent?
- CSS approach single primary, no mix?

### Phase 3 — Consolidation & spawning (5 sub-phases, ~8h)

**3.1 Aggregate findings (~2h)**

Consolidate `findings/` into `consolidated-bugs.md`, severity-ranked
(🔴 CRITICAL / 🟠 HIGH / 🟡 MEDIUM / 🟢 LOW).

**3.2 Spawn backlog tasks (~4h)**

Per finding cluster: spawn `lore/1-tasks/backlog/XXXX_BUG|REFACTOR|FEATURE_*.md`
with frontmatter `related_tasks: ['0257']`, severity tag, effort estimate
(S/M/L), dependencies (blocked-by / blocks).

**3.3 Cluster small fixes into batches (included in 3.2)**

Per 0251 model: small fixes grouped by file area into 1-2 batch tasks
(avoid spawning 30 trivial PRs).

**3.4 Write audit-summary.md (~1h)**

TL;DR for team + headline numbers + dropped-scope follow-up list + backfill
state snapshot (start + end of audit).

**3.5 Update `lore/3-wiki/` (~1h)**

If patterns emerge worth documenting (e.g., FE testing standards, formatter
conventions, error state taxonomy).

## Total effort

| Track                   | Sub-phases | Hours     |
| ----------------------- | ---------- | --------- |
| Track 1 (code-level)    | 29         | ~63       |
| Track 2 (visual + UX)   | 8          | ~25.5     |
| Phase 3 (consolidation) | 5          | ~8        |
| **Total**               | **42**     | **~96.5** |

**Calendar:** ~8 working days @ 12h focused, or ~12 days @ 8h normal, or
2-2.5 weeks part-time.

## Out of scope (DROPPED — spawn as follow-up tasks in Phase 3)

User confirmed 2026-05-24: all 14 dropped areas stay dropped. Spawn as
separate backlog tasks during Phase 3.

| Dropped area                  | Follow-up task name                      |
| ----------------------------- | ---------------------------------------- |
| O testing coverage            | `XXXX_FEATURE_frontend-testing-baseline` |
| N i18n readiness              | `XXXX_FEATURE_frontend-i18n` (warunkowo) |
| AJ asset optimization         | spawn if perf issues found in audit      |
| AT animation polish           | spawn if specific complaint surfaces     |
| S browser compat matrix       | `XXXX_FEATURE_browser-compat-ci`         |
| T production parity           | post-prod-up audit                       |
| BR Open Graph / Twitter cards | `XXXX_FEATURE_frontend-og-meta`          |
| BM long-running tab leaks     | `XXXX_RESEARCH_frontend-memory-leaks`    |
| BJ WebSocket / SSE            | `XXXX_RESEARCH_frontend-realtime`        |
| BV offline / service worker   | `XXXX_FEATURE_frontend-pwa`              |
| BZ GDPR / cookie banner       | `XXXX_COMPLIANCE_frontend-gdpr`          |
| CE command palette            | `XXXX_FEATURE_frontend-command-palette`  |
| CF export CSV/JSON            | `XXXX_FEATURE_frontend-data-export`      |
| BO session replay             | skip unless team requests                |

## Acceptance Criteria

- [ ] Track 1 — Phase 1.0 archaeology done; all known deferred items cross-referenced
- [ ] Track 1 — All MUST sub-phases (1.0–1.6) covered with findings
- [ ] Track 1 — All HIGH sub-phases (1.7, 1.8, 1.9, 1.9b, 1.9c, 1.10, 1.10b, 1.10c, 1.10d, 1.15, 1.18) covered
- [ ] Track 1 — All MEDIUM sub-phases (1.11, 1.11b, 1.12, 1.13, 1.14, 1.16, 1.17) covered
- [ ] Track 1 — All LOW quick wins (1.19, 1.20, 1.21, 1.22) covered
- [ ] Track 1 — Phase 1.12 EXTRA: useTableUrlState justification analysis written
- [ ] Track 1 — Phase 1.5 state matrix CSV has 126 rows (14×9), no gaps
- [ ] Track 2 — Playwright MCP full re-pass done; report per route
- [ ] Track 2 — All sub-phases covered (Figma fidelity, perf, live, responsive, a11y, CSS)
- [ ] Track 2 — Phase 2.4 responsive CSV has 42 rows (14×3), no gaps
- [ ] Phase 3 — `consolidated-bugs.md` severity-ranked
- [ ] Phase 3 — Spawned backlog tasks created with frontmatter `related_tasks: ['0257']`
- [ ] Phase 3 — `audit-summary.md` written for team
- [ ] Phase 3 — Out-of-scope follow-up tasks spawned (per Out of scope table)
- [ ] **Docs updated** — `N/A — audit task, no architecture change.` Per ADR 0032.
- [ ] **API types regenerated** — `N/A — no changes under crates/api/**, Cargo.{toml,lock}, libs/api-types/**.`

## Critical files to read at audit start

- `lore/1-tasks/archive/0063-0077_*.md` (all FE tasks)
- `lore/1-tasks/archive/0249_*.md`, `lore/1-tasks/active/0250_*.md`
- `lore/1-tasks/backlog/0251_BUG_frontend-qa-fixes-batch.md` (recently fixed bugs)
- `lore/2-adrs/0032_*.md` (evergreen docs gate)
- `libs/api-types/src/openapi.json` (generated client baseline)
- `web/src/api/**` (FE API layer)
- `libs/ui/src/**` (shared UI primitives)
- `web/src/pages/**` (page components per route)
- `web/src/router/routes.ts` (URL helpers)
- `web/src/pages/format.ts` (formatters)
- `crates/api/src/assets/handlers.rs` (`:id` polymorphic accepted formats reference)
- `crates/domain/src/enums/operation_type.rs` (27 ops enum reference)
- `web/src/pages/transactions/operationTypes.ts` (FE op enum)

## Output structure

```
0257_RESEARCH_frontend-comprehensive-audit/
├── README.md                             # this file
├── findings/
│   ├── 00-archaeology.md                 (1.0)
│   ├── AF-openapi-adherence.md           (1.1)
│   ├── A-AC-spec-source-consistency.md   (1.2)
│   ├── AH-file-folder-structure.md       (1.3)
│   ├── C-api-consistency.md              (1.4)
│   ├── D-state-coverage-matrix.csv       (1.5, full 14×9 = 126 cells)
│   ├── M-AE-console-error-handling.md    (1.6)
│   ├── K-cross-entity-links.md           (1.7)
│   ├── J-data-formatting.md              (1.8)
│   ├── U-component-reuse.md              (1.9)
│   ├── X-coupling.md                     (1.9b)
│   ├── Y-simplicity.md                   (1.9c)
│   ├── Z-senior-craft.md                 (1.10)
│   ├── AA-overengineering.md             (1.10b)
│   ├── AB-hallucination.md               (1.10c)
│   ├── AD-maintenance-cost.md            (1.10d)
│   ├── P-code-quality.md                 (1.11)
│   ├── AQ-type-safety.md                 (1.11b)
│   ├── AL-state-separation.md            (1.12, includes useTableUrlState analysis)
│   ├── E-url-state-functional.md         (1.13)
│   ├── L-search-functional.md            (1.14)
│   ├── AN-stellar-domain.md              (1.15)
│   ├── AI-bundle-analysis.md             (1.16)
│   ├── H-security-subset.md              (1.17)
│   ├── Q-AR-lore-process-conventions.md  (1.18)
│   ├── AO-build-deploy.md                (1.19)
│   ├── quick-wins-DM-DN-CA.md            (1.20)
│   ├── CO-dependency-hygiene.md          (1.21)
│   ├── I-polling-cache.md                (1.22)
│   ├── playwright-pass/                  (2.0)
│   │   ├── E0-shell.md
│   │   ├── E1-home.md
│   │   ├── E2-transactions-list.md
│   │   ├── E3-transactions-detail.md     # stub flagged
│   │   ├── E4-ledgers-list.md
│   │   ├── E5-ledgers-detail.md
│   │   ├── E6-accounts-detail.md
│   │   ├── E7-assets-list.md
│   │   ├── E8-assets-detail.md
│   │   ├── E9-contracts-detail.md
│   │   ├── E10-nfts-list.md
│   │   ├── E11-nfts-detail.md
│   │   ├── E12-liquidity-pools-list.md
│   │   ├── E13-liquidity-pools-detail.md
│   │   └── E14-search.md
│   ├── B-figma-fidelity.md               (2.1, no time-box)
│   ├── AG-performance-smooth-ux.md       (2.2)
│   ├── AP-loading-patterns.md            (2.2b)
│   ├── V-live-indicator.md               (2.3)
│   ├── R-responsive-matrix.csv           (2.4, full 14×3 = 42 cells)
│   ├── F-CH-a11y-color-blind.md          (2.5)
│   ├── AK-css-theme.md                   (2.6)
│   ├── consolidated-bugs.md              (3.1, final severity-ranked)
│   ├── dropped-scope-followups.md        (3.2, 14 follow-up tasks list)
│   └── audit-summary.md                  (3.4, TL;DR for team)
├── spawned-tasks/                        (3.2, populated during Phase 3)
│   └── (~20-50 .md files, one per spawned backlog task)
└── worklog/                              (per lore-framework convention)
    └── YYYY-MM-DD-session.md             (one per audit session)
```

## Methodology

- **Senior fresh-eye stance:** spec / Figma / docs / task body / code — all are
  someone's interpretation, any can be wrong. Flag every conflict.
- **Read-only audit:** no code edits during the audit pass. Findings only.
  Fixes spawn into separate backlog tasks in Phase 3.
- **Playwright MCP:** live exploration only (per `[[feedback_playwright_mcp_vs_cli]]`).
  CLI Playwright for any regression test that's spawned.
- **Backfill state captured:** start + end of audit in `audit-summary.md`
  (drift between Track 1 + Track 2 documented).

## Resume in new session

A fresh Claude session picking up this task should:

1. Read this README in full
2. Read recent FE-related archived tasks (0063-0077 list in frontmatter)
3. Read prior QA findings: `lore/1-tasks/archive/0251_BUG_frontend-qa-fixes-batch.md`
4. Verify local stack: Vite `:4200` + API `:9000` + Postgres `:5433` running
5. Verify backfill state (snapshot in `findings/audit-summary.md` once started)
6. Promote `backlog/ → active/` via `git mv`, then update `status: backlog → active`
7. Start with Wave 1 (see Execution Strategy below) — 4 parallel Explore subagents
8. After each wave, write findings to `findings/<area>.md` per Output structure
9. No commits without explicit user signal (per CLAUDE.md project rule)

## Execution Strategy

Sub-phases ranked by confidence (deterministic vs subjective judgment) and
upstream effect (does this finding unlock or invalidate other sub-phases?).
Higher-confidence + higher-upstream phases run first.

### Confidence tiers

**Tier 1 — pure deterministic** (auto-tooling output / grep / file inspection;
zero subjective judgment; max upstream effect):

- 1.0 Archaeology (read-only lore sweep)
- 1.1 OpenAPI strict adherence (grep `fetch(`, `axios`, `as any`, `as PoolItem`)
- 1.11 P Code quality (`tsc --noEmit`, `eslint`, `knip`, `ts-prune`)
- 1.11b AQ Type safety depth (tsconfig flags, ESLint AST rules)
- 1.16 Bundle analysis (`vite-bundle-visualizer`)
- 1.19 Build & deploy hygiene (grep, gitleaks/trufflehog)
- 1.20 Quick wins (status indicator / version / footer link inspection)
- 1.21 Dependency hygiene (`npm outdated`, `npm audit`, `license-checker`)

**Tier 2 — mostly deterministic** (grep-heavy or Playwright pass/fail):

- 1.4 API consistency cross-checks
- 1.7 Cross-entity link integrity
- 1.8 Data formatting consistency
- 1.13 URL state nav functional
- 1.14 Search functional
- 1.17 Security subset
- 1.18 Q+AR Lore process + commit conventions
- 1.22 Polling / cache logic

**Tier 3 — Claude solo but some judgment** (grep + interpretation):

- 1.5 State coverage matrix 14×9 = 126 (Playwright marathon)
- 1.6 Console + error handling
- 1.9 U Component reuse
- 1.9b X Coupling
- 1.12 State separation + EXTRA useTableUrlState analysis
- 1.15 Stellar domain consistency

**Tier 4 — heavy subjective** (Claude drafts findings, user spot-checks):

- 1.2 Spec/source consistency
- 1.3 File/folder structure
- 1.9c Y Simplicity
- 1.10 Z Senior craft
- 1.10b AA Overengineering
- 1.10c AB Hallucination
- 1.10d AD Maintenance cost

**Tier 5 — visual, user review required:**

- All Track 2 sub-phases (2.0–2.6)

### Quick start — top 3 to run solo first

If starting cold and unsure where to enter, run these three sequentially:

1. **1.0 Archaeology** — foundation for Phase 3 spawning; output = ground-truth
   deferred-items list cross-referenced to existing-or-missing spawned tasks
2. **1.11 P Code quality** — `tsc`/`eslint`/`knip` errors may block subsequent
   audits; fix-first if any
3. **1.21 Dep hygiene** — `npm audit` may surface CVE blockers requiring
   immediate dependency upgrade before audit continues

All three together = ~4-5h, fully deterministic, zero user input needed
between them.

### Waves — parallelized execution plan

Wave splitting maximizes Claude throughput via parallel Explore subagents
(max 3 per single Agent tool call). Order matters: each wave consumes findings
from the previous when downstream check requires it.

**Wave 1 — Tier 1 deterministic foundations (~3h, 4 parallel agents)**

- Agent A: 1.0 Archaeology
- Agent B: 1.11 P + 1.11b AQ Type safety (auto-tooling combo)
- Agent C: 1.1 OpenAPI strict adherence (grep-heavy)
- Agent D: 1.21 Dependency hygiene + 1.16 Bundle analysis (CLI tooling)

After Wave 1: write findings, then continue.

**Wave 2 — Tier 1 cleanup + Tier 2 start (~3h, 3 parallel agents)**

- Agent A: 1.4 API consistency + 1.8 Data formatting (grep)
- Agent B: 1.19 Build & deploy hygiene + 1.20 Quick wins (grep + UI inspect)
- Agent C: 1.18 Q+AR Lore process (frontmatter cross-ref to body sections)

**Wave 3 — Tier 2 Playwright + grep (~3h)**

- Playwright session: 1.7 Cross-entity links + 1.13 URL state nav + 1.14 Search
  (single MCP run hitting all routes)
- Parallel Explore agent: 1.17 Security + 1.22 Polling/cache (grep)

**Wave 4 — Tier 3 sequential (~10h)**

- 1.5 State coverage matrix 14×9 = 126 (dedicated Playwright marathon, ~8h)
- 1.6 Console + error handling (parallel during 1.5 Playwright pauses)
- 1.9 + 1.9b + 1.15 (grep-heavy, can run after 1.5/1.6)
- 1.12 State separation + EXTRA useTableUrlState analysis

**Wave 5 — Tier 4 subjective drafts (~8h)**

- 1.2 Spec/source consistency
- 1.3 File/folder structure
- 1.9c Y Simplicity
- 1.10 + 1.10b + 1.10c + 1.10d (senior smell tests)

User spot-check each Tier 4 finding before consolidation — drafts may need
trim or expansion based on user judgment.

**Wave 6 — Track 2 visual + UX (~25h, requires user review)**

Run only after Track 1 fixes (if any blocking) have landed. Otherwise visual
findings may be invalidated by ongoing code-level changes.

- 2.0 Playwright MCP full re-pass (4h)
- 2.1 Figma fidelity (6-8h, no time-box per user choice)
- 2.2 + 2.2b Performance + Loading patterns (4h)
- 2.3 V Live indicator logic (30 min)
- 2.4 Responsive 14×3 = 42 (7h)
- 2.5 A11y visual + auto (2h)
- 2.6 AK CSS theme (1h)

**Wave 7 — Phase 3 consolidation (~8h)**

- 3.1 Aggregate findings → `consolidated-bugs.md`
- 3.2 Spawn backlog tasks per finding cluster (mix of bug-fix / refactor / docs)
- 3.3 Cluster small fixes into batches (per 0251 model)
- 3.4 Write `audit-summary.md` for team
- 3.5 Update `lore/3-wiki/` if patterns emerge

### Worklog convention

Per `lore-framework`, write session logs to `worklog/YYYY-MM-DD-session.md`
during each wave. Capture: which sub-phases worked on, findings count,
blockers hit, decisions made, time spent. Future sessions resume from worklog.

## Issues Encountered

_(to be filled during audit)_

## Design Decisions

### From Plan

1. **Two-track sequencing.** Track 1 (code, Claude solo) before Track 2 (visual,
   user review). Structural / contract issues fixed in Track 1 might invalidate
   Track 2 visual findings.
2. **Senior fresh-eye stance — no source is revealed truth.** Per karolkow
   2026-05-22; mirrors `feedback_sources_are_interpretations`.
3. **Read-only audit + spawned fixes.** Findings don't get fixed mid-pass.
4. **MUST first within each track.** High-impact issues surface early.
5. **Exhaustive state matrix 14×9 = 126 cells.** User chose full matrix over
   sampled 4×5 = 20 on 2026-05-24 — better coverage, more effort accepted.
6. **Exhaustive responsive 14×3 = 42 cells.** User chose full matrix over
   sampled 5×3 = 15 on 2026-05-24.
7. **Figma fidelity BEZ time-box.** User chose pixel-perfect over 30 min/route
   time-box on 2026-05-24 — more thorough, slower.
8. **Split merged phases back to granular per user 2026-05-24:**
   - U+X+Y → 1.9 + 1.9b + 1.9c (component reuse / coupling / simplicity)
   - Z+AA+AB+AD → 1.10 + 1.10b + 1.10c + 1.10d (senior craft / overengineering / hallucination / maintenance cost)
   - P+AQ → 1.11 + 1.11b (code quality / type safety depth)
   - AG+AP → 2.2 + 2.2b (performance / loading patterns)
     Granular = more deliverables, more focused findings per file.
9. **Kept merged per user 2026-05-24:**
   - Q+AR → 1.18 (lore process + commit conventions) — same code area
   - F+CH → 2.5 (a11y + color blindness) — same audit pass
10. **EXTRA 1.12 useTableUrlState justification analysis** per user 2026-05-24
    — senior fresh-eye question whether the abstraction is needed at all
    given TanStack already in stack.

### Emerged

_(to be filled during audit)_

## Future Work

_(populated during Phase 3 — spawned tasks list, including Out-of-scope
follow-ups from table above)_

## Notes

- **Scope built interactively** via 9-batch Q&A walkthrough on 2026-05-24.
  Every sub-phase explicitly confirmed (Tak / Nie / modyfikacja). See
  Design Decisions section for all explicit choices captured.
- **Prior QA pass:** birthed 0251 (13-bug fix batch); methodology proven.
  Merged on develop 2026-05-24.
- **Backfill state at task creation:** ~61% done at 0251 planning time,
  re-snapshot at audit start in `findings/audit-summary.md`.
- **Per `feedback_new_tasks_on_develop`:** this task file lives on develop
  branch as a new backlog item.
- **Per CLAUDE.md "no commit without explicit signal":** task file written
  but uncommitted; karolkow holds commit signal.
