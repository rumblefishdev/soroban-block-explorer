# Gate A — Triage Decisions

**Date:** 2026-05-25
**Stage:** end of Wave 3 (Tier 1 + Tier 2 sub-phases complete)
**Findings inventoried:** ~89 actionable across 16 files (`00-archaeology` through `quick-wins-DM-DN-CA`)
**Cumulative severity:** 2 🔴 / 22 🟠 / 29 🟡 / 27 🟢

## Scope of this gate

Per task README "Triage gates" section:

- **Class A (baseline-breaker)** — decide per finding: fix-first | document baseline + accept | defer
- **Class B (routing/contract) that affects Wave 4 sub-phases (1.5 / 1.6 / 1.9 / 1.9b / 1.12 / 1.15)** — fix-first or accept measurement against current broken state
- **Class B that only affects Track 2** — defer to Gate B
- **Class C (visual/layout)** — defer to Gate B
- **Class D (catalog-only)** — defer to Phase 3 bulk-spawn
- **Class E (off-band: security / secret / license)** — immediate independent fix; does not block gate

## Wave 4 sub-phases at risk

Wave 4 runs:
1. **1.5 state coverage matrix** 14×9 = 126 cells (Playwright marathon)
2. **1.6 console + error handling** across all routes
3. **1.9 + 1.9b** component reuse + coupling (grep-heavy)
4. **1.12 state separation** + EXTRA `useTableUrlState` justification
5. **1.15 Stellar domain consistency** (strkey, XDR, Soroban-era, network passphrase)

A finding becomes Gate A fix-first only if it changes what these sub-phases measure.

---

## Class A — baseline-breakers (5 findings)

| ID | Finding | Cascade if NOT fixed | Decision | Rationale |
|---|---|---|---|---|
| **F-AQ-1** 🟠 | `noUncheckedIndexedAccess` off in `tsconfig.base.json` | Toggling mid-audit regenerates tsc errors → 1.11/1.11b baseline invalid | **DEFER (Phase 3 refactor task)** | Audit MEASURES current "flag off" baseline. Toggling = refactor scope, not Gate A. Spawn dedicated task post-audit: enable flag, fix new errors, separate PR. |
| **F-AQ-4** 🟠 | Zero branded ID types (`AccountId` / `AssetId` / `PoolId` / etc.) | None for Wave 4 (1.15 inspects display patterns, not type-level) | **DEFER (Gate B refactor task)** | Type-system refactor, not measurement question. Gate B can decide if it bundles with 1.10/10b senior craft findings. |
| **F-AI-1 + F-AI-2** 🟠 | Main bundle 594KB / LP chunk 313KB | None for Wave 4 (perf is 2.2 = Wave 6) | **DEFER (Gate B)** | Wave 4 doesn't measure bundle. Gate B re-snapshots for Wave 6 baseline. |
| **DM-1** 🟠 | `Footer.tsx:78-102` hardcoded "All systems operational" | Confirms 2.3 V live indicator finding (Wave 6) but doesn't break Wave 4 | **DOCUMENT BASELINE + DEFER (Gate B)** | Audit baseline = "footer indicator is hardcoded, no health probe". 2.3 V will cite this. Fix-first = single-PR refactor (introduce `/health` probe + indicator state), but doesn't unlock Wave 4 anything. |
| **F-AF-1** 🟡 | `client.ts:11-29` error interceptor flattens typed `ErrorEnvelope` | 1.6 console error handling sees flattened `Error` not typed `code` discriminant — partial measurement loss | **ACCEPT BASELINE + NOTE IN 1.6** | Document in Wave 4 1.6 output: "error-state taxonomy measured at consumer level, not interceptor level". Fix-first not justified; refactor in Phase 3 with typed `extractErrorCode()` helper. |

## Class B — routing/contract that affects Wave 4 (3 fix-first candidates)

