---
id: '0329'
title: 'Transaction detail shows only 1 of N operations (folded operations_appearances)'
type: BUG
status: completed
related_adr: []
related_tasks: []
tags: ['frontend', 'transaction-detail', 'effort-small']
links: []
history:
  - date: 2026-06-25
    status: backlog
    who: stkrolikiewicz
    note: 'Task created — bug found while exploring multi-operation transaction display.'
  - date: 2026-06-26
    status: active
    who: stkrolikiewicz
    note: 'Promoted to active — starting frontend fix.'
  - date: 2026-07-01
    status: completed
    who: stkrolikiewicz
    note: >
      Merged via PR #297 (merge 0f0a06fb) + archived. Frontend-only unfold:
      header count from `operation_count`, picker from `heavy.operations`
      (new `buildOperationEntries`), fallback to folded light. 3 files, +183/-11;
      typecheck + lint + 104 web tests green (incl. new operationEntries.test.ts).
      Verified live against prod API: airdrop tx c91a146e…fdd2 now renders
      "100 Operations" + 100 picker rows (#1…#100) — was "1 Operation". Steps 1-2
      of the plan shipped; Step 3 (heavy population) confirmed live on prod.
---

# Transaction detail shows only 1 of N operations (folded operations_appearances)

## Summary

The transaction detail view renders only the first operation and a "1 Operation"
header for transactions that actually contain several operations. Root cause: the
operation list **and** the count are built from `operations_appearances`, which
folds same-identity envelope operations into a single row (task 0163). The
unfolded list already ships in the same API response as `heavy.operations`
(XDR-decoded). Fix is frontend-only: drive the header from `operation_count` and
the picker list from `heavy.operations`.

## Status: Completed

## Context

- `operations_appearances` stores one row per distinct operation **identity**
  (`type` + source + asset + …, **not** amount / offer_id / price); the `amount`
  column is a **fold count**, not a token amount (task 0163). N same-identity ops
  collapse to 1 row.
- Confirmed in prod ClickHouse (ledger 63040312):
  - `143323a2…088d` — `operation_count = 4`, but **1** appearance row
    (type 12 `manage_buy_offer`, `fold_count = 4`).
  - `e8249a…2581` — `operation_count = 3`, but **1** appearance row
    (type 3 `manage_sell_offer`, `fold_count = 3`).
- Frontend [`OperationsSection.tsx:24`](../../../web/src/pages/transaction-detail/sections/OperationsSection.tsx) reads
  `tx.operations` (the folded "light" list) for both the picker and the count
  (`ops.length`) → renders "1 Operation" + one row.
- Backend: `fetch_operations` selects from `operations_appearances`
  ([`queries.rs:411`](../../../crates/api/src/transactions/queries.rs) /
  [`queries_ch.rs:761`](../../../crates/api/src/transactions/queries_ch.rs)) → light
  is folded. `heavy.operations` is built by `xdr_parser::extract_operations` over
  the whole envelope (unfolded, 1:1 with real ops) and is populated for **all**
  non-parse-error txs ([`handlers.rs:345`](../../../crates/api/src/transactions/handlers.rs)) —
  **not** gated to Soroban.
- Multi-op txs in our data are mostly bots (offer batches) and airdrops (e.g. one
  100-payment tx `c91a146e…fdd2`, memo "claim your SVX airdrop"). Pattern:
  `operation_count > 1` with `has_soroban = 0`.

## Implementation Plan

### Step 1 — header count from `operation_count` (bulletproof, frontend)

`OperationsSection.tsx`: header count reads `tx.operation_count`
([`dto.rs:117`](../../../crates/api/src/transactions/dto.rs), always present) instead of
`ops.length`. Fixes the misleading "1 Operation" regardless of heavy availability.

### Step 2 — picker list from `heavy.operations` (frontend)

Iterate `heavy.operations` (unfolded) when present; fall back to `light.operations`
when `heavy` is null. Each heavy op carries its own `details` (op_type, amount,
from/to), so the per-op detail panel reads `heavy[i].details` directly instead of
matching a folded light row.

### Step 3 — verify `heavy` is reliably populated in prod (precondition for Step 2)

`heavy` is a **runtime XDR fetch** from the stellar archive — envelope XDR is
**not** stored in ClickHouse (verified: no xdr/envelope column on `transactions`),
and the serving API is not on the CH box (Caddy there proxies straight to CH; the
public API is Turnstile-gated). So `heavy` can be null on archive miss/timeout.
Confirm on one real authenticated response (Network tab in Normal mode, or
authenticated curl) that `heavy.operations` has 4 items for `143323a2…`. If
`heavy` is frequently null in prod → escalate to a backend fix (reliable
enrichment, or return the unfolded op list).

## Acceptance Criteria

- [x] Detail header shows the true operation count (`operation_count`) — verified
      live: airdrop `c91a146e…fdd2` renders "100 Operations" (was "1 Operation").
- [x] Operation picker lists every operation — verified live: 100 picker rows
      (`#1…#100`) on `c91a146e…`; also confirmed 15/17/85-op batches in prod.
- [x] Graceful fallback when `heavy` is null — `buildOperationEntries` returns the
      folded light rows; count stays correct from `operation_count`. Covered by
      `operationEntries.test.ts`.
- [x] `heavy.operations` population in prod verified — Step 3 done: real prod-API
      response (dev-proxy) returned the full unfolded op list.
- [x] **Docs updated** — N/A (frontend rendering only; no schema/endpoint/pipeline
      change).
- [x] **API types regenerated** — N/A (no `crates/api/**` / `Cargo` / `api-types`
      change).

## Implementation Notes

Shipped in PR #297 (merge `0f0a06fb`), 3 files, +183 / -11:

- `web/.../sections/OperationsSection.tsx` — `count = tx.operation_count` (was
  `ops.length`); picker driven by `buildOperationEntries(tx).map(e => e.row)`;
  `selectedHeavyOp` now taken 1:1 from the entry (lore-0330's amount panel wiring
  preserved).
- `web/.../sections/operationEntries.ts` (new) — `buildOperationEntries()` maps
  each `heavy.operations` item to `{ row, light, heavy }`, matching the heavy op to
  its folded light identity (by `application_order`, then `type_name` fallback);
  returns folded light rows unchanged when `heavy` is empty.
- `web/.../sections/operationEntries.test.ts` (new) — unfold, fallback, and match
  cases (+102 lines).

Verified: `nx typecheck/lint/test @…-web` green (104 tests); live against prod API.

## Design Decisions

### From Plan

1. **Header count from `operation_count`, picker from `heavy.operations`** — exactly
   Steps 1-2 of the plan. Fallback to folded light preserves correctness when the
   read-time XDR fetch (ADR 0029) is unavailable.

### Emerged

2. **Ported only the core-unfold commit; dropped the amount commits.** The old
   `fix/0329` branch (5 commits) also added per-op amount rendering, but lore-0330
   shipped that independently (amount-sent) and merged first. Cherry-picking just
   the unfold (`a3b8ee30`) onto current develop avoided conflicts with 0330's
   `humanizeOp.ts` / `stroops.ts` rewrite; the amount work is now redundant.
3. **Unique picker keys via `application_order`.** Folded entries share one light
   `appearance_id`; the row key is overridden with `application_order` so
   same-identity heavy ops (e.g. 100 payments) get stable, unique React keys.
4. **Stale cross-worktree `.tsbuildinfo` gotcha** — the symlinked node_modules
   shares the nx/tsc cache; a stale `dist/*.tsbuildinfo` produced bogus TS6053
   "file not found" on first typecheck. Cleared it + `--skip-nx-cache`. (Noted for
   the `worktree-hooks` skill.)

## Notes

- **Bonus:** switching the picker to `heavy.operations` also surfaces
  per-operation amounts (`heavy.details.amount`) — the "show payment amount in
  the transaction detail" ask from the same exploration session.
- **Fold is intentional** (task 0163 storage optimization) — do **not** unfold the
  DB table; the per-op truth lives in the envelope XDR (`heavy`).
- Touch points:
  `web/src/pages/transaction-detail/sections/OperationsSection.tsx:24`;
  `crates/api/src/transactions/{handlers.rs:345, queries.rs:411, queries_ch.rs:761, dto.rs:117}`;
  `crates/xdr-parser/src/operation.rs:25`.
