---
id: '0275'
title: 'Contracts list page: design + implementation (no Figma source)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0226', '0274']
tags:
  [
    priority-medium,
    effort-medium,
    layer-frontend,
    layer-backend,
    phase-pre-launch,
    milestone-2,
  ]
milestone: 2
links:
  - web/src/router/index.tsx
  - web/src/pages/AccountsListPage.tsx
  - web/src/pages/accounts/AccountsTable.tsx
  - web/src/pages/accounts/AccountsFilters.tsx
history:
  - date: '2026-05-29'
    status: backlog
    who: FilipDz
    note: >
      Spawned alongside 0274 (FE→API gaps). Two list pages shipped
      without a Figma source: Accounts and Contracts. Accounts was
      spec and is live. Contracts is still a `PageStub` — needs
      columns / sort / filter decided, then implemented FE + BE.
  - date: '2026-06-01'
    status: active
    who: karolkow
    note: >
      Activated alongside 0274 — taking both over as a pair (0275's
      `GET /v1/contracts` mirrors 0274's `GET /v1/accounts`; FE
      Contracts list mirrors the Accounts list pattern). No prior work
      exists on 0275 — still a `PageStub`, no branch. Work continues on
      the shared 0274 branch (renamed to span both).
---

# Contracts list page: design + implementation (no Figma source)

## Summary

The Accounts list page and the Contracts list page both shipped
without a Figma design. Accounts was built from a text spec and is
live; Contracts is still a stub route. This task captures the design
discussion for Contracts (columns, sort, filter), then implements the
page end-to-end (FE + BE endpoint).

## Context

During design parity we walked every page against its Figma node —
two were the exception:

- **Accounts list** — built from a text spec
  (`# | Account | XLM Balance | % Supply | Last Seen | First Seen
| Domain`). Lives at
  [`web/src/pages/AccountsListPage.tsx`](../../../web/src/pages/AccountsListPage.tsx)
  - the supporting components in
    [`web/src/pages/accounts/`](../../../web/src/pages/accounts/).
    Currently mocked client-side (see 0274 — the `GET /v1/accounts`
    endpoint is the headline backend gap).
- **Contracts list** — still a `PageStub` wired at
  [`web/src/router/index.tsx`](../../../web/src/router/index.tsx).
  No design, no spec, no implementation, no backend endpoint.

The Accounts pattern (header → DataListCard → ExplorerTable →
cursor pagination) is the proven template for both. Contracts just
needs the column/sort/filter decisions, then the same plumbing.

## Implementation

### 1. Design discussion (open questions)

Decide on the following before writing code — capture the outcome
inline in this task (or spawn a `notes/Q-` if it gets long):

- **Columns** — likely candidates: contract id, type (SAC /
  Soroban / classic), deployer, deployed at ledger, recent
  invocations, last-seen ledger. Pick the 5–7 that read at a
  glance.
- **Sort modes** — e.g. recently deployed, most invocations,
  most unique callers.
- **Filters** — contract type (SAC vs Soroban), maybe a search by
  contract id substring, maybe deployer.

### 2. Frontend

Mirror the Accounts list pattern:

- `web/src/pages/ContractsListPage.tsx`
- `web/src/pages/contracts/ContractsTable.tsx`
- `web/src/pages/contracts/ContractsFilters.tsx`
- Hook in `web/src/api/hooks/useContractsList.ts` (start mocked
  client-side like `useAccountsList` if the backend endpoint
  lands later).
- Replace the `PageStub` at `/contracts` in the router.

### 3. Backend

Add `GET /v1/contracts` (list) — query params + response shape
driven by the design discussion above. Index per sort mode.
Update OpenAPI + regenerate `libs/api-types`.

Once the real endpoint lands, swap the FE hook from the local mock
to the generated SDK helper.

## Acceptance Criteria

- [ ] Columns / sort modes / filter set agreed and recorded in
      this task body.
- [ ] FE: `/contracts` renders a real list page (no `PageStub`)
      using the Accounts-list pattern (PageHeader + DataListCard + ExplorerTable + cursor pagination).
- [ ] BE: `GET /v1/contracts` ships with the agreed query params
      and response shape; OpenAPI updated; `libs/api-types`
      regenerated.
- [ ] FE points at the real backend endpoint (no in-memory
      synthesised rows left).
- [ ] Empty / error / filtered-empty states wired (mirror
      AccountsListPage).
- [ ] (Optional, deferrable) Figma frames for both Accounts and
      Contracts list pages — backfill the `// Figma:` comments in
      both FE families.

## Notes

- The 0274 task already lists the Accounts backend endpoint as a
  blocker; this task should NOT duplicate that — only add the
  Contracts backend endpoint. Cross-link if the two end up
  shipping together.
