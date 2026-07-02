# A + AC — Spec / source consistency (Wave 5 1.2)

**Wave:** 5 (Tier 4 subjective)
**Stance:** senior fresh-eye; spec / Figma / task / code = interpretations,
any can be wrong.
**Date:** 2026-05-25
**Baseline SHA:** `81928602` (audit tip, post-0254 merge).

## Per-check table

| #     | Check                                                                     | Verdict | Evidence                                                                                                                                                                                                                                                    | Severity | Class |
| ----- | ------------------------------------------------------------------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ----- |
| AC-1  | OpenAPI vs `crates/api/**` zero-diff (`api-types:check-generated`)        | ✓       | Local run exit 0; `git diff --exit-code -- libs/api-types/src/{openapi.json,generated}` clean                                                                                                                                                               | —        | —     |
| AC-2  | CI gate "API types freshness" wired                                       | ✓       | `.github/workflows/ci.yml:88-97` `api-types-codegen` job runs `nx run @rumblefish/api-types:check-generated`; paths-filter on `crates/api/**`, `Cargo.{toml,lock}`, `libs/api-types/**`                                                                     | —        | —     |
| AC-3  | FE uses generated types, no inline `any`                                  | ✓       | Cross-cite Wave 1 `findings/AF-openapi-adherence.md` AF-1..AF-4. Re-verified post 0254 merge: `grep "as any\|@ts-ignore"` → 0; `grep "(interface\|type) [A-Z][A-Za-z]*Response"` → 0 in web/src + libs/ui/src                                               | —        | —     |
| AC-4  | Generated types match openapi (round-trip)                                | ✓       | Spot-check LedgerListItem, AccountDetailResponse, PageInfo: openapi.json shape ↔ types.gen.ts shape identical; required arrays match                                                                                                                        | —        | —     |
| AC-5  | 5 sampled endpoints — handler/openapi/types alignment                     | ✓       | See "Per-endpoint sample" table below — all 5 align                                                                                                                                                                                                         | —        | —     |
| AC-6  | Spec drift detection (post-shipping task body adjustments)                | ⚠       | See F-A-1 / F-A-2 below                                                                                                                                                                                                                                     | 🟡 / 🟢  | D     |
| AC-7  | ADR 0032 evergreen-docs gate honored on shape-changing PRs (last 4 weeks) | ⚠       | See F-A-3 table below — 4/5 sampled PRs comply; 1 gap                                                                                                                                                                                                       | 🟡       | D     |
| AC-8  | Per-feature spec ↔ task ↔ docs ↔ code coherence (3 features sampled)      | ⚠       | See F-A-4 / F-A-5 below                                                                                                                                                                                                                                     | 🟡       | D     |
| AC-9  | Each inconsistency documented as deviation in task body                   | ✓       | 0073 / 0074 / 0077 all have explicit "matches Figma" / "Figma took priority" notes in Design Decisions or AC delta                                                                                                                                          | —        | —     |
| AC-10 | FE response handling matches `@rumblefish/api-types`                      | ✓       | Single API entry point (`web/src/api/client.ts`) + 26 hook files (each `useXxx`) all import `*Options` from generated client. No custom response mappers, no extension fields.                                                                              | —        | —     |
| AC-11 | Custom response mappers in `web/src/api/`                                 | ⚠       | Only 1: `web/src/api/client.ts:11-29` error interceptor flattens `ErrorEnvelope` → vanilla `Error`. Cross-cite F-AF-1 (Gate A accepted)                                                                                                                     | 🟡       | A     |
| AC-12 | Test fixtures match prod response shape (dev-mock-api.mjs)                | N/A     | `tools/dev-mock-api.mjs` not present in worktree (referenced in original Wave 5 plan but no such file). `web/dev-mock-server.mjs` referenced in 0072 archive is gitignored / not in tree. **Real backend used for dev** (cross-cite Wave 1 AF-table row 12) | —        | —     |
| AC-13 | Each unchecked AC has spawned task                                        | ⚠       | Cross-cite Wave 1 A3 (25/28 Future Work without spawned task) — Phase 3 sub-phase 3.2 owns bulk-spawn                                                                                                                                                       | 🟠       | D     |
| AC-14 | Backend response shape vs spec promise                                    | ✓       | Sampled 3 endpoints below — backend response = utoipa schema = OpenAPI = generated types (single Rust source of truth via `utoipa-axum`)                                                                                                                    | —        | —     |

