---
id: '0054'
title: 'Concurrent ClickHouse reads — four rules'
status: accepted
deciders: [karolkow]
related_tasks: ['0446', '0445', '0481', '0402']
related_adrs: []
tags: [performance, api, clickhouse]
links: []
history:
  - date: '2026-08-13'
    status: accepted
    who: karolkow
    note: >
      Written post-factum from 0446: rules discovered independently twice
      (0446 review, 0445) deserve writing once. Rule 4 tightened same day
      from measured-once to bounded-by-construction after review.
---

# Concurrent ClickHouse reads — four rules

## Context

Every serial round trip to ClickHouse (Hetzner, outside the AWS network) costs
~14 ms of wall clock. Task 0446 removed 16 avoidable serial waves; task 0445
independently arrived at the same patterns days later. Rules discovered twice
deserve writing down once.

## Decision

1. **Independent reads go out together.** Two reads where neither consumes the
   other's output run under `tokio::join!`. If they also return the **same row
   shape**, merge them into ONE statement instead (`UNION ALL` +
   `LIMIT 1 BY` — assets keyset arms): one round trip, one socket, and any
   shared scalar subquery (the `max(sequence)` fence) evaluates once, killing
   the torn-page race two statements always carry.

2. **`join!`, not `try_join!`, when failures are not equivalent.** Separate
   `match` arms keep per-query error logs and let one side degrade (nullable
   field) while the other propagates (404/500). `try_join!` only where any
   failure means the same 500 (search buckets).

3. **The mTLS pool covers the widest per-request fan-out, counting NESTING.**
   A `join!` inside a joined future multiplies (tx detail: 3 arms × one arm
   joins 2 = 4; search = 6). `pool_max_idle_per_host` below the true fan-out
   makes every such request pay a fresh TCP+TLS+mTLS handshake (~2×RTT +
   crypto on a 256 MB Lambda) — more than the round trip the concurrency
   saves. Today: widest 6, pool 8. Re-count when adding fan-out.

4. **Existence gates pair with the read they guard only when the miss is
   bounded BY CONSTRUCTION, confirmed by one measurement.** The gate still
   decides the 404 first, so responses never change; the cost is the guarded
   read wasted on a missing id. The criterion is structural, not the
   measurement alone — a number measured once goes stale as data grows:

   - PK-prefix seek + `LIMIT` → miss cost is bounded by the query's own shape
     regardless of table growth → pair (NFT transfers: 24.5k rows / 6.7 ms).
   - Any scan whose extent a _caller-supplied window_ controls (the chart's
     `ledgers` build side, up to a ~19-year window) → unbounded → stay serial
     (measured 43.7M rows / 4.66 s for a missing pool).

   Better than either: a gate that also RETURNS data the page needs (0279's
   `fetch_pool_asset_ids`) — one query doing both jobs. Prefer it whenever the
   schema allows.

## Consequences

Half of requests save 14–28 ms; per-request peak DB concurrency rises but is
bounded (≤6) against `max_concurrent_queries = 1000 (live)` and
`max_connections = 4096 (live)`. The failure mode to guard in review is rule
3: new fan-out without a pool re-count silently converts the gain into
per-request handshakes.
