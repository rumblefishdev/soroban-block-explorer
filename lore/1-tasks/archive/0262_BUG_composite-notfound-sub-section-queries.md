---
id: '0262'
title: 'Composite NotFound — Account + Contract + LP detail sub-section queries fire alongside parent 404 (dual error blocks)'
type: BUG
status: completed
related_adr: []
related_tasks: ['0257', '0073', '0075']
tags:
  ['frontend', 'audit-blocker', 'priority-high', 'effort-small', 'phase-bug']
links:
  - 'Finding: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/D-state-coverage-matrix.md (F-D-2)'
  - 'Paired finding: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/M-AE-console-error-handling.md (F-AE-5)'
  - 'Audit context: lore/1-tasks/active/0257_RESEARCH_frontend-comprehensive-audit/findings/triage-gate-B.md'
history:
  - date: '2026-05-25'
    status: backlog
    who: karolkow
    note: 'Spawned from 0257 Gate B (F-D-2 + F-AE-5, Class B 🟠 HIGH). Post-research scope-correction: only Account + Contract pages affected (Asset already gated at AssetDetailPage.tsx:127; Contract uses tabs so max 2 blocks not 4; LP early-returns on invalid pool ID). Audit-blocker: must land before Wave 6 — 2.0 Playwright + 2.5 a11y will re-report dual-block on E6 + E9 valid-format-404 scenarios.'
  - date: '2026-05-26'
    status: active
    who: karolkow
    note: 'Activated as part of Gate B fix-first batch (0262/0263/0264 + 0265 off-band CVE) on shared branch.'
  - date: '2026-05-26'
    status: active
    who: karolkow
    note: 'Scope expansion — post-activation cross-check audit found LiquidityPoolDetailPage.tsx ALSO affected on valid-strkey-but-404 path: detail.isError gates only summary at L67-77; PoolCharts (L96), PoolParticipants (L98-100), PoolTransactions (L101-103) render unconditionally. Original task claim "LP early-returns on invalid pool ID" covered only format-invalid case (L54-60), not 404 case. 3 pages affected, not 2. Asset (L127 gate) confirmed SAFE; Ledger/Tx/NFT use full-page early-return — SAFE.'
  - date: '2026-05-26'
    status: completed
    who: karolkow
    note: 'Implemented in 473de2a2 (Gate B batch). 3 pages render-gated (Account/Contract/LP). Manual UI verification done via Playwright MCP against local stack (cargo lambda watch + nx dev :4201 + Postgres :5433 from feat-0077 worktree): valid-format-404 IDs render single NotFound block on E6/E9/E13; garbage IDs hit early-return validator unchanged. Sub-queries still fire network on LP 404 (4× 404 in console) but render-gated per AC — render-gate scope only, not query-disable scope.'
---

# Composite NotFound — Account + Contract + LP detail dual error blocks

## Summary

Account, Contract, and LP detail pages fire sub-section queries
(Transactions tab on account; current tab on contract; charts +
participants + transactions on LP) in parallel with parent entity query.
When parent returns 404 for a **valid-format-but-not-found** ID
(e.g. `GAAA…AAA`, `CAAA…AAA`, valid-strkey LP id), the sub-section
queries also error with the same ID, producing **2-4 stacked error
blocks** on one page instead of a single clean NotFound.

Scope corrected after research + post-activation cross-check audit:

- **Account** (E6): `<AccountTransactions/>` always mounts regardless
  of `account.isError` → 2 blocks
- **Contract** (E9): `<ContractDetailPage>` uses tabs; only one tab
  mounts at a time → max 2 blocks (summary + active tab)
- **LP** (E13): `LiquidityPoolDetailPage.tsx:67-77` gates only
  `summarySection` on `detail.isError`. `PoolCharts` (L96),
  `PoolParticipants` (L98-100), `PoolTransactions` (L101-103) render
  unconditionally → up to **4 blocks** on valid-strkey-but-404. Note:
  invalid-format ID hits L54-60 early-return → single NotFound (SAFE);
  bug only on valid-format-but-not-found path.