| ID | Finding | Wave 4 impact | Decision |
|---|---|---|---|
| **F-E-1** 🔴 | Pagination cursor never written to URL on `/transactions` + `/ledgers` Next-click | **1.5 cells D2/D9 on every list page** become unreliable: "success state" matrix can't validate URL-restorable pagination; D9 "stale polling indicator" tied to cursor refresh path | **FIX-FIRST** ✅ Single highest-value Gate A item. Spawn `audit-blocker` task. Without fix, 1.5 records "URL state broken across all list pages" 14 times. |
| **F-K-1 + A1** 🟠 | `/transactions/:hash` renders 14-line stub (`PageStub`), no validation | **1.5 entire E3 row (9 cells) becomes N/A**; 1.6 E3 console blank; 1.7 every tx-link landing E3 = stub-by-design; 2.0/2.1/2.4 E3 = N/A on Track 2 | **RESOLVED 2026-05-25:** 0070+0071 in-flight by FilipDz (local-only, unpushed). Path B (document baseline + delta-audit post-merge). Skip E3 row across all matrix sub-phases with `N/A — gated on FilipDz merge` markers. See action item #3 below for full plan. |
| **F-E-2** 🟠 | Lowercase `?op=invoke_host_function` from URL → MUI warning + API 400 + 0 rows; `normalizeOperationType` doesn't normalize URL input | **1.6 console** registers API 400 on every Next-click during 1.5 marathon → 100+ false-positive console errors during Playwright pass | **FIX-FIRST** ✅ Single-file fix (`normalizeOperationType` at URL parse boundary). Without it, 1.6 baseline polluted by reproducible API 400 noise that masks real console findings. |

## Class B — defer to Gate B (Track 2 impact only)

Listed for the record; no Gate A action.

| ID | Finding | Why Gate B |
|---|---|---|
| F-K-2 + F-K-3 🟠 | Pool detail reserve labels + participants "Since ledger" no `<a href>` | 1.7 already done (Wave 3); affects 2.0 Track 2 only |
| F-L-1 + F-K-4 🟠 | Pool strkey (`L...`) search → 0 results across 6 tabs; strkey↔hex unified canonicalisation | 1.14 already done; affects 2.0 search Track 2 |
| F-E-3, F-E-8 🟡 | URL state edge cases | 2.0 Track 2 measures real-world UX impact |
| F-I-3 🟡 | Polling cache logic gap | 2.0 / 2.2 Track 2 |
| C-5 🟠 | Missing `isAssetId` / `isNftId` validator | Routing-level refactor; Gate B with type-safety refactors |

## Class C — defer to Gate B (visual/layout)

All deferred. Track 2 measures visual fidelity vs Figma post-fix. Gate B triages.

- **CA-1 + CA-2** 🟠 Footer Terms/Privacy/Cookies + Resources dead `<span>` (no `href`) — likely pre-launch must-fix at Gate B
- **J-3** Tx pages: `TopNav.formatNumber` duplicated locally
- **J-5** Timestamp depth inconsistency
- **J-7** Address/hash truncation re-implementations across pages
- **F-L-2** Search no-results layout

## Class D — defer to Phase 3 bulk-spawn (~22 findings)

Catalog-only. Phase 3 sub-phase 3.2 spawns each as `lore/1-tasks/backlog/XXXX_*.md` with `related_tasks: ['0257']`. Examples:

- **A2 + Q-4** 0066 task triple-drift (frontmatter / body / history) → lore-hygiene fix
- **A3** 25/28 Future Work items from FE archive without spawned task → bulk spawn
- **A5** 0199 (LP analytics) + 0215 (LP-blocked FE) still blocked, oracle ADR pending
- **DN-1** Build version / SHA not displayed in UI
- **F-CO-6** `@mui/utils` triple-version in lockfile
- **AR-7** Branch protection rules (needs GitHub-side human verify)
- **Q + AR series** Lore process compliance batch
- **All J-data-formatting sub-Yellow** Trailing-zero, em-dash vs hyphen, etc.
- **All H-security LOW** (1 finding only)
- **All I-polling sub-Yellow + LOW** (4)
- **All K/E/L sub-Yellow + LOW** (~6)
- **AO-build hygiene sub-Yellow + LOW** (2)

Exact list = every Wave 1-3 finding tagged `[Class D]`.

## Class E — off-band immediate (2 actions)

