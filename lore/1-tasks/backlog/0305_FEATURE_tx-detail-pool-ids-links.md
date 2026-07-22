---
id: '0305'
title: 'FEATURE: surface operation pool_ids (multi-hop crossed pools) in transaction detail UI'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0261', '0268', '0257', '0281']
tags: [priority-low, effort-small, frontend, milestone-2]
milestone: 2
links:
  - docs/runbooks/artifacts/e20_validation_20260618.md
history:
  - date: '2026-06-18'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from the 0281 window / 0268 work. The API already returns
      OperationItem.pool_ids (Array of SEP-23 L… strkeys) — the full
      crossed-pool list for path payments + LP ops — but no UI consumes it.
      Capability exists, frontend is agnostic; this surfaces it. Optional
      polish, not launch-blocking.
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Still valid — re-checked 2026-07-22, and it survived a false positive.**
      A grep for `pool_ids` in `web/src` hits three files, which looks like the
      feature exists. It does not: `operationEntries.ts:56` is a fallback
      `pool_ids: []` used when no matching light operation is found, and the other
      two hits are test fixtures. Nothing renders the field.
      The task's own sentence — "the frontend currently ignores this field
      entirely" — is still accurate. Recording this so the next sweep does not
      close it on the strength of the same grep.
---

# FEATURE: surface operation pool_ids in transaction detail UI

## Summary

The API's `OperationItem.pool_ids` (Array of SEP-23 `L…` strkeys) already
carries the full set of liquidity pools an operation crossed — single-element
for LP deposit/withdraw (types 22/23), the full multi-hop list for path
payments (types 2/13) and offers that filled against a pool (0261/0268). The
frontend currently ignores this field entirely. Render it in the transaction
detail view as links to the pool detail page.

## Context

- Spawned from the 0281 maintenance window (0268 scalar `pool_id` →
  `pool_ids Array` migration). E20 re-validation confirmed the attribution is
  correct on 200 anchors (`docs/runbooks/artifacts/e20_validation_20260618.md`).
- API contract: `OperationItem.pool_ids: string[]` is in the OpenAPI spec +
  `libs/api-types` generated types, returned by `GET /v1/transactions` and
  `GET /v1/transactions/{hash}`.
- Frontend today: the tx-detail operation components (`OperationPicker`,
  `toFlowNodes`, `OperationJsonDetail`) render `OperationItem` but never read
  `pool_ids`. The LP detail page is a separate surface and is OUT of scope —
  its endpoint (`/v1/liquidity-pools/:id/transactions`) does not return
  `pool_ids`.

## Implementation

- In the tx-detail operation rendering (`web/src/pages/transaction-detail/…`),
  read `OperationItem.pool_ids`.
- For each strkey, render a link/chip to the pool detail route
  (`/liquidity-pools/:id`) — the strkey is already in that route's id shape.
- Show on path payments (2/13), LP deposit/withdraw (22/23), and offers that
  filled against a pool. Empty array → render nothing (the common case).
- Multi-hop path payment: render all crossed pools (the array is the full,
  canonically-sorted list).
- Optionally surface pool hops in the flow diagram (`toFlowNodes.tsx`).

## Acceptance Criteria

- [ ] Tx-detail shows crossed pools as links for path payments + LP ops.
- [ ] Multi-hop path payment renders all crossed pools.
- [ ] Operations with empty `pool_ids` render unchanged (no empty UI).
- [ ] Pool links resolve to the existing LP detail page.
- [ ] **Docs updated** — N/A (frontend rendering only; no system-shape change).
- [ ] **API types regenerated** — N/A (already generated; read-only consumer).
