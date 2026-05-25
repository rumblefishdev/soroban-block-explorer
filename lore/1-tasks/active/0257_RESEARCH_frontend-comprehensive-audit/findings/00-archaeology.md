# 1.0 Archaeology — FE lore sweep

Date: 2026-05-25
Scope: every FE-related task (`tags: frontend` or filename containing
`frontend|ui-`) across `archive/`, `active/`, `backlog/`, `blocked/`.
Method: read-only grep over task bodies + code TODOs + cross-ref to spawned
backlog tasks.

## Summary

| Bucket | Count |
|---|---|
| FE-tagged tasks scanned | 31 (22 archive, 2 active, 4 backlog, 1 blocked, plus 0257 itself + 0246/0249 referenced) |
| Future Work items found | 28 |
| Future Work → spawned backlog task | **3 of 28** (0226 from 0061, 0229 from 0076, 0238 from 0230) |
| Future Work → NOT spawned (gap) | **25** |
| Issues Encountered worth re-audit | 7 (recurring worktree node_modules trap, husky pre-commit drift, prettier collapse, etc.) |
| Emerged Decisions (autonomous) | 41 — list at end, several need re-validation |
| Unchecked Acceptance Criteria | 4 ('deviation noted' style) + 1 hard defer (Playwright CLI regression on 0077) + 1 hard defer (Manual QA on 0238) |
| Code TODO / FIXME / XXX / HACK | **0** literal markers under `web/src` / `libs/ui/src` / `libs/api-types/src` (excl. generated) |
| Tasks with `defer` / `follow-up` / `nit` in code | 1 (`usePoolTransactions.ts:16` references 0247 / 0249 pending) + 1 (`TimeSeriesChart.tsx:109` doc comment) |
| Blocked FE-related tasks never unblocked | **2** (0199 LP analytics, 0215 LP-FE-impact doc) |
| Task status / body drift | **1 hard** (0066 frontmatter `status: active`, body says `Status: Backlog` / `Not started` — actually implemented per history 2026-05-11) |
| Task status drift | **1 soft** (0063 frontmatter `status: active`, body `## Status: Backlog`, but all but one AC checked) |
| Stub page in production routes | **1** (`web/src/pages/TransactionDetailPage.tsx`, 14 lines, 334 bytes — page reachable at `/transactions/:hash`) |

## Critical pre-launch flags

| # | Severity | Finding |
|---|---|---|
| A1 | 🔴 CRITICAL | `web/src/pages/TransactionDetailPage.tsx` is a 14-line `PageStub` mounted in router `index.tsx:31-32` for `/transactions/:hash`. Every transaction-hash link from the home page / `/transactions` / `/ledgers/:seq` / `/accounts/:id` / `/contracts/:id` / `/liquidity-pools/:id` lands here. 0070 + 0071 (the real work) sit in backlog, no `priority-launch-blocker` tag. |
| A2 | 🟠 HIGH | 0066 (TanStack scaffold) `status: active` but body says `Not started` — history entries show it was implemented 2026-05-11 but the task body was never updated. Pure lore hygiene drift; no code impact, but indicates other FE tasks may also carry stale bodies. Re-audit body of every `status: active` FE task. |
| A3 | 🟠 HIGH | 25 of 28 Future Work items from archived FE tasks have NO spawned backlog task. List below — every one is a deliberate post-merge deferral with no tracking. |
| A4 | 🟡 MEDIUM | 0226 (libs/ui vitest infra) backlog since 2026-05-15, blocks 4 explicit references (0073/0074/0077/0238 deferred their Playwright CLI runs and unit tests on it). 10 days later, no movement. |
| A5 | 🟡 MEDIUM | 0199 (LP analytics) blocked-on-oracle; 0215 (LP-blocked endpoint FE impact catalog) blocked. 0077 explicitly deferred chart wiring to 0199 — until 0199 unblocks, every LP detail page renders "Chart data not yet available — pending oracle (task 0199)" placeholder. No oracle-decision ADR exists. |

## Future Work inventory — cross-ref to spawned backlog task

Table format: source task → deferred item → spawned-task or **GAP**.

