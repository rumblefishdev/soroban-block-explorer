# D3 / AC4 — load-test evidence (measured 2026-07-17)

Context for the Milestone 3 evidence write-up. **Every number here was measured
on production on 2026-07-17**, not modelled. Full trail + per-endpoint detail:
[task 0357](../1-tasks/active/0357_PERF_launch-readpath-perf-cluster.md).

AC4 as written: _"p95 < 200 ms at 1M req/month equivalent; error rate < 0.1%"_.

## The verdict, in one table

| AC4 half          | Result                                                    | Status               |
| ----------------- | --------------------------------------------------------- | -------------------- |
| error rate < 0.1% | **0 errors in ~33,000 requests**; 95% CI upper **0.009%** | **MET**, ~10x margin |
| p95 < 200 ms      | **558 ms** at the 1M/mo rate                              | **NOT met**, ~2.8x   |

No 429, no 5xx, no shed — at any tier, including 50x the required load.

## What "1M req/month" actually is

`1M / 2,592,000 s` = **0.386 req/s** — about one request every 2.6 seconds. The
AC's headline rate is trivially small; the interesting question was never
capacity.

| tier | req/month | rps   | measured p95 |
| ---- | --------- | ----- | ------------ |
| A    | 1M        | 0.386 | 558 ms\*     |
| B    | 10M       | 3.858 | 568 ms       |
| C    | 50M       | 19.29 | 576 ms       |

\* Tier A's own p95 is statistically meaningless (n≈120 → p95 is the 6th sample
from the top; it swung 593→661 between two runs of identical code, on luck
alone). Use tier B as the estimator: latency is **flat across a 26x load range**
(p50 167/160/168 ms), proven independently three times, so B's large-n p95 is
valid at A's rate. Same reason error rate is quoted on the pooled ~33k: at
0.386 rps you would need **2.2 hours** to gather the n≥3000 the rule of three
needs to demonstrate <0.1% at all.

## Why p95 misses — the part that matters for the claim

The D3 doc's old framing ("heavy read endpoints run 10-45x over target") is
**obsolete**. Every endpoint it named has been fixed and now does 19-52 ms of
ClickHouse work. The 558 ms breaks down as:

| cause                                                                          | share of the p95 tail | is it a slow query?                     |
| ------------------------------------------------------------------------------ | --------------------- | --------------------------------------- |
| `txdetail` — ≥427 ms **outside** ClickHouse (6 CH queries/req, mTLS-per-query) | 34%                   | No — connection/batching                |
| `lplist` — `min(ledger_sequence)`, ~11.3M rows/req, CH 425 ms                  | 32%                   | **Yes** — the only one                  |
| `nftdetail` — request-time `token_uri()` RPC + IPFS, 3 s cap                   | 26%                   | No — deliberate, [ADR 0043](../2-adrs/) |
| Lambda + mTLS + network floor — **~60-90 ms on every request**                 | baseline              | No — architectural                      |

**One of the four is a slow query.** The floor alone spends a third of the
200 ms budget before ClickHouse is asked anything: `netstats` does ≤32 ms of CH
work and takes 90 ms.

`nftdetail` is additionally **inflated by the harness**: it samples 500 NFTs
uniformly, so an LRU(1024) that real hot-key traffic would hit never warms. Its
p95 is worst-case, not typical. Left uncorrected on purpose — uniform sampling
is the conservative choice and needs no invented traffic assumptions.

## Capacity — worth stating, it is strong

- **50M req/month sustained, zero errors** — 50x the AC target.
- Post-#347/#349 the saturation knee is **gone**: 50M/mo costs the same p95 as
  10M/mo (576 vs 568). That morning it cost +68% median.
- The API Gateway throttle (50 rps) is itself a ~130M req/month ceiling.
- Box read work at 50M/mo fell 78.3bn → 23.9bn rows (−69%) over the day.

## Recommended framing (needs team + SCF sign-off BEFORE the M3 claim)

Report AC4 per-endpoint and honestly, with the cause named for each: error rate
met with a 10x margin; capacity proven to 50x the target with zero errors; p95
at 558 ms with a **named, non-speculative** breakdown — architectural floor, one
documented external-dependency trade-off, one connection-layer issue, one known
query fix.

_"Our p95 is dominated by platform overhead and one deliberate freshness
trade-off, and we sustain 50x the required load with zero errors"_ is a
materially different claim from _"our queries are slow"_ — and unlike the old
framing, every number in it was measured.

**Risk:** if SCF insists on a literal flat 200 ms across all endpoints, that is
not reachable without removing the mTLS-per-query floor and re-opening ADR 0043.
Confirm the framing early.

## Caveats an evidence reviewer should know

- **The ClickHouse box is shared with `stellar-prices-api`.** Its OHLCV batch
  can read 14.2bn rows in 10 minutes and **double the API's p95** on traffic 5x
  below our own saturation point. Proven by controlled re-run: same code, same
  rate, 33 min apart — the clean run read _more_ of our rows in 2.7x less CH
  time. Decided as a known risk, no isolation task (see 0357). It means the
  explorer's p95 is not unilaterally ours to deliver.
- All quoted runs were **audited clean** of that contention (`prices_writer`
  ≤0.02bn in-window); the one contaminated run was discarded and re-run.
- Numbers come from an **open-model** driver (Poisson arrivals, rate as input).
  The older `--vus` closed-loop harness cannot express a req/month target and
  suffers coordinated omission — do not compare these to pre-2026-07-17 figures.
- Raw per-request CSVs are gitignored by design; 0357 is the durable record.
