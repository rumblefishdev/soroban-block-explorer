---
id: '0262'
title: 'Composite NotFound — Account + Contract detail sub-section queries fire alongside parent 404 (dual error blocks)'
type: BUG
status: backlog
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
---

# Composite NotFound — Account + Contract detail dual error blocks

## Summary

Account and Contract detail pages fire sub-section queries (Transactions
tab on account; current tab on contract) in parallel with parent entity
query. When parent returns 404 for a **valid-format-but-not-found** ID
(e.g. `GAAA…AAA`, `CAAA…AAA`), the sub-section query also errors with
the same ID, producing **2 stacked error blocks** on one page instead
of a single clean NotFound.

Scope corrected after research vs initial finding claim:
- **Account** (E6): `<AccountTransactions/>` always mounts regardless
  of `account.isError` → 2 blocks
- **Contract** (E9): `<ContractDetailPage>` uses tabs; only one tab
  mounts at a time → max 2 blocks (summary + active tab)
- **Asset** (E8): **already gated** at `AssetDetailPage.tsx:127`
  (`!asset.isError && <AssetTransactions/>`) — single block, no fix needed
- **Liquidity Pool** (E13): early-returns at `isPoolId(value)` validator
  for garbage IDs → single NotFound state. Valid-strkey-but-404 would
  trigger sub-section, but pre-deploy this is unlikely; can re-test
  after 0264 strkey canonical lands.

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
<AccountTransactions accountId={accountId} />

// After (Approach B — render-level gating, matches AssetDetailPage.tsx:127)
{!account.isError && <AccountTransactions accountId={accountId} />}
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

### Step 3: Verify

- Navigate `/accounts/GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`
  (valid-format strkey but not in DB) → **single NotFound block**, no
  transactions error visible
- Navigate `/contracts/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`
  → single NotFound block, no tab strip rendered
- Navigate valid `/accounts/<real-G-strkey>` → all tabs load normally
  (no regression)
- Navigate valid `/contracts/<real-C-strkey>` → all tabs load normally
- Garbage paths (e.g. `/accounts/garbage`) still hit early-return
  validator → single NotFoundState (unchanged)

### Step 4: Regression test

Add Playwright assertion (gated on 0226 vitest infra): valid-format-404
scenario renders single error block on E6 + E9.

## Acceptance Criteria

- [ ] `AccountDetailPage.tsx` render-gates `<AccountTransactions/>` on
      `!account.isError`
- [ ] `ContractDetailPage.tsx` render-gates tab strip on `!contract.isError`
- [ ] Valid-format-404 IDs render single NotFound block on E6 and E9
- [ ] Valid IDs still load all tabs normally; no regression
- [ ] Garbage IDs still hit early-return validator (unchanged behavior)
- [ ] Audit branch `research/0257_frontend-comprehensive-audit` rebased onto develop post-merge
- [ ] Finding `F-D-2` in `D-state-coverage-matrix.md` marked `RESOLVED in <SHA>` (note: max 2 blocks, not 4 as originally claimed)
- [ ] Finding `F-AE-5` in `M-AE-console-error-handling.md` marked `RESOLVED in <SHA>`
- [ ] **Docs updated** — `N/A — bug fix, no architecture change`. Per ADR 0032.
- [ ] **API types regenerated** — `N/A — frontend-only`.

## Notes

- Effort: ~30-45 min (2 small JSX changes + verify).
- Asset detail (`AssetDetailPage.tsx:127`) already implements the pattern
  — use as reference.
- LP detail can be re-tested after 0264 strkey canonical lands (post-0264,
  valid-strkey-but-404 path becomes observable; if dual-block surfaces,
  extend this fix scope or spawn follow-up).
- Could batch with 0263 in same PR if implementer prefers (both are FE
  polish in pages/ subdirectory).
