# X — Coupling / decoupling (Wave 4 1.9b)

## Per-check table

| Check | Result | Evidence |
|---|---|---|
| `libs/ui` has no dependency on `web/` | ✓ | `grep -rn "from ['\"]web/\|@rumblefish/soroban-block-explorer-web" libs/ui/src/` returns 0 hits |
| `libs/api-types` has no dependency on `libs/ui` or `web/` | ✓ | Same grep returns 0; package.json verified — only `@hey-api/openapi-ts` runtime deps |
| Each page-level component extractable independently | ✓ | Each `*Page.tsx` is a leaf — composes child sections that take only `poolId`/`accountId`/etc. as props (verified pool-detail: `PoolKpiStrip({pool})`, `PoolParticipants({poolId})`, etc.) |
| Prop drilling >3 levels | ✓ | Sampled pool-detail: max 2 levels (Page → SectionCard → leaf). Asset-detail / contract-detail similar. No deep drilling. |
| Global state usage justified | ✓ | Only 1 `createContext` in app code: `libs/ui/src/theme/ThemeProvider.tsx:26` — ColorModeContext (light/dark mode). No app-level global state. TanStack QueryClient is the only "global" store, which is correct. |
| Each custom hook single responsibility | ✓ | Sampled: `useCursorPagination` (URL ↔ cursor), `usePageHandlers` (response → button state), `useDetailMode` (URL ↔ mode tab), `useTableUrlState` (URL serialization). Each has one job. |
| No cycles between modules | ✓ | `libs/ui` ←→ `libs/api-types` independent. `web/` depends on both. No back-edges. (Nx graph generation timed out locally; relied on grep direction analysis.) |
| API client single entry point | ✓ | `web/src/api/` is the single layer. All hooks (`useLedgersList`, `useAssetsList`, etc.) import from generated client. No raw `fetch` / `axios` in app (verified). |

## Findings

### F-X-1 [Class C, Severity 🟡] — `assetLegLabel` cross-folder reach from `liquidity-pools/` into `pool-detail/`

- **Evidence:**
  - `web/src/pages/liquidity-pools/PoolsTable.tsx:16` and `AssetAvatar.tsx:4`: `import { assetLegLabel } from '../pool-detail/helpers.js';`
  - Comment at line 12-14 documents the choice: helper "lives in detail-page helpers but the labelling rule belongs to both list + detail".
- **Impact:** Tight coupling between two sibling page folders. If `liquidity-pools/` folder is extracted independently, it breaks.
- **Recommendation:** Move `assetLegLabel` + `classifyLpTx` to `web/src/pages/liquidity-pools/shared/` or hoist to `web/src/pages/format.ts` (since they format an LP-specific value).
- **Class:** C — defer to Gate B with senior-craft batch.

### F-X-2 [Class C, Severity 🟢] — `web/src/pages/detail/` is a single-file folder

- **Evidence:** `web/src/pages/detail/` contains only `SectionCard.tsx`. Folder is a 1-file home for a shared chrome.
- **Per task README 1.3 file/folder structure:** folder name matches concept ("detail") but it's a thin home.
- **Recommendation:** Hoist SectionCard to `libs/ui` per F-U-1; folder can be deleted.
- **Class:** D (catalog-only) — Phase 3 with F-U-1 hoist.

### F-X-3 [Class A, Severity 🟡] — `usePageHandlers` extracted as shared chunk post-0254

- **Evidence:** Bundle delta in Gate A doc shows new `usePageHandlers-*.js` chunk (2.35 KB / 1.16 KB gz) after 0254 merge.
- **Interpretation:** This is GOOD. Hook hoisted to libs/ui (`libs/ui/src/table/usePageHandlers.ts`) gets its own Vite chunk and is shared across 13 paginated pages.
- **Class:** A — positive baseline note, not a problem. Confirms the right level of abstraction.

### F-X-4 [Class C, Severity 🟡] — Hooks colocated in two places

- **Evidence:**
  - `web/src/api/hooks/` — 7+ hooks (useLedgersList, useAssetsList, useContractEvents, useContractInvocations, usePoolTransactions, usePoolsList, usePoolParticipants — all are TanStack `useQuery` wrappers tied to API endpoints)
  - `web/src/pages/transaction-detail/useDetailMode.ts`, `useTxHashParam.ts` — page-local hooks
  - `libs/ui/src/table/useCursorPagination.ts`, `useTableUrlState.ts`, `usePageHandlers.ts` — shared infrastructure hooks
- **Verdict:** Three-tier hook organization is actually fine. API hooks colocated near API layer ✓. UI infrastructure in libs/ui ✓. Page-local utility hooks colocated near page ✓.
- **Class:** C — informational; document pattern in 3-wiki (per Phase 3 sub-phase 3.5).

### F-X-5 [Class D, Severity 🟢] — `web/src/utils/` has 1 file (`poolIdStrkey.ts`)

- **Evidence:** Only utility outside `web/src/pages/` is `poolIdStrkey.ts`. Other "utility-ish" code lives in `web/src/pages/format.ts` and `web/src/pages/transactions/formatters.ts` (note: 2 formatter files).
- **Inconsistency:** `poolIdStrkey` is a Stellar-domain util — could justifiably live in `libs/ui` (alongside `truncateMiddle`, identifiers). But hoisting requires depending on `@stellar/stellar-sdk`'s `StrKey` — bundle-size cost. Today it lives in `web/`.
- **Class:** D — Phase 3 decision: hoist + accept bundle cost, OR keep in web. Document.

## Summary

5 findings: 0 🔴, 0 🟠, 3 🟡, 2 🟢.

**Coupling baseline is excellent.** No cycles, no cross-package leaks, no global state abuse. The few concerns are folder hygiene (F-X-1, F-X-2) and pattern documentation (F-X-4).
