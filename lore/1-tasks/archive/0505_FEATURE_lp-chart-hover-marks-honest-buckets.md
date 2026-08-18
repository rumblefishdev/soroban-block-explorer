---
id: '0505'
title: 'LP chart polish: hover-only marks + honest end-of-bucket TVL stamping'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0199', '0356']
tags: ['frontend', 'charts', 'effort-small']
links: []
history:
  - date: 2026-08-18
    status: active
    who: stkrolikiewicz
    note: 'Task created from 1Y chart review on pool HU/TGM (LCYIZIZD…VOSO)'
  - date: 2026-08-18
    status: active
    who: stkrolikiewicz
    note: >
      Implemented + verified in browser against prod API (dev proxy): marks
      hidden with hover dot intact, 1Y cliff moved Aug 10 → Aug 17. 278 web
      tests green (+4 new on toChartPoints). PR pending.
  - date: 2026-08-18
    status: completed
    who: stkrolikiewicz
    note: >
      All 6 acceptance criteria met (2 N/A by scope: no docs/architecture
      shape change, no API surface change). 4 files touched + 1 new test
      file, 4 new tests. Key decisions: bucket-end stamping on the frontend
      rather than a new API field; PERIOD_CONFIG as the single source for
      interval + bucket width. Archived with the implementation PR.
---

# LP chart polish: hover-only marks + honest end-of-bucket TVL stamping

## Summary

Two presentation fixes on the LP detail chart. (1) Static per-point markers
clutter the line at every density — hide them and keep only the hover
highlight dot (the `LineHighlightPlot` element that already tracks the
cursor). (2) The TVL step line stamps each bucket's `argMax` (end-of-bucket
state) at the bucket **start**, so with `stepAfter` a weekly drop renders up
to a week earlier than it happened — stamp TVL points at bucket end instead
(last point clamped to now).

## Status: Completed

**Current state:** Implemented, browser-verified against the prod API, and
shipped on `feat/0505_lp-chart-hover-marks-honest-buckets`.

## Context

Reviewed on pool `LCYIZIZD3SNJJQPYAQIWV23NQWEC6BVHUU6ZY6EGV5QPNG4Q726EVOSO`
(HU/TGM, alive ~2.5 weeks). Prod CH reproduction of the chart query at
`interval=1w` returns exactly 3 buckets:

| bucket (x-axis) | TVL     | actual argMax snapshot |
| --------------- | ------- | ---------------------- |
| Mon 2026-08-03  | $40,492 | Aug 9, 22:12           |
| Mon 2026-08-10  | $9,772  | Aug 16, 23:57          |
| Mon 2026-08-17  | $9,934  | Aug 18, 09:42          |

Data is correct per the endpoint contract (task 0199: weekly buckets,
`argMaxIf` TVL, `toMonday` stamp). The render is misleading: the value
measured Aug 16 is drawn as holding **from Aug 10**, so the gradual
Aug 12–16 decline (visible on 7D) becomes a cliff at Aug 10. Frontend-only
fix; no API change (returning the argMax snapshot timestamp was considered
and rejected — contract change + types regen not worth it).

## Implementation Plan

### Step 1: hover-only marks

`libs/ui/src/visualization/TimeSeriesChart.tsx`: `showMark: false`, drop the
now-dead `.MuiLineChart-mark` style block and the marks-at-every-density
comment (deliberate reversal of that earlier decision — product call from
this review). Hover dot + tooltip are `LineHighlightPlot`, unaffected.

### Step 2: end-of-bucket stamping for TVL

`web/src/pages/pool-detail/PoolCharts.tsx`: when mapping API rows for the
`tvl` metric, shift `timestamp = min(bucket + bucketWidth, now)`. Applies to
all periods uniformly (1h buckets shift by 1h — negligible but consistent).
Volume/fees bars keep bucket-start labels (a bar is "the week of Aug 10").
Fix the stale "density-gated" comment while there.

## Acceptance Criteria

