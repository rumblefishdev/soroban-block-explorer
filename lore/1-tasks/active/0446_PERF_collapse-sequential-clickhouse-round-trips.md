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

## Audit — verdict per site (2026-08-12)

Every one of the 28 endpoints read end to end (handler → query fn → helper). The
unit below is a **wave**: one serial step, whatever its query count. Two queries
in one `tokio::join!` are one wave; `A.await` then `B.await` are two.

### Parallelisable — independent, awaited one at a time

| Site                                         | Endpoint(s)                         | What is serial today                                                                                        | Waves |
| -------------------------------------------- | ----------------------------------- | ----------------------------------------------------------------------------------------------------------- | ----: |
| `contracts::fetch_contract_list`             | `GET /contracts`                    | recent-invocation counts, deployer StrKeys and SAC assets — all three read only `list_rows`                 | 4 → 2 |
| `contracts::fetch_events`                    | `/contracts/:id/events`             | tx headers then ledger `closed_at` — also **mergeable** (`INNER JOIN ledgers`, the shape used 200 lines up) | 3 → 2 |
| `contracts::fetch_invocation_appearances`    | `/contracts/:id/invocations`        | caller StrKeys then tx headers, both off `key_rows`                                                         | 3 → 2 |
| `contracts::fetch_contract`                  | `/contracts/:id`                    | deployer StrKey then SAC asset (SAC-only, ~2.9% of contracts)                                               | 3 → 2 |
| `transactions::resolve_source_and_closed_at` | `GET /transactions`                 | source accounts then ledger `closed_at`; on 2 of the 3 statement paths                                      |    −1 |
| `transactions::fetch_operations`             | `/transactions/:hash`               | `resolve_accounts` then `resolve_contracts`                                                                 |    −1 |
| `transactions::fetch_invocation_appearances` | `/transactions/:hash`               | `resolve_contracts` then `resolve_accounts`                                                                 |    −1 |
| `ledgers::fetch_transactions`                | `/ledgers/:seq`                     | `resolve_accounts` then tx-list aggregates                                                                  | 3 → 2 |
| `liquidity_pools::fetch_pool_transactions`   | `/liquidity-pools/:id/transactions` | page and aggregates both need only the keys                                                                 | 4 → 3 |
| `accounts::get_account`                      | `/accounts/:id`                     | balances then deleted-status, both off `header.id`                                                          | 3 → 2 |

### Parallelisable, but it is an existence gate — DONE

Second query never consumes the first's output — it re-derives everything from
the path. Concurrency wins a wave on the happy path and wastes one query on the 404.

**"404s are rare" is the wrong test, and taking it on frequency alone was a
mistake caught in review.** What matters is what a single 404 costs, because
the ids are user-supplied and trivially generated. Measured on prod for an
entity that does not exist:

| Guarded read                     |      rows read | time       | paired? |
| -------------------------------- | -------------: | ---------- | ------- |
| ledger transactions              |              0 | 4 ms       | yes     |
| pool transactions                |        560,960 | 11 ms      | yes     |
| nft transfers                    |         24,583 | 6.7 ms     | yes     |
| pool participants                |         25,496 | 113 ms     | yes     |
| **pool chart**                   | **43,738,676** | **4.66 s** | **NO**  |
| (the gate itself, `pool_exists`) |         16,569 | 3.6 ms     | —       |

`get_pool_chart` stays SERIAL. Its `JOIN (SELECT … FROM ledgers WHERE closed_at
…)` build side is materialised even when the left side is empty, and
`MAX_CHART_BUCKETS = 1_000` admits a ~19-year window — so pairing it would hand
a 4-second, 43M-row read to anyone who asks for a pool id that does not exist.
That is 1,300× the gate it would have skipped. The other four are paired.

Responses are unchanged in every case: the existence answer is still checked
FIRST, so a missing entity still yields 404 even when the page read failed too.
Only what the 404 path costs changes.

`ledgers::get_ledger` needed one cleanup before it could pair — the tx query
took a `closed_at` argument it never read (`_closed_at`), which made the
dependency look real. Parameter dropped.

