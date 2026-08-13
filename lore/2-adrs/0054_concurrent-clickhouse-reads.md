---
id: '0054'
title: 'Concurrent ClickHouse reads — four rules'
status: accepted
date: '2026-08-13'
related_tasks: ['0446', '0445', '0481', '0402']
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

4. **Existence gates pair with the read they guard only after measuring the
   miss.** The gate still decides the 404 first, so responses never change;
   the cost is the guarded read wasted on a missing id. Measure that cost for
   an id that does NOT exist: bounded (NFT transfers 24.5k rows / 6.7 ms) →
   pair; unbounded (pool chart 43.7M rows / 4.66 s) → stay serial. Better than
   either: a gate that also RETURNS data the page needs (0279's
   `fetch_pool_asset_ids`) — one query doing both jobs.

## Consequences

Half of requests save 14–28 ms; per-request peak DB concurrency rises but is
bounded (≤6) against `max_concurrent_queries = 1000 (live)` and
`max_connections = 4096 (live)`. The failure mode to guard in review is rule
3: new fan-out without a pool re-count silently converts the gain into
per-request handshakes.
