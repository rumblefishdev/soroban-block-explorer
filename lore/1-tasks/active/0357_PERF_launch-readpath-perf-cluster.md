---
id: '0357'
title: 'PERF: launch read-path perf cluster — scan→seek/projection across scan-bound endpoints (2026-07-06 load test)'
type: PERF
status: active
related_adr: []
related_tasks: ['0338', '0353', '0354', '0355', '0356', '0346']
tags: [priority-high, effort-large, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links:
  - crates/load-tests/README.md
history:
  - date: 2026-07-06
    status: active
    who: stkrolikiewicz
    note: >
      Spawned from the 2026-07-06 prod load test (re-run of the 0338 harness).
      Even at 10 VU / idle, heavy read endpoints show p95 = 3-9 s against the
      AC4 target of p95 < 200 ms. B2 join (client duration <-> system.query_log)
      confirmed server-side: ClickHouse scans 13-25M rows/request on the
      scan-bound endpoints. This cluster owns the untasked endpoints and
      coordinates the pre-existing 0353 / 0355 / 0356.
  - date: 2026-07-06
    status: active
    who: stkrolikiewicz
    note: >
      Round 1: nftdetail (#314) + asttxs (#315) merged — the two clean swaps.
      A table-size probe reframed the rest (see Findings): the 24M+ monsters are
      the accounts JOIN (22M) or liquidity_pool_snapshots FINAL (295M), NOT
      soroban_contracts (159k). Remaining = projections (acclist 0353, lplist/
      lpdetail/lpchart 0356-blocked, lptxs) + a shared-const refactor (astlist).
      Finding A (limit*128 under-delivery) logged as a shared lptxs+asttxs
      hardening follow-up.
---

# PERF: launch read-path perf cluster

## Summary

The 2026-07-06 prod load test (direct API-GW origin, warm, 10 VU) shows the
heavy read endpoints running 10-45x over the AC4 `p95 < 200 ms` target — at
idle, so it is baseline per-query cost, not contention. The B2 join proves the
cause is server-side: ClickHouse reads **13-25M rows/request** on the offenders
(the same whole-dimension `JOIN` / large-scan / `FINAL` anti-patterns that
0344/0345/0354 already removed elsewhere). This task is the umbrella for
finishing that job across the remaining endpoints, so AC4 can be met — or
knowingly reframed — before the mainnet launch.

## Context

- Evidence: `crates/load-tests/out/2026-07-06T12-10-00Z/results.csv` (B2-joined;
  `out/` is gitignored — the baseline table below is the durable record).
- Proven playbook: whole-dimension `JOIN accounts` / `soroban_contracts` →
  id-IN resolver (0344/0345/0354); `optimize_read_in_order` defeat (0353);
  `FINAL` over large ranges (0356).
- `txdetail` is explicitly OUT of scope here: its ClickHouse time is ~34 ms; the
  ~1 s total is non-CH overhead (Lambda / mTLS-per-query) — a separate trail.

## Worklist (ranked by read_rows, 2026-07-06)

| endpoint  | read_rows | state                                                                                                |
| --------- | --------- | ---------------------------------------------------------------------------------------------------- |
| nftdetail | 24.7M     | **DONE #314** — resolver swap, 24.7M→103k (235×)                                                     |
| asttxs    | 23.6M     | **DONE #315** — read-in-order driver, 338M→662k (512×)                                               |
| acclist   | 24.9M     | **known-issue** (0353) — projection rejected (CH 26.3 blocks RMT); FE-cached 60s, low-traffic browse |
| lplist    | 24.2M     | `liquidity_pool_snapshots` (295M) latest-per-pool → 0356-class, **blocked**                          |
| lpdetail  | 14.5M     | 0356 — snapshots FINAL, **blocked** (indexer before/after bug)                                       |
| lpchart   | 13.2M     | 0356 — snapshots FINAL, **blocked**                                                                  |
| lptxs     | 18.5M     | at the 0281-C read-in-order **floor**; further = pool_id projection                                  |
| astlist   | 1.99M     | own task — `assets FINAL` + lookup joins in the shared `ASSET_LIST_CH_SELECT` const                  |
| astdetail | 1.99M     | 0334 seek done — still 2M, partial                                                                   |
| search    | 1.0M      | untasked — 4 CH queries, unprofiled                                                                  |
| txlist    | 2.0M      | untasked — 0290/0333 archived, still ~900 ms                                                         |

(`ctrevents` sampled fine at 0.25M/52 ms, but 0353's worst case is mega-contracts
that random id sampling did not hit — keep 0353's fix.)

## Findings (2026-07-06) — table sizes reframed the plan

`system.tables` rows: `liquidity_pool_snapshots` **295M**, `accounts` **22.3M**,
`assets` 359k, `soroban_contracts` **159k** (tiny), `lp_positions` 109k,
`liquidity_pools` 76k. So the 24M+ "monsters" are the **`accounts` whole-JOIN (22M)**
or the **snapshots FINAL (295M)** — NOT `soroban_contracts`. This killed the initial
"swap the soroban_contracts join" hypothesis for lplist/astlist (that join reads 159k,
negligible).

- **nftdetail / asttxs** — done (accounts-join resolver swap; read-in-order driver).
- **acclist (0353)** — accounts (22M); the 0353 schema projection, not a query swap.
- **lplist / lpdetail / lpchart** — `liquidity_pool_snapshots` (295M) latest-per-pool;
  0356-class (snapshots FINAL + the non-deterministic before/after-image bug),
  **blocked**. A `latest-snapshot-per-pool` projection would unblock all three at once.
- **lptxs** — already at the 0281-C read-in-order floor; the residual is inherent to
  `has(pool_ids)` (array membership can't prefix-seek); needs a pool_id-keyed projection.
- **astlist** — `assets FINAL` + lookup joins in the shared `ASSET_LIST_CH_SELECT` const;
  a real refactor (touches the detail paths too), own task, `<200 ms` not guaranteed.

**Finding A (from #315 review) — shared hardening candidate:** the read-in-order driver
(`fetch_transactions` + `fetch_pool_transactions`) can under-deliver a page if `limit*128`
raw op-rows don't cover `limit` distinct → `finalize_page` reads a false last page
(lost-tail). Reachable only via transient re-ingest duplicates in `operations_appearances`
(RMT on `(ledger,tx,app_order)`; live ingestion is idempotent + abort-before-commit, so
dups come only from backfill-overlap and self-heal on merge). Follow-up: a bounded
`LIMIT 1 BY` fallback on capped under-delivery, applied to BOTH drivers.

## Implementation

Per endpoint, same playbook: find the whole-dimension read (a `JOIN` on a
surrogate, a scan the PK/sort key does not prune, or `FINAL` over a range),
replace with an id-IN resolver / seek / projection, and diff output
byte-identical vs prod (or a local range that contains the data).

Order (impact + dependency):

1. **nftdetail (0355)** — activated; effort-small, resolver swap already specced.
2. **lists** — lplist, astlist (+ acclist via 0353): also edge-cache (0346)
   candidates as a launch stopgap while the query fix lands.
3. **re-open the `[~]` "done" ones** — lptxs, asttxs, astdetail, txlist: confirm
   current read_rows and finish to target.
4. **search** — profile the 4 CH queries, seek the offender.
5. **lpdetail/lpchart (0356)** — blocked on the indexer snapshot fix first.

## Acceptance Criteria

- [ ] Every worklist endpoint either meets `p95 < 200 ms` at idle OR is a
      documented, edge-cached (0346) / known-issue with a post-launch commitment
- [ ] No whole-dimension reads remain on fixed endpoints (read_rows bounded to
      the working set, verified via `system.query_log`)
- [ ] Outputs byte-identical to pre-change (prod before/after or local range)
- [ ] Query-only where possible; any schema / projection / index change noted per endpoint
- [ ] D3 AC4 position restated with post-fix numbers (feeds the SCF claim)
- [ ] **Docs updated** — N/A unless a projection/index is added → then update the
      schema pages under `docs/architecture/**`
- [ ] **API types regenerated** — N/A (query-internal; no API surface change)