- [x] Line charts show no static markers; hovering still shows the single
      nearest-point highlight dot + tooltip (verified on 1D: tooltip
      "Aug 18, 5:00 AM · $11.1K" with one dot)
- [x] TVL step transitions land at (or after) the time the underlying state
      was measured, never before — 1Y cliff on the HU/TGM pool moved from
      Aug 10 to Aug 17
- [x] Headline date reflects the clamped latest point (Aug 18 on 1Y)
- [x] `web` + `ui` typecheck/lint/tests green (278 web tests, +4 new)
- [x] **Docs updated** — N/A: presentation-only frontend change, no schema /
      endpoint / data-contract change
- [x] **API types regenerated** — N/A: no `crates/api` / `Cargo.*` /
      `libs/api-types` change

## Implementation Notes

Three files + one test file:

- `libs/ui/src/visualization/TimeSeriesChart.tsx` — `showMark: false`,
  `.MuiLineChart-mark` style block deleted.
- `web/src/api/hooks/usePoolChart.ts` — the per-preset switch became a
  `PERIOD_CONFIG` table that also carries `bucketMs`, exported as
  `periodBucketMs`. One table so query params and bucket-end stamping
  cannot drift apart.
- `web/src/pages/pool-detail/PoolCharts.tsx` — row mapping extracted to an
  exported `toChartPoints(rows, field, bucketEndShiftMs, nowMs)`; TVL
  passes the bucket width, flows pass 0.
- `web/src/pages/pool-detail/PoolCharts.test.tsx` — 4 cases: end-of-bucket
  stamp, clamp-to-now, flows keep bucket start, null rows dropped.

## Issues Encountered

- **Stale worktree `node_modules` symlink.** This worktree's `node_modules`
  was symlinked to the primary checkout, whose `api-types` predates
  `listPoolTransactionsOptions` — `web:typecheck` failed in files this task
  never touched. `nx reset` + dropping `dist/` did NOT fix it (the symlink
  is the cause, not the dist); a real `npm ci` inside the worktree did.
  Same family as the known stale-dist trap, different root cause.
- **Port 4200 was taken** by the primary checkout's dev server, so the
  preview ran on 4201 via a local (gitignored) `launch.json` entry, with
  `web/.env.development.local` copied from primary and flipped to the prod
  API proxy (its local-API-bin target on :9101 was not running).

## Design Decisions

### From Plan

1. **Hover-only marks via `showMark: false`.** The hover dot is
   `LineHighlightPlot`, a separate MUI element, so hiding series marks
   leaves it untouched — no custom hover implementation needed.
2. **Bucket-end stamping on the frontend, not the API.** The alternative
   (return the `argMax` snapshot timestamp) changes the response contract
   and forces an `api-types` regen for a presentation bug.

### Emerged

3. **`PERIOD_CONFIG` table replaced the `switch`.** The stamping needs the
   bucket width, which was implicit in the interval mapping. Two places
   deriving "how wide is a 1Y bucket" would rot; one table cannot.
4. **`toChartPoints` exported for testing** rather than tested through the
   rendered component — it is pure, and the arithmetic (not the DOM) is
   what can silently go wrong.
5. **Shift applies to every period, not just 1Y.** A 1h bucket moves by an
   hour — invisible but consistent, and it avoids a special case whose
   only justification would be "the bug was only visible at 1w".
6. **Flows deliberately keep bucket-start labels.** A bar is "the week of
   Aug 10"; shifting it would misplace the bar, not fix anything.

## Notes

Prod verification (2026-08-18, sorban-prod CH): chart SQL from
`crates/api/src/liquidity_pools/queries.rs::fetch_pool_chart` reproduced
with `argMaxIf(closed_at, …)` added to expose the real snapshot times — see
table above. Headline `$9,935` on 1Y vs `$10,464` on 7D is the daily-vs-
hourly price grain plus the excluded in-progress price bucket, by design
(task 0199).
