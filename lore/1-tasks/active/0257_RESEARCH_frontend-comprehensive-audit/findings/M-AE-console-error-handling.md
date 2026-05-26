# M + AE — Console + error handling (Wave 4 1.6)

**Stance:** read-only, evidence-driven. Findings = post-Gate A baseline (F-E-2 lowercase `?op=` dropped; F-E-1 cursor URL resolved by 0254 merge).

## Per-check verdicts

| Check | Result | Notes |
|---|---|---|
| Zero ERROR per route happy path | ✓ | All 14 routes traversed in this session show 0 console errors except favicon 404 (universal, harmless). |
| Zero WARN per route | ✓ | No MUI warnings, no React warnings observed across home, list, detail, search routes. |
| React duplicate-key warnings | ✓ | Per Wave 1 B5 history: previously fixed. Re-verified clean across all list rendering. |
| Deprecated lifecycle warnings | ✓ | None observed. App is hooks-only. |
| Strict-mode double-render side effects | ✓ | TanStack queries idempotent; `useEffect` only in `useCursorPagination.ts` (reset-on-resetKey-flip) — guarded by `useRef` against initial-mount fire. |
| Source map present in dev | ✓ | Vite dev server serves sourcemaps automatically. Error stack traces resolve to .tsx lines (verified via thrown-error simulation in dev tools). |
| Network 4xx/5xx counts per route happy path | ⚠ | See F-AE-1 below — `/favicon.ico` 404 on every route. Otherwise clean. |
| Every try/catch has logger or user-feedback | ✓ | 5 try blocks total in `web/src/`: see F-AE-2 inventory below. All either rethrow or have user-visible fallback. None swallow silently. |
| No silent exception swallow | ✓ | Reviewed each try/catch — all either rethrow or fall through to TanStack's `isError` state. |
| Hard-fail decisions documented | ✓ | `assetLegLabel` (web/src/pages/pool-detail/helpers.ts:16-23), `classifyLpTx` (web/src/pages/pool-detail/PoolTransactions.tsx:44-67), `poolIdHexToStrkey` (web/src/utils/poolIdStrkey.ts:75-90), `config.ts` env throws — all have JSDoc/comment rationale. |
| Async hooks (TanStack) — error state properly propagated | ⚠ | 10/13 pages call `isError` correctly. 3 pages have composite-error UX (E6/E8/E9 — see F-D-2 in matrix doc). |
| No `console.log` leftover | ✓ | Grep `console\.(log|warn|error|info|debug)` in `web/src/` and `libs/ui/src/` (excluding tests) returns 0 hits. |

## Findings

### F-AE-1 [Class D, Severity 🟢] — `/favicon.ico` 404 on every route

- **Evidence:** Every navigated route logs `Failed to load resource: 404 — http://localhost:4200/favicon.ico:0`.
- **Cause:** `web/public/favicon.ico` is absent (verified `ls web/public/` returns no favicon).
- **Impact:** Cosmetic; one console error line per cold load.
- **Class:** D (catalog-only — pre-launch nit).
- **Recommendation:** Phase 3 — add favicon asset or `<link rel="icon" href="data:,">` shim in `web/index.html`.

### F-AE-2 [Class D, Severity 🟢] — try/catch inventory (informational)

5 try blocks in `web/src/`:

| File:line | Purpose | Behavior on catch |
|---|---|---|
| `web/src/api/config.ts:9` | URL validation of `VITE_API_BASE_URL` | Rethrows with informative message — runtime crash if env missing (correct fail-fast) |
| `web/src/pages/url.ts:10` | URL parse for cross-entity link | Falls back to raw string return — graceful |
| `web/src/pages/contracts/ContractEvents.tsx:57` | JSON parse on event payload | Falls back to raw string display — graceful |
| `web/src/pages/contracts/ContractEvents.tsx:99` | Hex decode for event payload | Falls back to raw hex — graceful |
| `web/src/pages/nft-detail/NftMediaPreview.tsx:28` | URL validation for media | Falls back to icon placeholder — graceful |

All justified. Class D informational; no action.

### F-AE-3 [Class A, Severity 🟡] — `SectionErrorBoundary` inconsistent coverage

- **Evidence:** Grep `SectionErrorBoundary` in `web/src/pages/`:
  - Used: AccountDetailPage, ContractDetailPage (verified) — wraps each section
  - **NOT used:** LedgerDetailPage, AssetDetailPage, LiquidityPoolDetailPage, NftDetailPage, TransactionDetailPage (transaction-detail/index.tsx)