| Source | Deferred item | Spawned task? |
|---|---|---|
| 0059 | Live stats wiring (TPS/ledger-seq/accounts/contracts) in TopNav (`MOCK_STATS`) | tied to 0066 (now done per history, but TopNav still shows MOCK_STATS — re-check) |
| 0059 | Responsive nav (collapsible / hamburger on mobile) | **GAP** — not in backlog |
| 0061 | Unit test infra for `libs/ui` | ✅ 0226 (backlog) |
| 0062 | Migrate validators from `libs/ui/src/identifiers/validators.ts` → `libs/domain` | **GAP** — `libs/domain` doesn't exist; no spawn |
| 0062 | Audit `IdentifierDisplay` to render router `<Link>` not `<a>` once router lands (0066) | **GAP** — 0066 has landed; no audit task |
| 0067 | Route-param validation per page (deferred to per-page tasks) | partially absorbed by 0068+ tasks; 0251 fixed pool/asset; tx detail still stub |
| 0068 | Functional table sorting once API exposes sort param | **GAP** (contingent on backend) |
| 0068 | Populated-data visual diff against real backend | absorbed into 0251 / 0257 |
| 0069 | `libs/ui` error/empty divergence from Figma DS — fix at source | **GAP** — 0251 fixed B*-series, but original divergence audit never spawned |
| 0069 | Operation pill colour confirm with design | **GAP** |
| 0069 | OpenAPI operation_type enum in backend (FE filter list hardcoded today) | **GAP** — backend task not spawned |
| 0072 | Hoist `Button` + `formatFee` + 2-line timestamp from `web/pages/transactions/` → `libs/ui` (kills `pages/ledgers` → `pages/transactions` coupling) | **GAP** |
| 0072 | URL-synced cursor pagination across list pages (was unused) | ✅ 0238 (archived) |
| 0075 | Contracts list page + `Contracts` entry in `NAV_LINKS` (contract detail unreachable by browsing today) | **GAP** — confirmed: nav has no `/contracts` route, only deep link works |
| 0075 | Backend: events count for honest tab pill | **GAP** |
| 0075 | Document `wasm_interface_metadata` JSONB shape (so FE doesn't reverse-engineer) | **GAP** |
| 0075 | Synthesized SAC interface stub (SEP-41) | **GAP** |
| 0076 | NFT trait rarity ("X% have this") line | ✅ 0229 (backlog) |
| 0077 | Tx Amount column on PoolTransactions | **GAP** — gated on 0247 RESEARCH |
| 0077 | Chart series wiring | **GAP** — gated on 0199 (blocked) |
| 0077 | Per-leg `icon_url` in `PoolAssetLeg` (backend extension) | **GAP** |
| 0077 | Playwright CLI regression for LP pages | **GAP** — gated on 0226 |
| 0077 | LP "out-of-scope senior-eye" list: row distinction for stale pools, list sort options, tx-type filter, pool-id paste search, a11y review, mobile QA (6 items) | **GAP** all 6 |
| 0230 | (pointer to 0238) | ✅ 0238 (archived) |
| 0238 | Backend `prev_cursor` in `PageInfo` | **GAP** |
| 0238 | Unit tests for `useCursorPagination` | gated on 0226 |
| 0238 | Playwright CLI smoke for 11 paginated pages | gated on 0226 |
| 0238 | ADR for URL-cursor pagination convention (multi-cursor namespacing `cursor_p/_t/_e/_i`) | **GAP** — convention undocumented |
| 0251 | ScVal decoder for Contract Events | **GAP** |
| 0251 | Network runtime toggle (full multi-network) | **GAP** |
| 0251 | Transaction detail real implementation | ✅ 0070 / 0071 (backlog, stale) |
| 0251 | Searchable Autocomplete for ops dropdown (27-entry MUI Select usability) | **GAP** |
| 0251 | B4 fake-XLM disambiguation — design redo (tooltip / `(verified)` ribbon / sub-line) | **GAP** |

**Spawned-or-tracked: 5 / 28. Gap: 23 / 28 (82%).**

## Issues Encountered — worth re-audit

Recurring failure modes that hit nearly every FE task and have not been
documented in `lore/3-wiki/` as gotchas:

1. **Worktree without `node_modules`** — explicitly cited in 0061, 0069, 0072,
   0075 (`L8`/`L9`), 0077, 0230, 0238. Root cause = workspace symlink
   resolution walks up-tree. Confirmed fix = `npm install` from inside
   worktree. → wiki gap.
2. **Husky pre-commit drift on staged + deps typecheck** — 0077, 0238.
   `--no-verify` used; not a code defect but pattern of bypassing the gate.
3. **Stale vite dep cache** — 0061. `web/node_modules/.vite` clear required.
4. **Stale `libs/ui/dist/index.d.ts`** — 0069. Clean rebuild required.
5. **TypeScript incremental cache poisoning (`*.tsbuildinfo`)** — 0238.
   `find . -name '*.tsbuildinfo' -delete && rm -rf libs/ui/dist web/dist && npx nx reset`.
6. **Pre-commit typecheck from wrong directory** (nx resolved main project's
   `libs/ui` stub, not worktree's) — 0059.
7. **Prettier collapsed inline-code spans across line wraps** — 0077. No
   config change; rephrase as plain prose.

→ Recommendation: spawn `XXXX_DOCS_frontend-worktree-gotchas-wiki.md` to
land these in `lore/3-wiki/`.

## Code-level TODOs / FIXMEs / nits

`grep -rnE "TODO|FIXME|XXX|HACK\b" web/src libs/ui/src libs/api-types/src`
excluding `generated/` and `dist/`: **0 hits**. Indicates either disciplined
non-marker policy OR drift accepted silently into task bodies. Given the
volume of "deferred / pending / placeholder" decisions documented in task
bodies, the latter is more likely — recommend a convention check
(`grep -rE "placeholder|pending|stub"` returns many hits, see Y-simplicity
audit).

Useful matches found (non-`TODO` keywords):

| File:line | Context |
|---|---|
| `web/src/api/hooks/usePoolTransactions.ts:16` | `* implementation pending 0247 RESEARCH conclusion + 0249 follow-up.` |
| `libs/ui/src/visualization/TimeSeriesChart.tsx:109` | `* data fetching. Wrap in LazySection to defer rendering until visible.` (doc comment, not a TODO) |

## Unchecked Acceptance Criteria across FE archive

(Excluding "checked" + "deviation noted" cases; only items NOT delivered.)

| Source task | Item | Status |
|---|---|---|
| 0062 | Tooltip with full value on hover — `[ ]` with "deviated, see Emerged #4" | accepted deviation per Figma — OK |
| 0067 | Route params validated / Invalid → entity-typed not-found — `[ ]` deferred to per-page | partially absorbed (LP, asset done in 0251); tx-detail stub blocks closing |
| 0077 | Playwright CLI regression for both LP pages — `[ ]` deferred | gap, gated on 0226 |
| 0238 | Manual QA on all 11+ pages — `[ ]` deferred (Playwright dev server) | gap, gated on 0226 |

## Emerged Decisions (autonomous) inventory

41 emerged design decisions across FE archive. Many sensible (Figma-first
overrides, Stellar contract correctness), some worth Q&A re-validation:

### Worth re-audit (potential hallucination / scope creep)

| Source | Decision | Audit question |
|---|---|---|
| 0061 #4 | Sort caret without DS "Active" yellow pill — "deliberate middle ground" between two Figma variants | did designer confirm the middle-ground? |
| 0062 #4 | Tooltip removed from `IdentifierDisplay` per Figma exactly | hash truncation in tight cells now requires click-through. UX cost confirmed acceptable? |
| 0065 #4 | `OperationFlowTree` unified instead of separate `InvocationCallTree` | does it actually render Soroban call trees, or only operation flow? |
| 0065 #5 | Interval labels `1D/7D/30D/1Y` from Figma vs spec `1h/1d/1w` | did design intent change or was spec stale? |
| 0068 #5 | AppShell full-bleed only for `/` | layout one-off — accepted? |
| 0068 #6 | Page-level hero backdrop reproduced as inline component | reusable for future hero pages? |
| 0073 #5 | Balances show only "Native asset" / "Classic" (cannot distinguish SAC from API) | backend gap — spawn task? **GAP** |
| 0075 #6 | `interface_metadata` hand-typed from indexer source not OpenAPI | drift risk; backend task not spawned |
| 0075 #10 | Breadcrumb says "Contract" (Figma had stale "Account" leftover) | catch shows Figma is not 1:1 truth |
| 0077 #9 | Pool-id strkey encoder = ~60 LOC custom (avoid 50-100 KB stellar-base) | bundle size win documented; tested vs stellar-base. OK. |
| 0077 #12 | `assetLegLabel` hard-fails on schema drift via `throw` | error-boundary swallows in prod — does ops monitoring catch it? |
| 0077 #13 | `classifyLpTx` hard-fails on unknown op_type via `throw` | same as above |
| 0238 #5 | `cursorParam` multi-cursor namespacing (`cursor_p/_t/_e/_i`) via CURSOR_PARAMS registry | undocumented convention, ADR gap (already flagged) |
| 0251 B1 | `linked={false}` on pool-id header (instead of fixing href) | structurally avoids the bug; junior contributor will reintroduce link |

### Accepted (audit-aligned)

Remaining 27 are Figma-vs-spec alignments, error-boundary patterns, hook
extractions — all defensible, no audit flag.

## Blocked FE-related tasks

| Task | Status | Blocker | Date |
|---|---|---|---|
| 0199 | blocked | LP analytics oracle decision (no ADR) | unknown |
| 0215 | blocked | depends on 0199 (FE impact doc for blocked LP endpoints) | unknown |

→ Until 0199 has ADR or kill-decision, every LP detail chart shows
placeholder; 0215 (the doc that catalogs what FE shows in the
meantime) is itself blocked. Circular.

## Recommendations (for Phase 3 spawning)

Priority spawning list, by severity:

1. **🔴 LAUNCH BLOCKER:** Promote 0070 + 0071 from backlog → active, add
   `tag: priority-launch-blocker` (TransactionDetailPage stub is reached
   from every list).
2. **🔴 LAUNCH BLOCKER:** Spawn `XXXX_FEATURE_frontend-contracts-list-page`
   — `/contracts` route + nav link missing per 0075 future work.
3. **🟠 HIGH:** Update 0066 task body to reflect actual implementation
   state; audit every `status: active` FE task body for similar drift.
4. **🟠 HIGH:** Spawn `XXXX_RESEARCH_lp-oracle-decision-adr` — unblock 0199
   / 0215; defines whether LP analytics ships pre-launch or post-launch
   with explicit doc.
5. **🟠 HIGH:** Promote 0226 (test infra) — unblocks 4 deferred items.
6. **🟡 MEDIUM:** Batch-spawn the 23 GAP items above as backlog tasks with
   `related_tasks: ['0257']`, severity-tagged. Cluster small ones per the
   0251 model (`XXXX_DOCS_frontend-tx-detail-followups-batch`,
   `XXXX_FEATURE_frontend-libs-ui-hoist-batch`, etc).
7. **🟡 MEDIUM:** Spawn `XXXX_DOCS_frontend-worktree-gotchas-wiki` capturing
   the 7 recurring Issues Encountered failure modes into `lore/3-wiki/`.
8. **🟡 MEDIUM:** Spawn `XXXX_DOCS_adr-url-cursor-pagination-convention` for
   the multi-cursor namespacing (0238 deferred).
9. **🟢 LOW:** Spawn `XXXX_REFACTOR_frontend-error-throw-monitoring-audit`
   — `assetLegLabel` / `classifyLpTx` throw on schema drift; verify
   `SectionErrorBoundary` reports surface to ops.

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**A1 (🔴 CRITICAL — TxDetail stub):** **RESOLVED** in commit `a2c1b205`
(`feat(lore-0070): add transaction detail page`, merged via PR #215).
`web/src/pages/TransactionDetailPage.tsx` is now a 9-line re-export shim;
real page lives at `web/src/pages/transaction-detail/index.tsx` (145 LOC)
with normal/advanced mode toggle, 20+ supporting files (1799 LOC total),
TanStack hook `useTransactionDetail` → generated `getTransactionOptions`,
hash validation via `isTransactionHash`, `NotFoundState` for invalid hashes,
`GenericErrorState` with `classifyError`, `SectionErrorBoundary` wrapping
every section. Cascade dissolved: 1.5 E3 row now measurable; 1.7
outbound-from-E3 row now measurable (operations table → account / contract
identifiers via `IdentifierWithCopy` + `IdentifierDisplay`).

**A2 (🟠 HIGH — 0066 task body drift):** STILL STANDS. Filip's PR didn't
touch lore.

**A3 (🟠 HIGH — 25/28 Future Work gap):** STILL STANDS. 0070 + 0071 fall
out of the spawned-or-tracked set (now ✓), but the 23 other gaps remain.

**A4 (🟡 MEDIUM — 0226 test infra blocked):** STILL STANDS.

**A5 (🟡 MEDIUM — 0199/0215 LP blocked):** STILL STANDS.

**Recommendations 1 & 2 (LAUNCH BLOCKERS):**
- Item 1 (TxDetail) — **RESOLVED** by Filip.
- Item 2 (Contracts list page + `/contracts` nav) — STILL STANDS.
