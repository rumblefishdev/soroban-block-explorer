---
id: '0446'
title: 'PERF: collapse sequential ClickHouse round trips across the API query layer'
type: PERF
status: active
related_adr: []
related_tasks: ['0338', '0359']
tags:
  [phase-future, effort-medium, priority-medium, performance, api, clickhouse]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Spawned from the Milestone 3 AC4 latency analysis. Independent queries are
      awaited one at a time in 19 loops across 7 query modules, while 9 sites in
      the same crate already use `tokio::join!`. Measured cost of an avoidable
      round trip is 14.2 ms.
  - date: '2026-08-12'
    status: active
    who: karolkow
    note: >
      Activated. Re-checked against the code first: both named sites are
      unchanged — the two independent keyset arms in `assets::fetch_transactions`
      still run in a sequential loop, and `liquidity_pools` still has no
      concurrent site at all (its aggregate and account lookups are awaited one
      after the other though neither consumes the other's result). First slice is
      the assets keyset arms.
---

# Collapse sequential ClickHouse round trips across the API query layer

## Summary

The API issues an average of **3.24 ClickHouse queries per request** (median 3,
max 8). Each additional query costs a measured **14.2 ms** of wall clock, because
ClickHouse sits on Hetzner rather than inside the AWS network. A number of those
queries are independent of one another and are nevertheless awaited one at a
time.

Sweep the query layer: every set of independent queries should either run
concurrently (`tokio::join!` / `try_join_all`) or, where they share a result
shape, collapse into a single statement.

## Context

Derived from the 2026-07-17 load-test data (`docs/scf/load-tests/`, 16,232
requests joined to the ClickHouse query log per request).

Fitting non-database time against query count across 13,701 samples gives:

```
overhead ≈ 46.5 ms + 14.2 ms × (number of ClickHouse queries)
```

The 14.2 ms slope is the round trip itself — `ch_duration_ms` comes from
`system.query_log`, so it is already excluded.

**The codebase contains both patterns, two statements apart.** In
`crates/api/src/assets/queries.rs`, `fetch_transactions` runs its two keyset arms
in a sequential loop:

```rust
for sql in &sqls {
    keys.extend(client.query(sql).fetch_all::<AssetTxKeyChRow>().await?...);
}
```

and then runs the page query and the aggregate concurrently:

```rust
let (page_rows, aggregates) = tokio::join!(
    client.query(&page_sql).fetch_all::<AssetTxPageChRow>(),
    ch::fetch_tx_list_aggregates(client, &keys, false),
);
```

The two arms are independent by construction — arm A reads
`operation_asset_appearances`, arm B reads `soroban_invocations_appearances`, and
neither consumes the other's output. The merge, dedup and truncate that follow
are done in Rust over the two separately fetched result sets; ClickHouse could do
that itself with `UNION ALL` and return one page.

Repository-wide count of loops containing an `.await`, against concurrent sites:

| Module            | loops with await | `tokio::join!` |
| ----------------- | ---------------: | -------------: |
| `search`          |            **7** |              0 |
| `assets`          |                3 |              4 |
| `liquidity_pools` |                3 |              0 |
| `nfts`            |                2 |              0 |
| `transactions`    |                2 |    3 (handler) |
| `accounts`        |                1 |              1 |
| `contracts`       |                1 |              0 |
| **total**         |           **19** |          **9** |

That count is a starting point, not a work list — not every loop with an `.await`
is a query loop, and some are genuinely dependent. Each needs reading.

## Implementation

- Walk each `queries.rs` loop containing an `.await`. Classify: genuinely
  dependent, independent-but-sequential, or mergeable into one statement.
- Independent → `tokio::join!` for a fixed pair, `try_join_all` for a vector.
- Mergeable → one statement. The keyset arms in `assets::fetch_transactions` are
  the clearest candidate: `UNION ALL` with the ordering, dedup and `LIMIT` pushed
  into ClickHouse rather than reassembled in Rust.
- Leave genuinely dependent steps alone and say so in a comment, so the next
  sweep does not re-litigate them. Fetching page rows requires the page keys;
  that is two phases and cannot be one.
- Re-run the load-test harness (`crates/load-tests`, task 0338) before and after
  and compare `ch_queries` per endpoint plus median latency.

## Acceptance Criteria

- [ ] Every `.await`-in-loop site in `crates/api/src/**/queries.rs` classified as
      dependent / parallelisable / mergeable, with the verdict recorded
- [ ] `assets::fetch_transactions` keyset arms no longer sequential
- [ ] Median `ch_queries` per request drops measurably against the 3.24 baseline
- [ ] Load-test rerun shows a median-latency improvement, quantified per endpoint
- [ ] No change in response payloads — pagination, ordering and dedup semantics
      identical (existing endpoint tests stay green without modification)

## What this does NOT fix

**This will not bring AC4's p95 under 200 ms, and the task should not be sold as
if it might.** Simulated against the real measurements by subtracting only the
eliminable round trips:

| Scenario                         | p95, 40× tier | p95 excluding external-fetch endpoints |
| -------------------------------- | ------------: | -------------------------------------: |
| today                            |        576 ms |                                 506 ms |
| 3 round trips                    |        571 ms |                                 504 ms |
| 2 round trips                    |        566 ms |                                 503 ms |
| 1 round trip (theoretical floor) |        551 ms |                                 489 ms |

Even the unreachable one-round-trip floor buys 25 ms against a 377 ms gap.

The p95 tail is a different problem. The slowest 5 % of the 40× tier breaks down
as:

| Endpoint    | share of tail | total   | of which in ClickHouse | queries |
| ----------- | ------------: | ------- | ---------------------: | ------: |
| `nftdetail` |         22.4% | 1397 ms |              **20 ms** |       2 |
| `txdetail`  |         15.0% | 1021 ms |              **16 ms** |       5 |
| `lplist`    |          6.9% | 3020 ms |            **2823 ms** |       2 |
| `lpdetail`  |          4.5% | 1482 ms |            **1469 ms** |       1 |

Two unrelated causes, neither of them round trips: 37 % of the tail is waiting on
third-party infrastructure (IPFS gateway, cross-region S3 archive) with almost no
database time, and 11 % is a single slow scan with one or two queries. A 14.2 ms
round trip is noise against 1400–3000 ms.

This task is worth doing for the **median** — the typical request pays
14.2 ms × 3.24 ≈ 46 ms in round trips, and part of that is avoidable. The tail
belongs to the AC4 follow-ups already named in
`docs/scf/milestone-3-evidence.md` § AC4.

## Future Work

- Persisting or asynchronously serving the two runtime-fetched field sets
  (`txdetail` archive read, `nftdetail` token URI) — the actual p95 driver.
- Restoring `lplist`'s stored creation ledger to remove the full-history scan.