| Site                                                           | Endpoint(s)                        | Gate          | Paired             |
| -------------------------------------------------------------- | ---------------------------------- | ------------- | ------------------ |
| `liquidity_pools::{list_participants, list_pool_transactions}` | 2 endpoints                        | `pool_exists` | yes                |
| `liquidity_pools::get_pool_chart`                              | `/liquidity-pools/:id/chart`       | `pool_exists` | **no — see above** |
| `nfts::list_nft_transfers`                                     | `/nfts/:contract/:token/transfers` | `nft_exists`  | yes                |
| `ledgers::get_ledger`                                          | `/ledgers/:seq`                    | header fetch  | yes                |

### Mergeable into one statement

- `assets::fetch_transactions` keyset arms — **done**, `UNION ALL`.
- `contracts::fetch_events` — see above.
- `search::search_contracts` — prefix scan then name lookup could be a subquery,
  but the Rust-side duplicate collapse between the phases has to move into CH
  with it. Lower value: the six search buckets already run under `try_join!`.

### Genuinely dependent — leave alone

Surrogate-id resolution (`fetch_account` → transactions, `fetch_contract` →
stats / invocations / events, asset row → transactions, `lookup_hash_ledger` →
`fetch_detail`): the second query's WHERE clause IS the first's output.
Post-page resolution (`fetch_balances`, `fetch_pool_list`, `fetch_participants`,
`fetch_detail`, `fetch_participants`, `fetch_event_appearances`, NFT owner,
`search_assets` issuers): the id set exists only after the page returns.
`head::current_head_opt` before the list queries is a 304 gate that **skips** the
heavy query — serial on purpose.

Already concurrent: `search::fetch_search` (6 buckets, `try_join!`),
`transactions::get_transaction` (2 × `tokio::join!`),
`assets::fetch_transactions` and `accounts::fetch_transactions` (page ∥
aggregates).

No opportunity: `network/stats`, `/ledgers`, `/assets`, `/accounts`, `/nfts`,
`/liquidity-pools` (list), `/contracts/:id/interface`, `/search`.

**Total: ~16 waves removable across 13 endpoints.** At the measured 14.2 ms per
round trip that is ~14 ms off a typical detail request — the median case the task
was opened for, not the p95 tail.

## Predicted gain, from the recorded run (2026-08-12)

Not a measurement — an extrapolation over the 2026-07-17 per-request data
(`docs/scf/load-tests/40x-49.3M-per-month/results.csv`, 13,698 successful
requests). Each endpoint's measured median wall clock, minus 14.2 ms per wave
this branch removes:

| Endpoint    | n   | waves removed | median | est. after | est. |
| ----------- | --- | ------------: | -----: | ---------: | ---: |
| `ctrlist`   | 537 |             2 | 122 ms |      94 ms |  23% |
| `ctrfilter` | 497 |             2 | 125 ms |      97 ms |  23% |
| `ldgdetail` | 529 |             2 | 145 ms |     117 ms |  20% |
| `lptxs`     | 540 |             2 | 173 ms |     145 ms |  16% |
| `lpparts`   | 538 |             1 |  97 ms |      83 ms |  15% |
| `txfilter`  | 542 |             1 | 108 ms |      94 ms |  13% |
| `lpchart`   | 519 |             0 | 137 ms |     137 ms |   0% |
| `ctrevents` | 524 |             1 | 137 ms |     123 ms |  10% |
| `txlist`    | 492 |             1 | 138 ms |     124 ms |  10% |
| `ctrdetail` | 484 |             1 | 149 ms |     135 ms |  10% |
| `ctrinvoc`  | 507 |             1 | 170 ms |     156 ms |   8% |
| `asttxs`    | 546 |             1 | 195 ms |     181 ms |   7% |
| `accdetail` | 543 |             1 | 213 ms |     199 ms |   7% |
| `txdetail`  | 563 |             2 | 414 ms |     386 ms |   7% |

**State it per request, not as a percentage of an aggregate.** Of the 13,698
requests, **6,842 (49.9%) save something and 6,856 (50.1%) save nothing**: 4,176
save one wave (14.2 ms), 2,666 save two (28.4 ms). The run's actual mean is
227.6 ms and its actual median 151 ms, so a request that benefits gains roughly
9–19% of a median request and the rest gain zero.

The aggregate form — n-weighted mean of per-endpoint medians, 169.8 ms →
159.9 ms (−5.8%) — is the number the AC asks for, but it describes a statistic
no user experiences. `lpchart` contributes nothing (its gate stayed serial on
purpose, see above); including it the figures would have been 7,361 requests and
159.4 ms / −6.1%.

