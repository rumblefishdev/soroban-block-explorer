---
id: '0350'
title: 'API contract nits: amount-field naming + fee decimals doc + LP share_percentage purity'
type: REFACTOR
status: active
related_adr: []
related_tasks: []
tags: [api, api-types, clarity, priority-low, effort-small, optional]
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: karolkow
    note: >
      Three optional API-contract nits found during the amount-scaling review.
      Not bugs — clarity/purity/doc only. Refs verified against source.
  - date: 2026-07-03
    status: active
    who: karolkow
    note: Promoted to active to begin work.
---

# API contract nits (optional — not bugs)

## Summary

Three small API-contract clarity items. None affect correctness or scaling; all
optional. Grouped so they can be picked up (or declined) together.

## Nits

1. **`fee_charged` returns raw stroops with no `decimals` field.**
   `transactions` / `ledgers` / `assets` / `accounts` surface `fee_charged` as
   raw stroops. Fine in practice — native is always 7 decimals and the frontend
   `formatFee` hardcodes `/1e7` (`web/src/pages/transactions/formatters.test.ts`:
   `formatFee(100) → '0.00001 XLM'`). **Doc-only gap** — the raw-stroops +
   implicit-7-decimals contract isn't documented on the field. Fix = a doc/
   schema comment, not a shape change.

2. **`amount` fields on event/invocation appearances are fold COUNTS, not money.**
   `crates/api/src/contracts/dto.rs:130` (`amount: i32`) and `:143`
   (`amount: i64`) are appearance **fold/expansion counts** (one appearance row
   with `amount > 1` expands to N events), not monetary amounts. The name
   `amount` misleads (implies money → invites scaling). **Rename to `*_count`**
   (e.g. `event_count` / `fold_count`) for clarity. No scaling implication; a
   rename touches the DTO + `libs/api-types` regen + FE readers.

3. **LP `share_percentage` computed server-side.**
   `crates/api/src/liquidity_pools/queries_ch.rs:357`:
   `toString(lpp.shares * 100 / snap.ts) AS share_percentage`. It's a **ratio**,
   not amount-scaling, so acceptable server-side — but it IS backend division if
   the team wants strict "no math in the backend, frontend derives" purity.
   Optional: move the ratio to the frontend (return `shares` + `total_shares`,
   let FE divide), or leave as-is and accept the ratio exception.

## Acceptance Criteria

- [ ] Nit 1 — `fee_charged` raw-stroops + 7-decimals contract documented on the field
- [ ] Nit 2 — appearance `amount` fields renamed to `*_count` (DTO + api-types regen + FE)
- [ ] Nit 3 — decision recorded: move `share_percentage` to FE, or keep as an accepted server-side ratio
- [ ] **Docs updated** — if nit 2's rename changes the API shape, update `docs/architecture/**` frontend-data-contract refs.
- [ ] **API types regenerated** — nit 2 changes `crates/api/**` + `libs/api-types/**` → run `npx nx run @rumblefish/api-types:generate`. Nits 1 & 3 as scoped: N/A / doc-only.

## Notes

- All three verified against source (line refs above) on 2026-07-03. Optional —
  decline any that aren't worth the churn.
