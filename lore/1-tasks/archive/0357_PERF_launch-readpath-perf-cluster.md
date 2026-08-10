---
id: '0357'
title: 'PERF: launch read-path perf cluster — scan→seek/projection across scan-bound endpoints (2026-07-06 load test)'
type: PERF
status: completed
related_adr: []
related_tasks:
  [
    '0338',
    '0353',
    '0354',
    '0355',
    '0356',
    '0346',
    '0386',
    '0400',
    '0401',
    '0402',
  ]
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
  - date: 2026-07-06
    status: active
    who: stkrolikiewicz
    note: >
      lptxs spike: broadly slow (6/6 pools 6–23M), bloom already prunes 99.85%,
      read-in-order is NOT a clean lever (OFF was faster on a spread pool) → no
      query-only win, pool_id-MV is the real post-launch fix. Restated the D3 AC4
      position (point-lookups meet target / lists = documented known-issue+MV /
      0356 blocked) — literal flat-200ms is unachievable by launch, so AC4 must be
      reframed with team + SCF before the M3 claim. This feeds the SCF submission.
  - date: 2026-07-13
    status: active
    who: stkrolikiewicz
    note: >
      search PROFILED (prod chq) → PARKED, no launch action. The 4 concurrent
      buckets are all bounded: tx-hash + pool-id are point seeks (`= unhex(?)`);
      account + contract are read-in-order prefix ranges; free-text contract name
      is a GROUP BY over `soroban_contract_metadata` = only **3734 rows** (the
      original "scan suspect" — cleared). Worst measured = account-prefix `'G'`
      (matches ~all 22M accounts): **317k read_rows / 15 ms** — NOT a scan or bloom
      miss, but a read-in-order merge across ~39 parts (leading granule each;
      `accounts` is a churny RMT, dedup 0.606 → many un-merged parts). 15 ms is well
      under AC4 and search had **0 load-test timeouts** (unlike lptxs/asttxs 20–27%),
      so the load-test "1.0M" = sum across buckets, explained not pathological. Only
      lever if ever needed: fewer `accounts` parts (OPTIMIZE / merge cadence), NOT a
      query or schema change. (acclist's `accounts_recent` MV does not help search —
      search needs `account_id` order, not `last_seen`.) Also: acclist moved from
      known-issue to a real fix — 0385 / PR #328 (`accounts_recent` refreshable MV).
  - date: 2026-07-14
    status: active
    who: karolkow
    note: >
      txlist row progressed. 0386 (merged/in-PR) removed the dead `contract_ids`
      whole-table `soroban_contracts FINAL` from the shared aggregate helper
      (210k → 8k read_rows/page). Prod chq then pinned the txlist residual on the
      SAME accounts churny-RMT many-parts seek as the search-prefix finding above:
      the source-account resolve reads ~785k/page (11 ids, 22M-row `accounts`).
      Spawned 0387 to own that lever (merge cadence / accounts_recent / id-dict) —
      it is the read-path monster after 0386, not the contract FINAL.
  - date: 2026-07-17
    status: active
    who: stkrolikiewicz
    note: >
      Re-ran the load test against an OPEN model (harness `--rps`, Poisson
      arrivals). The 0338 `--vus` driver is closed-loop, so its rate is an
      OUTPUT and it cannot express the AC4 "N req/month" target at all
      (same `--vus 4` = 9459 rps on a local stub, ~10 rps on prod). Two
      series x 3 tiers (1M / 10M / 50M req-month = 0.386 / 3.858 / 19.29
      rps), ~33k requests, `loadTesting: true`.
      MEASURED, not inferred: latency is FLAT across a 26x load range
      (p50 167 / 160 / 168 ms at 0.386 / 3.858 / 5 rps) -> zero contention.
      The 06-07 "slow at 10 VU" was never about load; lowering the rate to
      the literal 1M/mo target changes nothing.
      Series 1 found the box SATURATED at 50M/mo: lpchart 41.8bn + lpdetail
      12.8bn + lplist 6.4bn = 78% of ALL box read work -> unrelated endpoints
      degraded as COLLATERAL (netstats 90->150 ms), not on their own cost.
      Spawned #347.
      Series 2 (post #347 + prod minmax index): box read work 78.3bn ->
      38.4bn (-51%); saturation knee GONE (50M/mo now +6% median vs 10M/mo,
      was +68%); netstats back to its idle 93 ms AT 50M/mo.
      ATTRIBUTION, split deliberately (two different pieces of work):
      lpchart 77.9M->26.3M/req = -27.5bn = the ops-applied `closed_at_mm`
      minmax index (69% of the win); lpdetail 27.2M->1.5M/req = -11.9bn =
      #347 (30%). Reporting them merged would credit #347 with 2/3 of
      someone else's result.
      AC4 VERDICT: error rate PASSES decisively (0 errors / ~33k requests;
      95% CI upper 0.009% vs the <0.1% target). p95 FAILS: 558 ms vs 200 ms
      (~2.8x). Cause re-diagnosed and NO LONGER a read-path story — see the
      rewritten "D3 AC4 position" + worklist below.
  - date: 2026-07-17
    status: active
    who: stkrolikiewicz
    note: >
      Series 3 (post #349, lpchart upper-bound seek). Every prediction landed:
      box read work @50M 38.4bn -> 23.9bn (-38%, predicted ~24bn); lpchart
      26.3M -> 571k rows/req and CH 106 -> 68 ms; lpchart p95 1407 -> 221 ms.
      Tier C @50M: p95 1242 -> 1009 -> **576 ms** across the three series (-54%).
      THE HEADLINE, stated plainly: 50M/mo now costs the SAME as 10M/mo
      (p95 576 vs 568). A 5x traffic increase is free; this morning it cost
      +68% median. Saturation is gone entirely.
      THE HONEST COROLLARY: #347 + #349 did NOT move the AC4 number. Tier B
      (10M/mo) p95 went 585 -> 558 -> 568 across all three series — noise. Both
      PRs removed SATURATION, not per-request cost, so they only pay where
      saturation existed (50M/mo). The AC4-rate p95 has sat at ~560 ms all day,
      held by txdetail (overhead) + nftdetail (IPFS) + lplist + the 60-90 ms
      floor — none of which either PR touched. Today bought capacity and
      headroom, not the M3 number. Both are worth having; do not conflate them.
      lplist CONFIRMED as the last genuine query offender: 11.3M rows/req,
      CH 425 ms, unchanged by #347/#349. The 0208 Path 1 argument now has
      numbers behind it.
      NEW RISK FOUND (see "prices_writer" section): the CH box is SHARED with
      stellar-prices-api. Its OHLCV batch read 14.2bn rows during tier B and
      DOUBLED our p95 (1194 vs 568) at a rate we cannot saturate ourselves.
      Proven by re-run, not inferred: same code, same 3.858 rps, 33 min apart —
      contaminated run read 4.03bn of OUR rows at CH p50 104 ms; clean run read
      MORE (4.17bn) at CH p50 39 ms. Our queries did not change; the box was
      busy. Series 1 + 2 were verified clean (0.02bn), so every earlier
      conclusion in this task stands.
  - date: 2026-07-17
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. 6 of 7 ACs met; the 7th ("Docs updated") is deferred to 0400 with
      its reason recorded rather than rubber-stamped — the architecture docs
      still describe Postgres as production, so there is no truthful page to
      write `closed_at_mm` onto. Outcome: 26 endpoints measured on an open model
      (~33k requests, 3 series); 10/26 meet p95 < 200 ms at 10M/mo (was 5/26);
      box read work @50M/mo 78.3bn -> 23.9bn (-69%) and the saturation knee is
      gone — 50M/mo now costs the same p95 as 10M/mo. AC4 splits: error rate
      PASSES (0 / ~33k, CI upper 0.009% vs <0.1%), p95 FAILS (576 vs 200 ms) with
      each tail contributor's cause named. Landed #346/#347/#349 + the prod
      `closed_at_mm` minmax index (now in `init.sql`). Un-owned future work
      spawned rather than dropped: 0401 (lplist — the last genuine query
      offender, re-litigates 0208 Path 1) and 0402 (txdetail — the >=427 ms
      provably outside ClickHouse). 0400 already owned the schema-drift/docs
      class. Note: `related_tasks` previously listed 0387, which was deleted and
      renumbered (18ba218b); the dangling id is dropped here, the history prose
      above keeps the trail. The AC4 framing still needs team + SCF sign-off
      before the M3 claim — that is downstream of this task, not part of it.
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

## Worklist (REVISED 2026-07-17 — measured, open-model, post #347)

The 07-06 ranking below is kept for the audit trail, but it is superseded: it
ranked by `read_rows` on a closed-loop run, which conflated per-query cost,
collateral from a saturated box, and non-CH overhead. The open-model re-run
separates them. Current state, by share of the p95 tail at 10M/mo:

| endpoint                                                         | p95 (10M/mo) | tail share | category                  | what it actually is                                                                                                                                                                                                       |
| ---------------------------------------------------------------- | ------------ | ---------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `txdetail`                                                       | 1160 ms      | 34%        | **overhead, not CH**      | >=427 ms provably OUTSIDE ClickHouse (6 CH queries/req, each `max<=12 ms` -> `sum<=72 ms`). mTLS-per-query / Lambda. No SQL fix exists.                                                                                   |
| `lplist`                                                         | 618 ms       | 32%        | **CH — the only one**     | `created_at` = `min(ledger_sequence)`, ~9.6M rows/req. Fix = stored `created_at_ledger` (0208 Path 1, was rejected on writer/RMT grounds — the numbers now justify re-litigating).                                        |
| `nftdetail`                                                      | 1800 ms      | 26%        | **by design (ADR 0043)**  | request-time `token_uri()` Soroban RPC + IPFS, 3 s cap, LRU(1024). CH work identical fast vs slow (54k rows / ~20 ms both). Harness inflates it: 500 random NFTs never warm the LRU. Known-issue, not a fix.              |
| `lpdetail`                                                       | 330 ms       | —          | **DONE #347**             | 27.2M -> 1.5M rows/req (-94%), CH 784 -> 193 ms. Residual 1.5M is real snapshot work, not the `ledgers` hash. Lever exhausted.                                                                                            |
| `lpchart`                                                        | 197 ms       | —          | **DONE #349 + ops index** | 77.9M -> 26.3M (`closed_at_mm` minmax) -> **571k** (#349 bounds the upper seek both ways). CH 411 -> 106 -> 68 ms; p95 @50M 1407 -> 221. **Meets <200 ms.** Index was prod-only -> now in `init.sql`; recurrence -> 0400. |
| acclist / asttxs / lptxs / search / astlist / astdetail / txlist | all < 300 ms | —          | **DONE**                  | 0353 / 0364 / 0365 / 0370 / 0385 / 0386 landed. CH time now 19-52 ms each. Off the list.                                                                                                                                  |

**The floor (new, applies to everything):** ~60-90 ms per request before any query
runs — Lambda + mTLS-per-query + network. `netstats` does `<=32 ms` of CH work and
takes 90 ms; `ctriface` 43 ms CH / 103 ms total. **A third of the 200 ms AC4 budget
is spent before ClickHouse is asked anything**, and no query change touches it.

## Worklist (ranked by read_rows, 2026-07-06 — SUPERSEDED, kept for the trail)

| endpoint  | read_rows | state                                                                                                                                              |
| --------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| nftdetail | 24.7M     | **DONE #314** — resolver swap, 24.7M→103k (235×)                                                                                                   |
| asttxs    | 23.6M     | **DONE #315** — read-in-order driver, 338M→662k (512×)                                                                                             |
| acclist   | 24.9M     | **0385 / PR #328** — projection rejected (0353) → `last_seen`-ordered `accounts_recent` refreshable MV; supersedes the FE-cache known-issue        |
| lplist    | 24.2M     | `liquidity_pool_snapshots` (295M) latest-per-pool → 0356-class, **blocked**                                                                        |
| lpdetail  | 14.5M     | 0356 — snapshots FINAL, **blocked** (indexer before/after bug)                                                                                     |
| lpchart   | 13.2M     | 0356 — snapshots FINAL, **blocked**                                                                                                                |
| lptxs     | 18.5M     | broadly slow (6/6 sampled pools 6–23M); 0281-C floor, RIO not a clean lever; pool_id-MV = real post-launch                                         |
| astlist   | 1.99M     | own task — `assets FINAL` + lookup joins in the shared `ASSET_LIST_CH_SELECT` const                                                                |
| astdetail | 1.99M     | 0334 seek done — still 2M, partial                                                                                                                 |
| search    | 1.0M      | **profiled 2026-07-13 → PARKED** — 4 buckets bounded (worst: account-prefix `'G'` 317k/15ms, many-parts merge not a scan); 0 timeouts; see history |
| txlist    | 2.0M      | untasked — 0290/0333 archived, still ~900 ms                                                                                                       |

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

**lptxs spike (2026-07-06) — broadly slow; corrected two earlier assumptions.**
6/6 random pools read 6–23M (not a single outlier). EXPLAIN: `idx_oa_pool_ids`
already prunes 99.85% (1185/781406 granules) — the residual is the pool's real
granule footprint (spread across ~1000+ granules of history), not a bloom miss.
Surprise: `optimize_read_in_order = 0` read LESS + faster (4.67M/40 ms vs
7M/310 ms) for a spread pool → read-in-order is NOT a clean universal lever (it
early-terminates only for DENSE pools; can't be globally disabled or dense pools
explode). No safe query-only win. Isolated ~150–330 ms, but that read-volume ×
10-VU concurrency is the ~2.4 s from the load test → a broad, scalability-class
miss. The pool_id-keyed MV (arrayJoin `pool_ids` → `ORDER BY (pool_id, ledger,
tx)` → prefix-seek, ~tens of k) is a stronger post-launch candidate than first
thought, not YAGNI.

## `prices_writer` — the CH box is shared, and it costs us 2x p95 (2026-07-17)

Found by accident: series 3's tier B came out 2x worse than series 1 and 2, at a
rate we had twice proven we cannot saturate. Cause was not ours.

**`stellar-prices-api` (separate repo, CH user `prices_writer`) shares the box.**
Its OHLCV aggregation (`INSERT INTO prices.price_ohlcv_1h … SELECT
toStartOfInterval(…)`) read **14.2bn rows in 10 minutes** — 3x our entire load
test — and doubled our p95.

Proven by a controlled re-run 33 minutes later, not inferred:

| tier B, 3.858 rps, same code | `prices_writer` reads | OUR read_rows | our CH p50 | our p95     |
| ---------------------------- | --------------------- | ------------- | ---------- | ----------- |
| contaminated (11:14)         | **14.2bn**            | 4.03bn        | **104 ms** | **1194 ms** |
| clean re-run (11:47)         | 0.02bn                | **4.17bn**    | **39 ms**  | **568 ms**  |

The clean run read **more** of our rows and ClickHouse still took **2.7x less**
time. Our queries did not change; the box was busy with someone else's work.

Per-window audit (all of today) — series 1 and 2 were clean, so every conclusion
drawn from them stands:

| window     | `prices_writer` | verdict                        |
| ---------- | --------------- | ------------------------------ |
| B series 1 | 0.02bn          | clean                          |
| C series 1 | 0.02bn          | clean                          |
| B series 2 | 0.02bn          | clean                          |
| C series 2 | 2.19bn          | 5.4% of ours — negligible      |
| B series 3 | **14.2bn**      | **discarded, re-run**          |
| C series 3 | 1.11bn          | cleaner than C series 2 — kept |

It is **bursty**, not constant: 0 for minutes, then 1.64bn/min for ~4 minutes.
The query count is flat (537-636 per window) — the same jobs, occasionally doing
enormous reads.

**Why this matters beyond a spoiled run.** AC4 asks for a p95 the explorer cannot
unilaterally deliver: another team's batch schedule can double it at any moment,
on traffic 5x below our own saturation point. Options are isolation (CH workload
scheduling / a `prices_writer` quota or memory cap) or naming it in the AC4 report
as a known risk.

**DECIDED 2026-07-17: no task.** Deliberate, not an oversight — do not re-open it
as an open question. It is recorded here as a **known risk for the AC4 report**,
which is the cheaper of the two options: the isolation work (CH workload
scheduling / a `prices_writer` quota) buys nothing for the M3 claim, and the
contention is bursty and infrequent rather than a standing outage. Revisit only
if it starts landing on real user traffic rather than on our measurements. What
this section already gives a future reader is the full evidence, so the decision
can be reversed cheaply if that changes.

**Harness implication:** every future run needs a pre-flight check that the box is
quiet, or its numbers are a lottery:

```sql
SELECT toStartOfMinute(event_time) AS m, round(sum(read_rows)/1e9,2) AS prices_bn
FROM system.query_log
WHERE type='QueryFinish' AND user='prices_writer' AND event_time > now() - INTERVAL 5 MINUTE
GROUP BY m ORDER BY m DESC;   -- all zeros → safe to measure
```

## D3 AC4 position (2026-07-17) — MEASURED, supersedes the 07-06 position below

Two open-model series, 6 tiers, ~33k requests against prod. This replaces the
07-06 position: that one was argued from a closed-loop run and assumed the cause
was scan-bound queries. The measurement says otherwise on both counts.

### AC4 splits cleanly in two

**"error rate < 0.1%" — MET, decisively.** 0 errors in ~33,000 requests across
1M, 10M and 50M req/month. 95% CI upper bound (rule of three) = **0.009%**, an
order of magnitude under the target. No 429, no 5xx, no shed — at any tier.

**"p95 < 200 ms" — NOT met: 558 ms (~2.8x).** But the cause is not what D3
currently claims, and the difference matters for the SCF conversation:

| what                          | contribution                         | is it fixable?                                     |
| ----------------------------- | ------------------------------------ | -------------------------------------------------- |
| Lambda + mTLS + network floor | ~60-90 ms on EVERY request           | not by query work; needs connection/arch change    |
| `txdetail` (34% of tail)      | >=427 ms outside CH, 6 queries/req   | connection batching, not SQL                       |
| `nftdetail` (26% of tail)     | request-time IPFS/RPC, 3 s cap       | deliberate ADR 0043 trade-off; harness inflates it |
| `lplist` (32% of tail)        | ~9.6M rows on `min(ledger_sequence)` | yes — 0208 Path 1                                  |

**Only one of the four is a slow query.** The D3 doc's framing ("heavy read
endpoints run 10-45x over target") is now obsolete: every endpoint that framing
named has been fixed and sits at 19-52 ms of CH time.

### Capacity is a non-issue — and worth saying out loud

- Latency is **flat across a 26x load range** (p50 167/160/168 ms). Contention
  does not exist below ~4 rps; the AC4 target rate (1M/mo = **0.386 rps**) is
  ~1 request every 2.6 s.
- **50M req/month sustained with zero errors** — 50x the AC target. Post #347 the
  saturation knee is gone: 50M/mo costs +6% median vs 10M/mo (was +68%).
- The API Gateway throttle (50 rps) is itself a ~130M req/month ceiling.

### Recommended framing (needs team + SCF sign-off BEFORE the M3 claim)

Report AC4 honestly and per-endpoint, with the cause named for each:
error rate met with a 10x margin; capacity proven to 50x the target with zero
errors; p95 at 558 ms with a **named, non-speculative** breakdown — ~60-90 ms is
architectural floor, one endpoint is a documented external-dependency trade-off
(ADR 0043), one needs connection work, one has a known query fix (0208 Path 1).

This is materially stronger than the 07-06 position. "Our p95 is dominated by
platform overhead and one deliberate freshness trade-off, and we sustain 50x the
required load with zero errors" is a different claim from "our queries are slow" —
and unlike the old framing, every number in it was measured today.

Risk unchanged: if SCF insists on a literal flat 200 ms across all endpoints, that
is not reachable without removing the mTLS-per-query floor and re-opening ADR 0043.
Confirm the framing early.

## D3 AC4 position (2026-07-06) — SUPERSEDED by the section above, kept for the trail

The literal AC4 ("p95 < 200 ms" flat across all endpoints) is **unachievable by
launch** and unrealistic for analytical/list endpoints on single-node ClickHouse.
Per-endpoint reality, from the load test + the acclist/lptxs spikes:

- **Point-lookups — meet / near target.** nftdetail (#314), account / tx detail.
  Single-row PK seeks; CH cost is ms, total ≈ the non-CH overhead.
- **Lists / tx-lists — over target; documented known-issue + post-launch fix.**
  asttxs fixed (#315); acclist = FE-cached 60 s, low-traffic, cosmetic freshness
  (projection rejected, CH 26.3 blocks RMT projections); lptxs = broad 6–23M,
  pool_id-MV post-launch; astlist = shared-const refactor.
- **Snapshot endpoints — BLOCKED.** lplist / lpdetail / lpchart on 0356 (indexer
  before/after-image fix), a separate task; not fixable by launch.

**Recommended framing (needs team + SCF sign-off BEFORE the M3 claim):** the AC4
load-test report states honest per-endpoint p95 — point-lookups meet < 200 ms; the
list / analytical endpoints are documented known-issues, each with a named cause and
a post-launch commitment (MVs, the 0356 indexer fix). Defensible (no explorer hits a
flat 200 ms on every endpoint) and effectively required (a flat target is
unachievable by launch: 0356 blocked, several endpoints need schema/MV work). Risk:
if SCF insists on a literal flat 200 ms, M3 slips weeks (MVs + unblocking 0356) —
confirm the framing with SCF early.

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
4. **search** — DONE (2026-07-13): profiled, all 4 buckets bounded, PARKED (see worklist row + history).
5. **lpdetail/lpchart (0356)** — blocked on the indexer snapshot fix first.

## Acceptance Criteria

- [x] Every worklist endpoint either meets `p95 < 200 ms` at idle OR is a
      documented known-issue with a named cause — see the revised worklist.
      10/26 meet it at 10M/mo (was 5/26); the 3 that dominate the tail each have
      a named category (overhead / ADR 0043 trade-off / 0208 Path 1).
- [x] No whole-dimension reads remain on fixed endpoints — verified via
      `system.query_log` per request (B2 join) on ~33k requests: the 07-06
      offenders now read 19-52 ms of CH time each. The last whole-dimension read
      (`lpdetail`'s 26M-row `ledgers` hash) died with #347: 27.2M → 1.5M rows/req.
- [x] Outputs byte-identical to pre-change — carried by #346/#347's own diffing;
      this task's contribution is measurement, not query changes.
- [x] Query-only where possible; any schema / index change noted per endpoint —
      one index (`closed_at_mm` minmax on `ledgers`, applied to prod by online
      ALTER during the #347 work). It was **prod-only**; now added to `init.sql`.
      The recurring class → **0400**.
- [x] D3 AC4 position restated with MEASURED numbers (feeds the SCF claim) — see
      the 2026-07-17 section; needs team + SCF sign-off before the M3 claim
- [ ] **Docs updated** — an index WAS added, so this gate fires. **Cannot be met
      honestly today**: `docs/architecture/database-schema/**` still describes
      Postgres as production and ClickHouse as a "read-empty pilot", so there is
      no truthful page to write `closed_at_mm` onto. Deferred to **0400**, which
      owns retiring the Postgres-era pages. Flagged rather than rubber-stamped.
- [x] **API types regenerated** — N/A (no `crates/api/**`, `Cargo.*` or
      `libs/api-types/**` change; the harness is a standalone test crate)

## Future Work

- **0400** — prod-only CH schema objects absent from `init.sql` (`closed_at_mm`
  found here, `oa_pool_seek` still open) + the stale architecture docs that let
  it through. Spawned from this task's measurement.
- **0401** — `lplist`, the last genuine query offender (~9.6M rows on
  `min(ledger_sequence)`). Needs 0208 Path 1 (stored `created_at_ledger`)
  re-litigated; it was rejected on writer/RMT grounds before these numbers existed.
- **0402** — `txdetail`, 6 CH queries/request, ≥427 ms provably outside
  ClickHouse. A connection/batching investigation, NOT a query task.
- **Harness realism** — uniform id sampling defeats `nftdetail`'s LRU(1024) by
  construction (500 random NFTs, ~95 requests → almost every lookup is a first
  hit), so its measured p95 is worst-case, not typical. A Zipf id distribution
  would report what real traffic sees. Deliberately not done: uniform is the
  conservative choice and needs no invented assumptions.