## Per-endpoint sample (AC-5 evidence)

5 endpoints sampled — each compared across handler (Rust `#[utoipa::path]` attr) → `openapi.json` → `types.gen.ts` → FE consumer:

| Endpoint                 | Handler:line                                                     | Spec source             | OpenAPI required[]                                                                                             | TS generated         | FE consumer hook                                                                                                              | Match? |
| ------------------------ | ---------------------------------------------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------- | -------------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------ |
| `GET /ledgers`           | `crates/api/src/ledgers/handlers.rs:48` `list_ledgers`           | utoipa attr lines 32-46 | `LedgerListItem` required: `sequence, hash, closed_at, protocol_version, transaction_count, base_fee`          | matches; same 6 keys | `useLedgersList.ts` → `listLedgersOptions`                                                                                    | ✓      |
| `GET /ledgers/:sequence` | `crates/api/src/ledgers/handlers.rs:125` `get_ledger`            | utoipa attr             | `LedgerDetailResponse` adds `prev_sequence / next_sequence / transactions`                                     | matches              | `useLedgerDetail.ts`                                                                                                          | ✓      |
| `GET /accounts/:id`      | `crates/api/src/accounts/handlers.rs:40` `get_account`           | utoipa attr             | `AccountDetailResponse` required: `account_id, sequence_number, balances, first_seen_ledger, last_seen_ledger` | matches              | `useAccountDetail.ts`                                                                                                         | ✓      |
| `GET /transactions`      | `crates/api/src/transactions/handlers.rs:54` `list_transactions` | utoipa attr             | `Paginated_TransactionListItem` + `page: PageInfo {limit, next_cursor?, prev_cursor?}`                         | matches              | `useTransactionsList.ts` → `usePageHandlers` reads `next_cursor / prev_cursor` (`libs/ui/src/table/usePageHandlers.ts:47-48`) | ✓      |
| `GET /liquidity-pools`   | `crates/api/src/liquidity_pools/handlers.rs:223` `list_pools`    | utoipa attr             | `PoolItem` required: `..., participant_count` (post-0246 Phase 2)                                              | matches              | `usePoolsList.ts`; participants displayed via `PoolsTable.tsx`                                                                | ✓      |

**Verdict:** spec-to-code round-trip is clean. Single Rust source-of-truth flows through utoipa → openapi.json → openapi-ts → web/src consumers without manual touch-up.

## Findings

### F-A-1 [Class D, Severity 🟡] — Spec drift detection: 0246 Phase 3 dropped mid-implementation, task body updated to reflect

- **Evidence:** `lore/1-tasks/archive/0246_FEATURE_backend-liquidity-pools-api-extensions.md:157-173` — Phase 3 (`total_count` envelope on participants endpoint) marked **DROPPED** in task body during implementation. Rationale fully documented in Design Decisions → Emerged.
- **Spec ↔ shipped reality:** matches today. Phase 1 (asset_code filter) + Phase 2 (`participant_count` on `PoolItem`) shipped; Phase 3 explicitly dropped with rationale.
- **OpenAPI confirms:** `participant_count` present in `Paginated_PoolItem` (verified above); no `total_count` field anywhere in participants endpoint shape.
- **Net:** **healthy spec hygiene** — when work changes mid-flight, the task body is updated, not silently drifted. Phase 3 strikethrough discipline + rationale capture is the right pattern.
- **Class:** D (informational — no fix needed). Documenting as **positive baseline** to contrast with cases where drift goes silent.

### F-A-2 [Class D, Severity 🟡] — 0254 BREAKING wire rename (`cursor → next_cursor` + `has_more` dropped) handled cleanly across spec/openapi/types/FE

- **Evidence chain:**
  - Task body (`lore/1-tasks/archive/0254_*.md:42`) explicitly tags `refactor!:` (Conventional Commits breaking)
  - ADR 0008 amended (commit `28763081`) with cursor direction encoding section
  - OpenAPI regen committed (`ce582861`) in same task branch
  - FE consumer migrated (`f646047d` drop FE prev-stack)
  - All 4 commits attributed `lore-0254` scope
  - Cross-cite triage-gate-A.md post-merge update: `grep "\.has_more\b"` → 0 in web/src; `grep "\.cursor\b"` → 2 hits in `useCursorPagination.ts`, both intentional URL-local cursor reads (not page response field)
