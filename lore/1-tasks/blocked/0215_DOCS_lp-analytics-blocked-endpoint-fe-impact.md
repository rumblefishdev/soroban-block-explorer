---
id: '0215'
title: 'Doc: LP analytics endpoints blocked-on-oracle — frontend impact catalog'
type: DOCS
status: blocked
by: ['0199']
related_adr: ['0043']
related_tasks: ['0199', '0207']
tags: [layer-docs, layer-frontend, audit-2026-05-12, priority-low, effort-small]
milestone: 2
links:
  - docs/audits/2026-05-12-ch-pilot-endpoint-audit.md
history:
  - date: '2026-05-12'
    status: blocked
    who: stkrolikiewicz
    by: ['0199']
    note: >
      Spawned from CH pilot endpoint audit §E21. Documents which
      endpoints + frontend views are functionally blocked until task
      0199 (LP analytics, blocked-on-oracle = Oskar's price API).
      Status `blocked` not `backlog` because the doc itself can only
      meaningfully be written once 0199's scope is locked. For now,
      this task carries the FE-impact info so it doesn't get lost.
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **The situation it documents is ending — 2026-07-22.**
      This catalogues which endpoints and frontend views show empty values because
      LP analytics are blocked on the price oracle. 0199 was unblocked today: the
      `prices` database is live on the cluster (37 tables, 593.6M rows, 122,706
      assets, history from 2024-02-20) and 39,370 of 52,288 pools (75.3%) have
      both legs priceable through the `price_usd_series` view.
      So the "blocked-on-oracle" framing is obsolete. What may still be worth
      writing is the inverse: which views change once TVL starts returning values,
      and how to present the ~1.5-day price staleness. Re-scope or close with
      0199; do not publish the catalogue as-is.
---

# LP analytics endpoints — blocked-on-oracle, frontend impact

## Summary

Three endpoints' display fields are currently NULL because task 0199
(LP analytics) is blocked on the team-built price oracle (Oskar). This
task catalogues which endpoints + which frontend views are affected,
so design / sprint planning can either swap them out, gray them, or
defer the dependent UI work.

## Blocked endpoints (per ADR 0043 — Lambda 2 owns USD-denominated fields)

| Endpoint                                 | NULL field(s)                                         | Frontend impact                                                                                                    |
| ---------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| E18 `/liquidity-pools` (list)            | `tvl` (table column)                                  | §6.13 list — "TVL" column shows `—` / "N/A". Sort-by-TVL disabled.                                                 |
| E19 `/liquidity-pools/:id` (detail)      | `tvl`, `volume`, `fee_revenue` (latest snapshot card) | §6.14 detail — top-card metrics empty. Show placeholder ("data not yet available") instead of zero.                |
| E21 `/liquidity-pools/:id/chart` (chart) | `tvl`, `volume`, `fee_revenue` (entire series)        | §6.14 detail — chart widget renders empty. Display "Chart data not yet available" overlay; do not show empty axes. |

## What works (no oracle dependency)

- E18/E19: pool identity (asset_a/b, fee_bps, total_shares, reserve_a, reserve_b) is **populated correctly** from on-chain data. Front-end can render these columns immediately.
- E21: `bucket` time-series structure works (`toStartOfInterval` returns real buckets). Only the metric series are NULL. Frontend can use `samples_in_bucket` as an "activity heat" proxy if desired.
- E20 `/liquidity-pools/:id/transactions`, E23 `/liquidity-pools/:id/participants` — fully populated, no oracle dependency.

## Unblock trigger

When task 0199 ships (price oracle + Lambda 2 LP analytics writer):

1. CH `liquidity_pool_snapshots.{tvl, volume, fee_revenue}` populate.
2. Update audit doc §E21 marking resolved.
3. Remove FE placeholder overlays.
4. Re-enable sort-by-TVL on E18 list.

## Acceptance Criteria

- [ ] This doc reviewed by FE lead — confirm view list (§6.13, §6.14) maps to current sprint plan.
- [ ] Sprint backlog tagged: any LP-detail UI work scheduled BEFORE 0199 ships must explicitly note "uses placeholder for tvl/volume/fee_revenue".
- [ ] On 0199 ship: revisit this doc, archive with `superseded_by: ['0199']`.

## Notes

- E21 bucketing logic itself is correct (verified in audit). Only the metric values are NULL.
- Display recommendation per row above; avoid silent NULL → 0 coercion (false data).
- Audit doc reference: `docs/audits/2026-05-12-ch-pilot-endpoint-audit.md` §State-NULL gaps + §E21.