| ID | Finding | Action | Blocks Gate A? |
|---|---|---|---|
| **F-CO-1** 🟠 | Vite 7.3.1 has 3 high-severity dev-server CVEs (path traversal, fs-deny bypass, arbitrary file read via WebSocket); fix at 7.3.3 | **Spawn dedicated `XXXX_CHORE_vite-7.3.3-cve-bump.md` backlog task → promote → merge → audit branch rebase**. Single-line dependency bump. | **No.** Off-band. Wave 4 can start without waiting. |
| **C-17** 🟠 | No `tower_http::cors::CorsLayer` in `crates/api/src/` — prod browser CORS depends on infra (API GW / ALB) terminating | **Open ticket with infra owner: confirm CORS termination at infra layer OR add `CorsLayer` to API**. Audit cannot resolve unilaterally. | **No.** Out of FE audit scope. Document, route to backend/infra. |

---

## Summary table

| Class | Total | Fix-first @ A | Defer to B | Defer to Phase 3 | Off-band immediate |
|---|---:|---:|---:|---:|---:|
| A | 5 | 0 | 3 | 2 | 0 |
| B (Wave 4 impact) | 3 | **3** | 0 | 0 | 0 |
| B (Track 2 only) | 6 | 0 | 6 | 0 | 0 |
| C | 5 | 0 | 5 | 0 | 0 |
| D | ~22 | 0 | 0 | ~22 | 0 |
| E | 2 | 0 | 0 | 0 | 2 |
| **Total** | **~43** | **3** | **14** | **~24** | **2** |

(Remaining ~46 of ~89 actionable findings are LOW severity catalog/nit items — all defer to Phase 3 by default.)

## Fix-first action list (Gate A → Wave 4)

Three `audit-blocker`-tagged backlog tasks to spawn + land before Wave 4 starts:

1. **`XXXX_BUG_url-cursor-not-written.md`** — F-E-1
   - **Class:** B
   - **Severity:** 🔴 CRITICAL
   - **Scope:** `web/src/pages/.../useTableUrlState` (and consumers on `/transactions`, `/ledgers`, every list page) — confirm cursor write path on Next-click, fix to persist `?cursor=...` in URL.
   - **AC:** refresh on page N preserves position; deep link `/transactions?cursor=ABC` lands on cursor page; both list pages tested.
   - **Related:** 0257 (this audit)

2. **`XXXX_BUG_url-op-filter-case-normalise.md`** — F-E-2
   - **Class:** B
   - **Severity:** 🟠 HIGH
   - **Scope:** `web/src/pages/transactions/operationTypes.ts` — `normalizeOperationType` to canonicalise URL input before API call.
   - **AC:** `?op=invoke_host_function` (lowercase) round-trips to canonical case; no MUI warning; API returns 200 with rows.
   - **Related:** 0257, 0251 (H2 — same root cause area)