- **Verdict:** breaking change followed the full ceremony — spec amended, openapi regenerated, types regenerated, FE migrated, all in one task branch. No spec/openapi/code drift.
- **Class:** D (positive baseline note).

### F-A-3 [Class D, Severity 🟡] — ADR 0032 evergreen-docs gate: 4/5 sampled recent merges comply; 1 gap (0254)

Sample of recent merge commits touching `crates/api/**` or `libs/api-types/**` since 2026-04-25 (per task README ADR 0032 spot-check directive):

| Merge                                     | Subject                                | `crates/api/**` or `libs/api-types/**` touched?                                                         | `docs/architecture/**` updated in same merge?                                                                                                               | Verdict                                                                    |
| ----------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| `d5cca013` (0246)                         | LP API extensions                      | ✓ (handlers/dto/queries)                                                                                | ✓ `docs/architecture/backend/backend-overview.md` (+22/-..) + `docs/architecture/frontend/frontend-overview.md` (+7/-..)                                    | ✓ compliant                                                                |
| `a2c1b205` (0070+0071 FE tx-detail)       | FE transaction detail page             | ✗ (no crates/api / libs/api-types touched in this PR)                                                   | N/A — pure FE PR                                                                                                                                            | ✓ N/A (correct per ADR 0032 — gate applies only to shape-changing changes) |
| `6af74d82` (0254)                         | `prev_cursor` + direction-aware cursor | ✓ (handlers, queries, common/cursor, common/pagination, openapi/schemas, ADR 0008 amend + openapi.json) | partial — `docs/architecture/frontend/frontend-overview.md` +10/-.. only; **NO `docs/architecture/backend/**` update\*\* for the pagination contract change | ⚠ partial gap                                                              |
| `353c0907` (0241 indexer hard-swap PG→CH) | indexer write path swap                | ✓ (touches infra + indexing) — beyond direct api scope, but architectural                               | (out of scope here — different layer)                                                                                                                       | (not sampled)                                                              |
| `9ad14df2` (0249 destroy AWS infra)       | infra teardown                         | infra-only                                                                                              | N/A                                                                                                                                                         | ✓                                                                          |

- **F-A-3 finding:** 0254 (BREAKING wire rename + new field) updated FE overview but did not update backend overview / pagination doc with the new `next_cursor`/`prev_cursor` shape. ADR 0008 amendment in `28763081` covers the spec-level decision; but the **prose-doc layer** (`docs/architecture/backend/`) was not synced.
- **Impact:** docs/architecture/backend continues to describe the prior `cursor + has_more` shape (pre-0254). New backend contributors reading the docs first would build to the old API contract.
- **Class:** D (catalog-only; Phase 3 batch as "doc-sync sweep for 0254 wire rename").
- **Recommendation:** Phase 3 spawn `XXXX_DOCS_evergreen-doc-sync-0254-pagination` — single PR adding next_cursor / prev_cursor shape to `docs/architecture/backend/backend-overview.md` pagination section.

### F-A-4 [Class D, Severity 🟡] — Per-feature triangulation: liquidity-pool detail

Sampled feature spec/Figma/docs/code consistency. **Note:** Figma read-only inspection deferred to Wave 6 per task README; this check is **prose/spec/code only**.

| Source                                            | Coverage of LP detail                                                                                                                                                                                                                     | Notes                                              |
| ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Spec task (0077 archive)                          | Full Figma alignment notes (Acceptance Criteria, Issues Encountered, Emerged Decisions list 13 items) — detailed; clearly authored after the work                                                                                         | ✓                                                  |
| Spec task (0246)                                  | Backend support for FE LP needs — participant_count + asset_code filter shipped; LP per-tx amounts split to 0247 RESEARCH (still blocked)                                                                                                 | ✓ — explicit defer doc                             |
| Backend code                                      | `crates/api/src/liquidity_pools/{handlers,dto,queries}.rs` + 13 SQL files in docs/architecture/database-schema/endpoint-queries/{18,19,20,21,22}\*.sql                                                                                    | ✓                                                  |
| OpenAPI                                           | `PoolItem` + `PoolDetailResponse` + `PageInfo` all present                                                                                                                                                                                | ✓                                                  |
| `docs/architecture/frontend/frontend-overview.md` | §6.13 + §6.14 cover LP list + detail                                                                                                                                                                                                      | ✓ — per Q-6 in Wave 2 verified updated in same PRs |
| Frontend code                                     | `web/src/pages/LiquidityPoolsListPage.tsx` + `web/src/pages/LiquidityPoolDetailPage.tsx` + `web/src/pages/{liquidity-pools,pool-detail}/*`                                                                                                | ✓                                                  |
| Gap                                               | 0199 (LP analytics — TVL/volume/fee_revenue per-snapshot) blocked-on-oracle; FE pool detail shows placeholder "Chart data not yet available — pending oracle (task 0199)". 0215 (FE impact catalog) blocked on 0199. Cross-cite Wave 1 A5 | ⚠ structural but already-documented gap            |

