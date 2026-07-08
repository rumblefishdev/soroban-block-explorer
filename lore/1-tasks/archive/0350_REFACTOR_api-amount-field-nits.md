---
id: '0350'
title: 'API contract nits: amount-field naming + fee decimals doc + LP share_percentage purity'
type: REFACTOR
status: completed
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
  - date: 2026-07-07
    status: active
    who: karolkow
    note: >
      Appended Nit 4 (from the 0244 PG-removal session): OpChRow +
      From<OpChRow> for OpRow in transactions/queries.rs is a redundant
      CH-shaped decode seam, exercised only by one unit test — the live
      operations path decodes OpRawRow → OpRow directly. NOT PG legacy;
      parked here as the thematically-closest home for a small API
      query-layer cleanup. Reopens the task (Nit 4 AC unchecked).
  - date: 2026-07-07
    status: completed
    who: karolkow
    note: >
      Nit 4 done — deleted OpChRow struct + From<OpChRow> for OpRow impl +
      its only consumer (op_row_uses_application_order_as_appearance_id test)
      from transactions/queries.rs. Pure dead-seam removal; live OpRawRow→OpRow
      read path untouched, no api-types/DTO impact. Nits 1–3 landed earlier in
      PR #312 (merged 2026-07-06). All ACs closed; archiving.
---

# API contract nits (optional — not bugs)

## Summary

Small API-contract clarity/cleanup items. None affect correctness or scaling; all
optional. Grouped so they can be picked up (or declined) together. (Nits 1–3 from
the amount-scaling review; Nit 4 a dead-decode-seam cleanup found in the 0244
PG-removal sweep.)

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

4. **`OpChRow` + `From<OpChRow> for OpRow` — redundant CH decode seam.**
   `crates/api/src/transactions/queries.rs:321` defines `OpChRow` with a
   `From<OpChRow> for OpRow` conversion (`:334`), but the **live** operations
   read path decodes `OpRawRow` (`:777`) → `OpRow` directly (`:898`). `OpChRow`
   is exercised only by one unit test (`:1123`) — a dead test-only seam, both
   structs CH-shaped (NOT PG legacy). **Fix:** delete `OpChRow`, its `From` impl,
   and the OpChRow-only round-trip test. Trivial; live path never touches it, no
   behavior change, no api-types impact (internal decode struct, not a DTO).
   (Found during the 0244 PG-removal dead-code sweep, 2026-07-07.)

## Acceptance Criteria

- [x] Nit 1 — `fee_charged` raw-stroops + 7-decimals contract documented on the field
      (transactions/accounts/assets/liquidity_pools DTOs; ledgers reuses `TransactionListItem`).
- [x] Nit 2 — **superseded: field REMOVED entirely, not renamed.** Big-picture recon showed
      the fold-count is doubly dead — FE never reads it AND on the live CH backend it is always
      `1` (fold has meaning only on the retired PG path). So it was a leaky storage detail on the
      API contract, redundant (endpoint already expands folds to per-event rows) and derivable.
      Removed from all four DTOs (`EventItem`, `InvocationItem`, `EventAppearanceItem`,
      `InvocationAppearanceItem`) + every now-dead internal read (PG row structs/SELECTs, CH
      intermediate rows/SELECTs, handler construction, CH test). Kept: `SUM(amount)` for
      `ContractStats.recent_events` (live, different use) + the DB column + indexer dual-write.
      api-types regenerated. FE untouched (never consumed it).
- [x] Nit 3 — **decision: keep `share_percentage` server-side.** It is a ratio
      (`shares * 100 / total_shares`), not amount-scaling, so it is an accepted server-side
      exception to the "FE derives" rule. Not worth the churn of returning raw shares + FE divide.
- [x] Nit 4 — `OpChRow` + `From<OpChRow> for OpRow` + the OpChRow-only round-trip test
      (`op_row_uses_application_order_as_appearance_id`) removed from `transactions/queries.rs`.
      Live `OpRawRow` → `OpRow` path unaffected; `operation_type_label` / `millis_to_utc`
      still used elsewhere in the file. No api-types change (internal decode struct, not a DTO).
- [x] **Docs updated** — `docs/architecture/**/14_get_contracts_events.sql` DTO-field ref
      updated (`EventItem.amount` → `.fold_count`). Schema docs describe the DB _column_ `amount`
      (unchanged) — accurate as-is. No FE-data-contract doc surfaces these fields.
- [x] **API types regenerated** — `npx nx run @rumblefish/api-types:generate` run; `openapi.json` + `generated/*` updated. Nits 1 & 3: doc/decision-only, no shape change.

## Notes

- All three verified against source (line refs above) on 2026-07-03. Optional —
  decline any that aren't worth the churn.