Treat the per-endpoint percentages as an upper bound: the 14.2 ms slope was
fitted against **query count**, and is applied here per **removed wave**. That is
the right reading of what the slope measures (the round trip itself), but it is
still an extrapolation and assumes ClickHouse absorbs the now-concurrent queries
without queueing.

**AC 3 as written cannot pass, and the AC is what is wrong, not the work.**
Concurrency does not reduce the number of queries — it overlaps them. Mean
`ch_queries` stays at 3.25 (verified against the same data); only `asttxs` drops,
8 → 7, from the `UNION ALL`. The criterion should read _waves_, not queries.

## The connection pool had to move with it

Concurrency only pays if the sockets are already open. `db_clickhouse::mtls`
built its hyper client with `pool_max_idle_per_host(2)` — sized, per its own
comment, for the indexer's **serial** writes. HTTP/1 only, so there is no h2
multiplexing to fall back on.

A Lambda instance serves one request at a time, so a `join!` no wider than the
cap reuses its sockets. Above it, the surplus connection is opened per request
and then evicted, and the next request pays a fresh TCP + TLS 1.3 + mTLS
handshake to Hetzner on a 256 MB Lambda — plausibly more than the round trip the
concurrency just saved.

**Count nesting, not arms.** The first attempt raised the cap to 4, on the
belief that the widest fan-out was 3. Review falsified that against this task's
own code — a `join!` inside a joined future multiplies:

- `search::fetch_search` — `try_join!` over 6 arms → **6**. Pre-dates this task,
  so the pool has been undersized for search all along.
- `transactions::get_transaction` — 3 arms, one of which is
  `fetch_invocation_appearances`, itself made a 2-arm join **by this task** → 4.
  That is the archive-degraded path, so the pool is tightest exactly when the
  request is already struggling.

Cap is 8. It bounds _retained idle_ sockets, not in-flight ones, so nothing else
changes and the indexer — which never holds more than one — is unaffected.

Not measured end to end: the handshake-vs-round-trip balance needs the load-test
rerun like everything else here.

## Acceptance Criteria

- [x] Every `.await`-in-loop site in `crates/api/src/**/queries.rs` classified as
      dependent / parallelisable / mergeable, with the verdict recorded
- [x] `assets::fetch_transactions` keyset arms no longer sequential — folded into
      one `UNION ALL` statement (not two concurrent queries): 2 round trips → 1,
      and CH runs the arms in parallel server-side (measured `elapsed`: arm A
      0.211 s, arm B 0.036 s, union 0.230 s ≈ the slower arm, not the sum).
      Differential-checked against prod CH on Circle USDC in both page
      directions, including a limit where the arms overlap so the cross-arm
      dedup is exercised: identical key sets. Read cost 62.4 M rows vs 60.9 M for
      the two arms — granule noise, no pruning regression.
      Gotcha for whoever re-verifies: pin the ledger fence to a literal first.
      The arms carry `ledger_sequence <= (SELECT max(sequence) FROM ledgers)`, so
      running old and new seconds apart on a live chain compares different
      windows and the newest-first page "mismatches" for no reason.
- [x] **Median serial WAVES per request drops** — a wave is one round trip,
      whatever its query count; queries inside one `tokio::join!` are one wave.
      Superseded the original wording, "median `ch_queries` drops against the
      3.24 baseline", which cannot pass by construction: concurrency overlaps
      queries, it does not remove them, so mean `ch_queries` stays at 3.25
      (verified against the same data). Only `asttxs` drops on query count,
      8 → 7, from the `UNION ALL`.

      By waves, measured against the recorded run: 16 waves removed across 14
      endpoints, 6,842 of 13,698 requests (49.9%) losing one or two. The
      remaining check is whether that converts to wall clock, which is AC 4's
      job — see the load-test caveat there and the connection-pool section
      above, since a wave only pays off on an already-open socket.

- [ ] Load-test rerun shows a median-latency improvement, quantified per endpoint
- [ ] No change in response payloads — pagination, ordering and dedup semantics
      identical (existing endpoint tests stay green without modification)
- [x] **Docs updated** — `N/A — no described architecture affected`. Same
      endpoints, same DTOs, same tables; only the order in which existing
      queries are issued changed. Evidence: `extract_openapi` output is
      byte-identical to the committed `libs/api-types/src/openapi.json`.
- [x] **API types regenerated** — ran `npx nx run @rumblefish/api-types:generate`;
      no diff produced, and `check-generated` passes. Nothing to commit.

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