- **Verdict:** LP feature is **the best-documented spec ↔ task ↔ docs ↔ code chain** in the project. Drift surface is minimal; gaps (0199, 0247) are explicitly tracked.
- **Subjective:** this feature could serve as the gold-standard exemplar for future feature documentation expectations in `lore/3-wiki/`.
- **Class:** D (recommendation: cite as model in Phase 3 wiki sub-phase 3.5).

### F-A-5 [Class D, Severity 🟡] — Per-feature triangulation: contract detail

| Source                           | Coverage                                                                                                                                                                                | Notes                                                  |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Spec task (0075 archive)         | Contract detail page — explicit Future Work list with 5 items (events count for tab pill, wasm_interface_metadata JSONB doc, SAC SEP-41 stub, contracts list page, /contracts nav link) | partial — items deliberately deferred                  |
| Backend code                     | `crates/api/src/contracts/{handlers,dto,queries}.rs`                                                                                                                                    | ✓                                                      |
| OpenAPI                          | `ContractDetail` + `EventItem` + `InvocationItem` + `ContractInterfaceResponse` present                                                                                                 | ✓                                                      |
| docs/architecture/frontend §6.10 | Contract route documented                                                                                                                                                               | ✓                                                      |
| Frontend code                    | `web/src/pages/ContractDetailPage.tsx` + `web/src/pages/contracts/*` (ContractEvents, ContractInvocations, ContractInterface, ContractSummary, interfaceMetadata.ts)                    | ✓                                                      |
| Gap 1                            | **No `/contracts` list page** + **no `/contracts` nav link** — contract detail unreachable by browsing (deep-link only). Cross-cite Wave 1 A3 / archaeology Recommendation 2            | 🟠 (launch blocker per archaeology) — separate finding |
| Gap 2                            | 0075 #6 Emerged: "`interface_metadata` hand-typed from indexer source, not OpenAPI" → drift risk if backend changes shape                                                               | already flagged in archaeology Emerged audit           |