3. **NO audit-blocker spawn for E3** — F-K-1 + A1 — **RESOLVED 2026-05-25:**
   merged via develop (commit `a2c1b205`, PR #215). E3 row now measurable.
   Delta-audit scope queued (see worklog Phase 4 re-audit queue).
   Original context for the record (was Path B before merge):
   - **Context:** 0070 + 0071 (TxDetail normal + advanced) are in-flight by
     **FilipDz** on local branches not yet pushed to origin (verified: zero
     remote refs matching `0070|0071|tx-detail|transaction-detail`).
   - **Decision:** **Path B (document baseline) + delta-audit plan.**
     Spawning fix-first audit-blocker would duplicate Filip's WIP and risk
     conflict on merge. Auditing his WIP wastes effort (he'll rework
     pre-merge).
   - **Wave 4 + Track 2 baseline:** E3 row skipped across all matrix
     sub-phases. Pre-populate output files with `N/A — gated on 0070+0071
     (FilipDz in-flight, local-only)` markers in:
     - 1.5 `findings/D-state-coverage-matrix.csv` — 9 N/A cells for E3 row
     - 1.6 `findings/M-AE-console-error-handling.md` — E3 section "N/A pending merge"
     - 2.0 `findings/playwright-pass/E3-transactions-detail.md` — header note "deferred"
     - 2.1 `findings/B-figma-fidelity.md` — E3 section "N/A pending merge"
     - 2.4 `findings/R-responsive-matrix.csv` — 3 N/A cells for E3 row
   - **Effective matrix:** 1.5 = 13×9 = 117 measurable cells; 2.4 = 13×3 = 39 measurable cells.
   - **Delta-audit post-merge** (separate ~1.5h session, single Playwright run):
     - E3: 9 cells 1.5 + 1.6 console + 1.7 cross-entity links FROM E3 (ops table → assets/contracts/accounts) + 2.0 visual + 2.1 Figma + 2.4 responsive 3 cells.
     - Total delta scope ~1.5h vs ~6h if E3 audited mid-Wave 4 and re-audited post-Filip-rework.
   - **Action item for user:** ask Filip ETA on 0070+0071 push/merge. If <1 week, queue delta-audit. If longer, document permanently in audit-summary.md as "E3 row not covered, gated on Phase Y task completion".
   - **No backlog task spawned** for E3 — Filip already owns the work via existing 0070+0071 in backlog.

Plus 1 off-band:

4. **`XXXX_CHORE_vite-7.3.3-cve-bump.md`** — F-CO-1
   - **Class:** E
   - **Action:** `npm i vite@7.3.3 --save-dev`; verify build still green; commit.
   - **Does not block Wave 4.**

Plus 1 off-band routing (no spawn, just communication):

5. **CORS infra question to backend/infra owner** — C-17
   - Email / Slack: "Production CORS — does API GW / ALB terminate, or do we need `tower_http::cors::CorsLayer` added to API?"

## After fix-first lands

1. Each `audit-blocker` task merges to develop via standard PR flow.
2. Rebase audit branch onto develop: `git rebase origin/develop` from `research/0257_frontend-comprehensive-audit`.
3. Update this triage doc with "Resolved" rows (date + commit SHA).
4. Append worklog: "Gate A fix-first batch landed, audit branch rebased on develop tip <SHA>, baseline reset for Wave 4."
5. **Then** start Wave 4 (Tier 3 sequential — 1.5 state matrix marathon).

## Anti-pattern reminder (per task README)

Do NOT fix-first at Gate A:
- Lore drift (0066) — batch in 1.18 / Phase 3
- Style / comment nits — Phase 3 batch PR
- Doc-only updates — Phase 3
- Renamings / refactors without behavior change — Phase 3
- Future Work backlog spawning — literally Phase 3 sub-phase 3.2
- Type-safety flag toggles — invalidates current baseline, Phase 3 dedicated task
- Bundle size optimization — Wave 6 measures post-fix; defer to Gate B

## Post-merge update 2026-05-25 — develop @ 6b7fb558 (FilipDz tx-detail PR #215)

**Action item #3 (E3 baseline decision):** **RESOLVED 2026-05-25** —
merged via develop (commit `a2c1b205`). E3 row now measurable. Delta-audit
scope queued (see worklog Phase 4 re-audit queue).

**Net Gate A fix-first scope unchanged:** 2 audit-blocker tasks
(F-E-1 cursor URL write + F-E-2 lowercase op normalise) + 1 off-band
(Vite 7.3.3 CVE bump) + 1 communication (CORS infra question).

**Wave 4 plan adjustment:** E3 row now in-scope across 1.5 / 1.6 / 1.7 /
2.0 / 2.1 / 2.4 (was N/A — Path B markers no longer needed). Effective
matrix restored to 1.5 = 14×9 = 126 cells; 2.4 = 14×3 = 42 cells.

**Severity escalations from post-merge findings:**
- J-4 (STROOPS_PER_XLM gap) bumps 🟡 → 🟠 HIGH (duplicate realised in tx-detail)
- J-7 (truncation re-impls) bumps 🟡 → 🟠 HIGH (count went from 2 → 6)
- F-J-16 NEW 🟠 HIGH (duplicate `formatFee` function)
- F-K-7, F-K-8 NEW 🟡 MEDIUM (E3 outbound link verification — defer to delta)
- F-AI-10 NEW 🟡 MEDIUM (E3 chunk composition)

None of the new severity escalations promote to fix-first at Gate A;
all are Class B/C deferrable to Gate B.

## Post-merge update 2026-05-25 (0254 merge @ 6af74d82) — develop @ 68b40058

**Karol's 0254 branch (direction-aware cursor pagination + prev_cursor
+ ADR 0008 amendment + test suite) merged into audit branch.**