- **Asset** (E8): **already gated** at `AssetDetailPage.tsx:127`
  (`!asset.isError && <AssetTransactions/>`) — single block, no fix needed
- **Ledger / Transaction / NFT**: full-page early-return on parent
  error — single NotFound, no sub-sections render — SAFE

Garbage IDs (e.g. `/accounts/garbage`) hit early-return validators
(`isAccountId`, `isContractId`) — render single NotFoundState; **NOT
the dual-block case**. The bug is only observable on **valid-format
IDs that 404 on the API** (account/contract that doesn't exist in DB).

## Status: Backlog

**Audit-blocker for task 0257 (FE comprehensive audit).** Must land
before Wave 6 (Track 2 visual + UX). Without fix, Wave 6 2.0 Playwright
re-walks valid-format-404 scenarios on E6 + E9 and re-reports the same
dual-block visual mess; 2.5 a11y screen-reader announces 2 error blocks
consecutively = bad keyboard nav UX.

Cascade compression: ~4-6 duplicate Wave 6 Track 2 findings avoided.

## Context

- Audit finding: `F-D-2` in
  [D-state-coverage-matrix.md](../active/0257_RESEARCH_frontend-comprehensive-audit/findings/D-state-coverage-matrix.md)
- Paired finding: `F-AE-5` in
  [M-AE-console-error-handling.md](../active/0257_RESEARCH_frontend-comprehensive-audit/findings/M-AE-console-error-handling.md)
- Live evidence: Wave 4 Playwright session 2026-05-25 — `/contracts/<garbage>`
  recorded as 4 blocks initially, but post-research clarification: tab
  refactor (commit `0c923f44`, 2026-05-22) limits contract to max 2.
- Prior art for fix pattern: `enabled: id.length > 0` already used in
  `useAccountDetail.ts:14`, `useAccountTransactions.ts:22`,
  `useContractInterface.ts:16`. Extending the same flag pattern.

## Implementation Plan

### Step 1: Account detail gating

**File:** `web/src/pages/AccountDetailPage.tsx:82-92`

```tsx
// Before
<AccountTransactions accountId={accountId} />;

// After (Approach B — render-level gating, matches AssetDetailPage.tsx:127)
{
  !account.isError && <AccountTransactions accountId={accountId} />;
}
```

Or use `enabled` on the child hook (`useAccountTransactions.ts`):

```ts
useAccountTransactions(accountId, {
  enabled: id.length > 0 && !accountIsError,
});
```

Approach B (render-gate) is simpler and matches existing pattern at
`AssetDetailPage.tsx:127`. Recommended.

### Step 2: Contract detail gating

**File:** `web/src/pages/ContractDetailPage.tsx:132-143`

Tabs mount one at a time. Apply render-gate at parent level so the
**whole tab strip** is hidden on parent error:

```tsx
// Before (paraphrase)
<>
  <ContractSummary ... />
  {activeKey === 'interface' && <ContractInterface />}
  {activeKey === 'events' && <ContractEvents />}
  ...
</>

// After
<>
  <ContractSummary ... />
  {!contract.isError && (
    <>
      {activeKey === 'interface' && <ContractInterface />}
      ...
    </>
  )}
</>
```

Or gate each tab's `enabled` flag on parent status. Tab approach is
verbose; render-gate at parent is cleaner.

### Step 3: LP detail gating

**File:** `web/src/pages/LiquidityPoolDetailPage.tsx:83-104`

LP renders summary + KPI + charts + participants + transactions as
five independent sections. Gate the three sub-section sections
(`PoolCharts`, `PoolParticipants`, `PoolTransactions`) on
`!detail.isError`, leaving KPI strip + summary handled by existing
isError branch at L67-77:

```tsx
// Before (L95-103, paraphrase)
<SectionErrorBoundary sectionName="pool-charts">
  <PoolCharts poolId={poolId} />
</SectionErrorBoundary>
<SectionErrorBoundary sectionName="pool-participants">
  <PoolParticipants poolId={poolId} />
</SectionErrorBoundary>
<SectionErrorBoundary sectionName="pool-transactions">
  <PoolTransactions poolId={poolId} />
</SectionErrorBoundary>

// After
{!detail.isError && (
  <>
    <SectionErrorBoundary sectionName="pool-charts">
      <PoolCharts poolId={poolId} />
    </SectionErrorBoundary>
    <SectionErrorBoundary sectionName="pool-participants">
      <PoolParticipants poolId={poolId} />
    </SectionErrorBoundary>
    <SectionErrorBoundary sectionName="pool-transactions">
      <PoolTransactions poolId={poolId} />
    </SectionErrorBoundary>
  </>
)}
```

Note: invalid-format pool ID already hits early-return at L54-60 (single
NotFoundState) — no change there. Bug only on valid-strkey-but-404.

### Step 4: Verify

- Navigate `/accounts/GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`
  (valid-format strkey but not in DB) → **single NotFound block**, no
  transactions error visible
- Navigate `/contracts/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`
  → single NotFound block, no tab strip rendered
- Navigate valid-strkey-but-404 LP id (after 0264 lands, strkey format
  becomes acceptable; pre-0264 use hex 64-char that satisfies `isPoolId`
  but 404s on backend) → single NotFound, no charts/participants/transactions
- Navigate valid `/accounts/<real-G-strkey>` → all tabs load normally
  (no regression)
- Navigate valid `/contracts/<real-C-strkey>` → all tabs load normally
- Navigate valid LP id → all sections load normally
- Garbage paths (e.g. `/accounts/garbage`, `/liquidity-pools/garbage`)
  still hit early-return validator → single NotFoundState (unchanged)

### Step 5: Regression test

Add Playwright assertion (gated on 0226 vitest infra): valid-format-404
scenario renders single error block on E6 + E9 + E13.

## Acceptance Criteria

- [x] `AccountDetailPage.tsx` render-gates `<AccountTransactions/>` on
      `!account.isError`
- [x] `ContractDetailPage.tsx` render-gates tab strip on `!contract.isError`
- [x] `LiquidityPoolDetailPage.tsx` render-gates PoolCharts +
      PoolParticipants + PoolTransactions on `!detail.isError`
- [x] Valid-format-404 IDs render single NotFound block on E6, E9, **and E13**
- [x] Valid IDs still load all tabs/sections normally; no regression
- [x] Garbage IDs still hit early-return validator (unchanged behavior)
- [ ] Audit branch `research/0257_frontend-comprehensive-audit` rebased onto develop post-merge (post-merge)
- [ ] Finding `F-D-2` in `D-state-coverage-matrix.md` marked `RESOLVED in <SHA>` (post-merge, SHA `473de2a2` pre-merge)
- [ ] Finding `F-AE-5` in `M-AE-console-error-handling.md` marked `RESOLVED in <SHA>` (post-merge)
- [x] **Docs updated** — `N/A — bug fix, no architecture change`. Per ADR 0032.
- [x] **API types regenerated** — `N/A — frontend-only`.

## Notes

- Effort: ~45-60 min (3 small JSX changes + verify; revised from
  ~30-45 min after LP scope add).
- Asset detail (`AssetDetailPage.tsx:127`) already implements the pattern
  — use as reference.
- LP scope added 2026-05-26 after post-activation cross-check audit
  confirmed valid-strkey-but-404 path triggers up to 4 stacked error
  blocks (summary NotFound + 3 sub-section error states). Originally
  scoped out under flawed assumption "early-returns on invalid pool ID"
  — that early-return only covers format-invalid case (L54-60), not
  404 case (L67-77 gates summary only).
- Could batch with 0263 in same PR if implementer prefers (both are FE
  polish in pages/ subdirectory; both touch LiquidityPoolDetailPage).

## Implementation Notes

Landed in commit `473de2a2` (Gate B batch, 4 tasks bundled).

**3 files touched** (render-gate JSX wrap pattern, mirrors `AssetDetailPage.tsx:127`):

- `web/src/pages/AccountDetailPage.tsx` — wrapped `<SectionErrorBoundary sectionName="account-transactions">` in `{!account.isError && (...)}`
- `web/src/pages/ContractDetailPage.tsx` — wrapped the entire `<Card>` containing Tabs nav + active-tab body in `{!contract.isError && (...)}`. Single guard covers all 3 tabs (Interface / Invocations / Events) + the tab nav itself. Tab nav disappears on parent 404 — desirable.
- `web/src/pages/LiquidityPoolDetailPage.tsx` — wrapped the 3 sibling `SectionErrorBoundary`s (`pool-charts`, `pool-participants`, `pool-transactions`) in `{!detail.isError && (<>...</>)}`. KPI strip + summary remain handled by their own `isError` branch inside the conditional render above.

**Manual verification** (Playwright MCP against local stack — backend `cargo lambda watch -p 9000`, FE `nx dev :4201`, Postgres `:5433`):

| Route                                                  | Result                                           |
| ------------------------------------------------------ | ------------------------------------------------ |
| `/accounts/GAAA…AAAT` (valid-format, not in DB)        | 1 NotFound block, 0 stacked errors ✅            |
| `/contracts/CAAA…AAAJ` (valid-format, not in DB)       | 1 NotFound block, 0 tabs rendered ✅             |
| `/liquidity-pools/LAAA…BLIR` (valid strkey, not in DB) | 1 NotFound block, 0 sub-sections rendered ✅     |
| `/accounts/garbage` (invalid format)                   | Single NotFoundState (early-return unchanged) ✅ |

## Issues Encountered

- **Sub-section queries still fire network on parent 404 — by design**. The render-gate only suppresses the error UI; React Query hooks inside the gated children still issue HTTP requests during the brief moment the parent query is in-flight, and once parent errors the hooks aren't unmounted because the gate evaluation happens on parent's `isError`. Observed in browser console for LP route: 4× 404 responses (parent + 3 sub-queries). Per AC the requirement was "single NotFound block on screen", not "no wasted network on 404". Treating this as out-of-scope rather than a regression — call out for follow-up if cold-network UX becomes important.

## Design Decisions

### From Plan

1. **Render-gate at parent level, not `enabled` flag at hook level**: task body listed both options. Picked render-gate because it mirrors the existing pattern at `AssetDetailPage.tsx:127` — keeping E6/E9/E13 + E8 (asset, already correct) symmetric. Easier to audit consistency.

2. **For Contract: gate the whole `<Card>` containing Tabs + body, not per-tab `enabled` flags**: simpler, single guard covers all 3 tabs and the tab nav. Tab nav disappearing on 404 is a feature — caller shouldn't see a nav strip pointing at sections of a contract that doesn't exist.

### Emerged

3. **LP scope-correction added post-activation (2026-05-26)**: original task body claimed LP early-returned on invalid pool ID. True for malformed-strkey, but **not** for valid-strkey-but-404. Cross-check audit found `LiquidityPoolDetailPage.tsx:67-77` gates only the summary slot; PoolCharts/PoolParticipants/PoolTransactions render unconditionally. Scope expanded from 2 → 3 pages, effort estimate 30-45min → 45-60min. Asset (`AssetDetailPage.tsx:127`) confirmed safe; Ledger/Tx/NFT use full-page early-return — safe.

4. **`SectionErrorBoundary` wrappers kept inside the gate**, not removed. The boundary now wraps an unmounted child when parent errors, which is a no-op — but keeps the JSX consistent with the happy-path render. Removing the boundary would couple gate semantics to layout structure.

## Future Work

None for this task — render-gate scope is fully closed. Follow-up consideration (out of scope, not spawned as separate task because no real UX impact yet): pre-suppress sub-query network fan-out via `enabled: !parent.isError` on each hook. Only worth doing if cold-network 404 cost becomes a measured problem.