- **Verdict:** spec/docs alignment good. Code has a structural gap (no list, no nav) that's a launch blocker but **fully tracked in the originating task body and in Wave 1 archaeology**.
- **Class:** D (no new finding — already in Phase 3 spawn pipeline).
- **design_parity update 2026-05-27 (`06ab34cc`):** F-A-5 **Gap 1 PARTIAL.** The `feat/design_parity` merge added `/contracts` + `/accounts` to `NAV_LINKS` (routes.ts) AND as routes — so the **nav-link half is DONE** (contract detail now reachable by browsing via the nav entry). BUT both routes render via the `<PageStub>` placeholder, NOT a real list page — the **list-page half is still TODO**. Net: F-A-5 → PARTIAL (queue card 1.3 → PARTIAL). Side effect: PageStub is now a live consumer (2 routes), which invalidates F-AH-1 "PageStub dead orphan, delete" — see AH-file-folder-structure / card 2.2 scope conflict. Source: `design-parity-impact-2026-05-27.md` §1 + §5.
- **design_parity ROUND 2 update 2026-05-29 (PR #224, `fce0d666` / merge `35ac27c0`):** F-A-5 **Gap 1 — accounts/contracts now SPLIT.** R2 shipped a **REAL `/accounts` list page** — `web/src/pages/AccountsListPage.tsx` + `web/src/api/hooks/useAccountsList.ts` + `accounts/AccountsTable.tsx` + `accounts/AccountsFilters.tsx` (cursor pagination, filters, sort, empty/error/loading states), route wired `router/index.tsx:48`. So Gap 1's **accounts half is RESOLVED** (PageStub → real page). **`/contracts` half STILL TODO** — `router/index.tsx:66` still renders `<PageStub title="Contracts">`. Net: F-A-5 stays **PARTIAL** (queue card 1.3 stays PARTIAL) until `/contracts` real list ships. Side effect on F-AH-1: PageStub now has **1** live consumer (`/contracts` only — `/accounts` graduated); still not deletable while contracts stubbed. `/accounts` live re-verify queued. Source: `design-parity-impact-2026-05-29.md` §1, §2, §3.

### F-A-6 [Class D, Severity 🟢] — Per-feature triangulation: transaction detail

| Source                           | Coverage                                                                                                                                                                               | Notes         |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| Spec tasks (0070 + 0071 archive) | Both originally backlog, recently shipped post-0257-Wave-3 (a2c1b205 merge). Task bodies were live during execution. Per archaeology resolution: 1799 LOC across normal/advanced split | ✓             |
| Backend code                     | `crates/api/src/transactions/{handlers,dto,queries}.rs`                                                                                                                                | ✓             |
| OpenAPI                          | `E3ResponseTransactionDetailLight` + `OperationItem` + `XdrOperationDto`                                                                                                               | ✓             |
| docs/architecture/frontend §6.4  | Updated in 0070+0071 PR                                                                                                                                                                | ✓             |
| Frontend code                    | `web/src/pages/transaction-detail/{index,advanced,normal,sections,shared}/*.tsx`                                                                                                       | ✓             |
| Drift candidates                 | Wave 4 surfaced: F-J-16 (duplicate `formatFee` impl), F-U-3/4 (truncation + STROOPS dups), F-AQ-7/8 (heavy unknown casts in XDR details). All Class C/D defer.                         | informational |

- **Verdict:** spec→ship chain clean. Code-quality follow-ups identified in Wave 4 are separable from spec consistency.
- **Class:** D.

### F-A-7 [Class D, Severity 🟢] — Task body deviation notes: sample of 3 archived FE tasks

Per task README 1.2 directive — check whether archived FE task bodies mention deviations from spec.

| Task                      | Has deviation note? | Quote                                                                                                                                                                                                  |
| ------------------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 0073 (account detail)     | ✓                   | `0073:201-202`: "Source account column dropped — the Figma account-transactions table has no Source account column though the original spec listed one. Figma took priority…"                          |
| 0074 (assets list/detail) | ✓                   | `0074:237-243`: "Type filter is chips, not a dropdown — the Figma filter is a chip row …Figma took priority; acceptance criteria updated to match"; "Asset-transactions table shows Ledger, omits Fee" |
| 0077 (LP list/detail)     | ✓                   | Per Wave 1 archaeology — 13 Emerged decisions captured; AC delta notes; Figma alignment fully documented                                                                                               |

- **Verdict:** **deviation documentation discipline is excellent**. Every sampled spec divergence has explicit narrative in task body. Cross-cite "Figma-first frontend work" `feedback_figma_first` memory pattern — team follows it.
- **Class:** D (positive baseline). No fix.

## Cross-cites

- **AC-3** confirms Wave 1 AF-table: zero `as any` / `@ts-ignore` / locally-redeclared response types in `web/src` + `libs/ui/src`. Post-0254 merge re-verified.
- **AC-11** restates Gate A F-AF-1 (error interceptor) — already accepted baseline + Phase 3 refactor.
- **AC-13** restates Wave 1 A3 — 25/28 Future Work un-spawned, Phase 3 owns.
- **F-A-3** new finding (this wave) — only spec/source-consistency gap discovered.

## Net 1.2 finding count

7 findings: 0 🔴 / 0 🟠 / 4 🟡 / 3 🟢.

**Class breakdown:** A=1 (cross-cite) / D=7.

**Subjective calls (Tier 4 flagging for user spot-check):**

1. F-A-3 — partial doc-sync gap on 0254. Severity 🟡 may be 🟢 if "ADR amendment + frontend-overview update" is judged sufficient and backend-overview is considered downstream. **User decision.**
2. F-A-4 — naming LP feature as gold-standard exemplar is a subjective judgment based on triangulating 4 sources. Could be a Phase 3 wiki investment or noise.

## Top issue

**F-A-3 partial ADR 0032 gap on 0254 PR.** Single doc-sync follow-up in Phase 3.
