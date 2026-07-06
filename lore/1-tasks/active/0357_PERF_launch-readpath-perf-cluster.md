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

| endpoint  | dur_p50 | ch_dur | read_rows | owner | state                                          |
| --------- | ------- | ------ | --------- | ----- | ---------------------------------------------- |
| nftdetail | 5395    | 4709   | 24.7M     | 0355  | active — start here (effort-small, known swap) |
| acclist   | 5262    | 4792   | 24.9M     | 0353  | backlog — schema projection                    |
| lplist    | 1984    | 1545   | 24.2M     | this  | untasked                                       |
| lptxs     | 2438    | 1730   | 18.5M     | this  | 0354 marked done `[~]` — reduced, off target   |
| lpdetail  | 1773    | 1842   | 14.5M     | 0356  | blocked (indexer before/after-image bug)       |
| lpchart   | 605     | 454    | 13.2M     | 0356  | blocked                                        |
| asttxs    | 2885    | 1741   | <=23.6M   | this  | 0354 `[~]` — reduced, off target               |
| astlist   | 1539    | 1544   | 1.99M     | this  | untasked                                       |
| astdetail | 1114    | 1044   | 1.99M     | this  | 0334 seek done — still 2M, partial             |
| search    | 930     | 595    | 1.0M      | this  | untasked                                       |
| txlist    | 910     | 260    | 2.0M      | this  | 0290/0333 archived — still ~900 ms             |

(ms; `ctrevents` sampled fine today at 0.25M/52 ms, but 0353's worst case is
mega-contracts that random id sampling did not hit — keep 0353's fix.)

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