### Fix-first scope after merge: **2 items → 0 items** (1 resolved + 1 dropped)

| Item | Pre-merge state | Post-merge verdict |
|---|---|---|
| F-E-1 (URL cursor not written) | 🔴 fix-first audit-blocker | **RESOLVED** via `f646047d` (FE prev-stack drop + wire backend `prev_cursor`) + `78345d49` (backend direction-aware cursor). See `findings/E-url-state-functional.md` Post-merge update section. |
| F-E-2 (lowercase op normalise) | 🟠 fix-first audit-blocker | **DROPPED 2026-05-25** — per user senior design call: "URL to URL i po prostu powinien być poprawny i tyle." Re-classified as ACCEPT BASELINE (URL = wire contract, FE owns canonicalisation only for URLs it produces; malformed external input → API 400 = expected REST behavior). Task file `0262_BUG_url-op-filter-case-normalise.md` moved to `.trash/`. See `findings/E-url-state-functional.md` F-E-2 section for full rationale + Wave 4 implications. |
| Vite 7.3.3 CVE bump | Class E off-band | STILL STANDS (`@vitejs/plugin-react@7.3.1` unchanged in merge) |
| CORS infra comm | Class E off-band | STILL STANDS |

### Cascade impact

- **0261 audit-blocker task `0261_BUG_url-cursor-not-written.md`** now
  obsolete. Do not delete (user owns the call) — flag for rename or
  removal.
- **Filename ID collision:** Staś's `0261_BUG_parser-missing-pool-id-on-path-payment-ops.md`
  was spawned same day with same `0261_` prefix. Two files share an ID.
  Flag for user decision (renumber one, or accept the collision as
  history given F-E-1 task is obsolete anyway).
- **F-E-8** (per-section cursor keys) — RESOLVED via same fix.
- Wave 4 1.5 cells D2 / D9 for every list page + multi-section detail
  can measure intended URL contract on first pass.

### New audit-surface from 0254

| Concern | Status |
|---|---|
| `cursor` → `next_cursor` rename (BREAKING wire format) | FE consumers all updated; `tsc` green (0 errors); no stale `.cursor` references in `web/src` or `libs/ui` consumer code (the 2 hits in `useCursorPagination.ts` are `state.cursor` for URL-local cursor, intentional). |
| `prev_cursor` field added | Wired via `usePageHandlers.ts:48` (`page?.prev_cursor ?? null`). |
| `has_more` dropped | No consumer accesses `.has_more` anywhere in `web/src/` (grep clean). |
| ADR 0008 amendment | Read; canonical URL state expectation = `?cursor=<token>` (single key per section), opaque base64-JSON. No FE-visible discontinuity. |
| Backend changes (handlers / queries / cursor.rs / pagination.rs) | Out of FE audit scope; ripple effects covered by FE-side wire shape verification above. |
| Integration test suite `crates/api/src/tests_integration.rs` (+544 LOC) | Out of FE audit scope. Mentioned in 0254 README "defer test suite to 0257" — 0257 does NOT inherit backend test work, it inherits the verified-via-tests FE wire contract. |

### Bundle delta post-merge

| Chunk | Pre-merge (post FilipDz) | Post-0254 merge | Delta |
|---|---:|---:|---:|
| main `index-*.js` | 596.20 KB / 189.36 KB gz | 596.20 KB / 189.37 KB gz | +0 raw / +0.01 KB gz (noise) |
| `LiquidityPoolDetailPage-*.js` | 307.12 KB / 93.81 KB gz | 307.15 KB / 93.81 KB gz | +0.03 KB raw / 0 gz |
| `TransactionDetailPage-*.js` | 29.97 KB / 9.13 KB gz | 29.97 KB / 9.13 KB gz | 0 / 0 |
| `usePageHandlers-*.js` | n/a (not separately chunked before) | 2.35 KB / 1.16 KB gz | NEW chunk — page handlers now shared across all paginated pages |