- **Class:** A — affects error-state taxonomy measurement
- **Impact:** A throw in (e.g.) `LedgerTransactions.tsx` would bubble to root error boundary instead of section-scoping. Currently TanStack `isError` handles 90% of cases, but synchronous render-time throws (e.g. `assetLegLabel` on malformed leg, `poolIdHexToStrkey` on malformed pool id) would crash the whole page instead of the affected section.
- **Recommendation:** Phase 3 — uniform `SectionErrorBoundary` wrapping across all detail pages (per task README 1.9 detail-page pattern checklist).

### F-AE-4 [Class A, Severity 🟡, RECORDED from Gate A] — `client.ts:11-29` error interceptor flattens typed envelope

- Per Gate A F-AF-1: API error envelope (`{ code, message, details }`) is flattened to a vanilla `Error` instance by the FE API interceptor.
- **Impact for 1.6:** Error-state taxonomy measured at TanStack consumer (`isError`) level, not at typed-discriminator level. UI can show "Something went wrong" but cannot programmatically distinguish 404 vs 5xx vs 400 vs network-error for differentiated UX.
- **Class:** A (baseline accepted per Gate A) — re-documented here per Wave 4 protocol.
- **No fix at Gate B.** Phase 3 dedicated refactor task.

### F-AE-5 [Class B, Severity 🟠] — Composite NotFound + error rendering on E6/E8/E9

- Cross-reference to F-D-2 in `D-state-coverage-matrix.md` for full details.
- Sub-section queries (recent transactions / events / balances) fire alongside the parent detail query. When parent 404s, sub-section queries also 404, producing dual error blocks visible to user.
- **Recommendation:** Sub-section components should `enabled: !!parentData` or detail page should early-return NotFound state before mounting children.
- **Class:** B — defer to Gate B.

### F-AE-6 [Class A, Severity 🟠 → RESOLVED 2026-05-25] — Pagination disabled state masked real progress

- Cross-reference F-D-1 in matrix.
- ~~Live API returns old pagination shape~~ — RESOLVED post API-binary restart. New shape `page: { next_cursor, prev_cursor, limit }` served correctly; FE Next/Prev cycle URL `?cursor=…` works end-to-end.
- **Root-cause class observation stands:** there is **no console error / no user feedback** when wire-shape drifts. A future similar drift would again present as silent broken UX. The Phase 3 runtime-shape-probe + dev-env-rebuild-runbook recommendations remain valid as preventive controls.
- Console-error-handling angle: no error surface even when key payload fields are missing is a **F-AE-* gap that survives F-D-1's resolution**. Consider keeping this finding open as a Class A 🟡 "silent shape mismatch has no console signal" rather than closing entirely.

### F-AE-7 [Class D, Severity 🟢] — No global error reporter

- No Sentry / DataDog / equivalent error-reporting integration in `web/src/main.tsx` or `web/src/app.tsx`.
- Per Out of scope (BO session replay) and pre-launch nature, not blocking.
- Phase 3: spawn `XXXX_FEATURE_frontend-error-reporting` consideration task.

## Baseline notes (per Gate A directive)

- **F-E-2 (lowercase `?op=` → API 400 baseline):** verified during 1.5 traversal — `?op=invoke_host_function` returns 200 OK + 11 rows + 0 console warnings. **Not logged as finding.** Baseline = REST contract expected; FE owns canonicalisation for URLs it produces; malformed external input → API 400 is normal.
- **F-E-1 (cursor not in URL) resolved by 0254 merge.** Replaced by **new** F-D-1 (live API serves pre-0254 shape → pagination disabled) — different root cause, different severity.

## Net 1.6 finding count

7 findings: 0 🔴, 2 🟠, 2 🟡, 3 🟢.

## Top concern

**F-AE-5 + F-D-2 (composite NotFound)** — Gate B fix-first candidate. Sub-section query gating is a small refactor with high UX dividend.

## Gate B merge resolution 2026-05-26 — develop @ cdb0c81d (PR #219)

### F-AE-5 — **RESOLVED** in `473de2a2` + `9e88114b`

Paired with F-D-2 (composite NotFound). Sub-section queries no longer fire on parent 404 → console no longer emits duplicate API 404 traces from sub-section TanStack hooks during valid-format-404 navigation. Fix same as F-D-2 (render-gates added on account / contract / LP detail pages).
