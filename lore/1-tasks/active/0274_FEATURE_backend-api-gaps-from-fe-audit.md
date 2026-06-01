---
id: '0274'
title: 'Backend: API endpoints + fields surfaced by FE gaps audit'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0199', '0226']
tags:
  [
    priority-medium,
    effort-medium,
    layer-api,
    layer-backend,
    phase-pre-launch,
    milestone-2,
  ]
milestone: 2
links:
  - docs/audits/2026-05-29-frontend-api-gaps.md
  - tools/dev-mock-api.mjs
history:
  - date: '2026-05-29'
    status: backlog
    who: FilipDz
    note: >
      Spawned from the FE→API gaps audit
      (`docs/audits/2026-05-29-frontend-api-gaps.md`). FE built the
      Accounts page against an in-memory mock + worked around several
      missing fields on existing endpoints. This task tracks the
      backend work to close those gaps.
  - date: '2026-06-01'
    status: active
    who: karolkow
    note: >
      Activated alongside 0275 — taking both over as a pair. Prior
      WIP exists on `origin/feat/0274_backend-api-gaps-from-fe-audit`
      (FilipD, commit f0ff1a72): OpenAPI cleanups subset covering 3 of
      7 gaps (#3 ledgers `order` param, #6 `interface_metadata` schema
      + FE defensive-parser deletion, #7 `recent_events` on
      ContractStats). Remaining: #1 `GET /v1/accounts` (headline), #2
      lp_op_details amounts, #5 `PoolAssetLeg.icon_url`. Continuing on
      FilipD's branch (rebased on develop) rather than a fresh start to
      avoid duplicating his 3 gaps; branch to be renamed to span both
      0274 + 0275.
---

# Backend: API endpoints + fields surfaced by FE gaps audit

## Summary

Implement the backend pieces FE called out in the
[FE→API gaps audit](../../../docs/audits/2026-05-29-frontend-api-gaps.md):
one new list endpoint, one expanded list, and four schema /
contract additions. Closes the FE workarounds (in-memory account
mocks, hidden Amount column on pool tx, hand-rolled
`interface_metadata` parser, etc.).

## Context

The FE is in design-parity / pre-launch state. The audit catalogues
seven concrete gaps blocking either a feature surface or a quality
detail:

> **Progress (2026-06-01).** Gaps #3, #6, #7 are **DONE** on this branch
> (commit `c6bec5ee`, FilipD's WIP rebased) — backend + OpenAPI +
> regenerated `api-types` all in sync. Remaining: #1, #2, #5. #4 stays with
> task 0199.

1. **No `GET /v1/accounts` list endpoint** — Accounts page renders
   from 80 in-memory synthesized rows (`useAccountsList.ts`).
2. **Per-op LP amounts missing** on
   `GET /v1/liquidity-pools/{pool_id}/transactions` — the "Amount"
   column in the pool-tx table is intentionally hidden.
3. ✅ **DONE — `order` query param** on `GET /v1/ledgers` — was
   silently _ignored_ by the real backend (only the mock honoured it);
   now wired + declared in OpenAPI (`c6bec5ee`).
4. **Pool chart values always `null`** — endpoint contract is in
   the spec but `tvl` / `volume` / `fee_revenue` are `null` for
   every bucket until task **0199** (LP analytics + price oracle)
   ships. FE renders a placeholder card.
5. **`PoolAssetLeg.icon_url` missing** — pool avatars fall back to
   the first letter of the asset code instead of a real icon.
6. ✅ **DONE — `interface_metadata` typed schema** on
   `GET /v1/contracts/{contract_id}/interface` — typed DTO + OpenAPI
   schema; FE defensive parser deleted (`c6bec5ee`).
7. ✅ **DONE — real events count** (`recent_events`) on
   `ContractStats` — Events tab pill no longer borrows
   `recent_unique_callers` (`c6bec5ee`).

The audit doc has the full TypeScript shapes FE expects — it is now
the single reference for those shapes. (The runnable dev mock
`tools/dev-mock-api.mjs` was removed 2026-06-01, ahead of the AC
below — shapes live in the audit doc.)

## Implementation

Remaining gaps (#1, #2, #5) — each independent, can ship piecemeal:

- **`GET /v1/accounts` (list)** — Query: `limit`, `cursor`,
  `sort=xlm_desc|last_seen_desc|first_seen_desc`, `filter[q]`,
  `filter[with_domain]`. Response item:
  `{ account_id, xlm_balance, xlm_supply_percent, first_seen_ledger,
last_seen_ledger, home_domain, rank? }`. The `accounts` module already
  has detail + tx-list scaffolding to reuse (`crates/api/src/accounts/`).

  > **⚠ Resolve on paper before coding — schema can't fully back this shape:**
  >
  > - **`xlm_supply_percent` has no backing data.** No network-wide XLM total
  >   supply is stored (`assets.total_supply` is per-asset only). Mock fakes a
  >   constant. Decide: hardcode / `SUM(balance)` aggregate / drop for v1.
  > - **`xlm_balance` + `sort=xlm_desc` cross a table boundary** —
  >   balance is in `account_balances_current` (asset_type=0), not `accounts`.
  >   Cross-table keyset cursor + a new balance index needed.
  > - **`sort=first_seen_desc`** needs a new index (only `last_seen` exists).
  > - **`rank`** stable only for one sort mode, breaks under filter — design it.

- **`?expand=lp_op_details`** on pool transactions — opt-in field
  per row: `lp_operation_detail { operation_type, amount_a,
amount_b }`. Backend research tracked as **0247**; FE follow-up task
  TBD (the original "0249" cite was wrong — 0249 = archived AWS-teardown).
- **`icon_url` on `PoolAssetLeg`** — NOT a column copy: `PoolAssetLeg`
  carries only XDR `(code, issuer)`; `icon_url` lives on the `assets`
  row → LEFT JOIN per leg (2/pool). Design for the N+1 cost on the pool
  **list** endpoint.

Done in `c6bec5ee` (no further work): #3 `order` on `/v1/ledgers`,
#6 `interface_metadata` schema, #7 `recent_events` on `ContractStats`.
Pool chart fields (gap #4) are covered by **0199**.

## Acceptance Criteria

- [ ] `GET /v1/accounts` ships behind the documented contract;
      FE swaps `useAccountsList` from in-memory mock to the generated
      SDK hook.
- [ ] `?expand=lp_op_details` on pool transactions is wired and the
      "Amount" column on the LP tx table is un-hidden FE-side
      (or tracked via 0247/0249).
- [x] OpenAPI declares the `order` param on `/v1/ledgers`. (`c6bec5ee`)
- [ ] `PoolAssetLeg` carries `icon_url`; pool avatars render real
      icons when available.
- [x] `InterfaceResponse.interface_metadata` has a real schema in
      OpenAPI; FE deletes `parseInterfaceMetadata`'s defensive parse. (`c6bec5ee`)
- [x] `ContractStats` exposes a real events count; FE points the
      Events tab pill at the new field. (`c6bec5ee`)
- [x] `tools/dev-mock-api.mjs` removed (done 2026-06-01, ahead of
      sequence — shapes preserved in the audit doc). FE still needs to
      point `VITE_API_BASE_URL` at the real backend once #1 ships.

## Notes

- The audit doc is the single source of truth for FE-side expected
  shapes — keep it in sync if backend semantics diverge during
  implementation.
- Mock-server divergences (transactions missing `contract_ids`,
  NFTs ignoring `filter[name]`, search shape mismatch) are
  intentionally **out of scope** here — they're FE dev-mock bugs,
  not real-API blockers.