F-AI-1, F-AI-2 STILL STAND at same numbers. New `usePageHandlers`
shared chunk is a small reuse improvement (was previously inlined per
page; now extracted).

### Deterministic delta check results

| Check | Result |
|---|---|
| `nx run-many -t typecheck` | exit 0, 0 errors (baseline holds; `cursor` → `next_cursor` rename caught zero consumer drift) |
| `nx run-many -t lint` | 1 problem (0 errors, 1 warning) — same `assetColor.ts:131` baseline |
| `nx build web` | exit 0, 2.23s, bundle deltas above |
| `grep -rnE "\.cursor\b" web/src libs/ui/src` (excluding generated) | 2 hits in `useCursorPagination.ts:22,118` — both `state.cursor` (URL-local cursor read), intentional. Zero stale refs to old `page.cursor` API shape. |
| `grep -rnE "\.has_more\b" web/src libs/ui/src` | 0 hits — clean rename. |
| `grep -rnE "\.next_cursor\b|\.prev_cursor\b" libs/ui/src` | 4 hits, all in `usePageHandlers.ts:47-50` (correct single consumer). |
| `grep "as any|@ts-ignore|@ts-expect-error" <touched files>` | 0 hits |
| `grep "fetch\(|axios" <touched files>` | 0 raw fetches (5 `refetch()` from TanStack — false positive) |

### Re-audit queue (work absorbed into Wave 4)

| Sub-phase | Scope | Effort | Priority |
|---|---|---:|---|
| 1.1 OpenAPI adherence | Verify regenerated `types.gen.ts` shape vs `openapi.json`; check no manual fetches in touched files (done above) | 5 min | Wave 4 |
| 1.4 API consistency | `cursor` → `next_cursor` rename consistency (done above — clean) | 0 (done) | done |
| 1.5 D2/D9 cells for E2/E4/E7/E10/E12 (list pages) | Re-verify pagination URL contract on Next + Prev + refresh + deep-link, per list page | 30 min Playwright | **Wave 4** |
| 1.5 D2/D9 cells for tab tables (E6, E8, E9, E11, E13) | Same scope on `?cursor_p=` / `?cursor_e=` / `?cursor_i=` per-section cursors | 25 min Playwright | Wave 4 |
| 1.6 console | F-E-2 DROPPED per design decision. Any API 400 from malformed `?op=` is expected baseline (user error, not FE bug). Record context note, do NOT log as console finding. | 0 (no new scope) | Wave 4 |
| 1.7 cross-entity links | List page row link rendering untouched by 0254 | 0 (no new scope) | done |
| 1.9 component reuse | `usePageHandlers` now extracted shared chunk — uniform usage across 13 pages. Confirms hook is the right level of abstraction. | 5 min | Wave 4 |
| 1.11 P / 1.11b AQ | Re-ran (done above — baseline holds) | 0 (done) | done |
| 1.13 URL state | F-E-1 resolved; F-E-2 + F-E-3 + F-E-7 still stand | 0 (done in this pass) | done |
| 1.16 bundle | Delta captured above | 0 (done) | done |
| 1.18 Q+AR lore | ADR 0008 amendment + 0254 archive: check task close metadata for completeness | 10 min | Wave 4 |

**Total delta-audit effort: ~75 min**, all absorbable into Wave 4
baseline. No separate session required.

### Recommendation

- **Wave 4 starts immediately.** F-E-1 resolved removes the #1
  audit-blocker. F-E-2 single-file fix can either be spawned as
  audit-blocker (preserves baseline cleanliness) or absorbed as a
  known-noise tag during 1.6 console review (cheaper). Default: keep
  audit-blocker for F-E-2, single fix, then start Wave 4.
- Audit branch tip is now `6af74d82` — pin Wave 4 baseline to this SHA.
