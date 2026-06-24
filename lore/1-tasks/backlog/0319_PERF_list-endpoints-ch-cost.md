---
id: '0319'
title: 'PERF: list-endpoint CH cost — PK-aligned ordering (projections) + drop FINAL/reverse-lookup; measure prod first'
type: FEATURE
status: backlog
related_adr: ['0047']
related_tasks: ['0243', '0281', '0290', '0317']
tags:
  [
    'performance',
    'api',
    'clickhouse',
    'accounts',
    'liquidity-pools',
    'contracts',
    'assets',
    'priority-medium',
    'layer-api',
    'layer-backend',
  ]
links:
  - crates/api/src/accounts/queries_ch.rs
  - crates/api/src/liquidity_pools/queries_ch.rs
  - crates/api/src/contracts/queries_ch.rs
  - crates/api/src/assets/queries_ch.rs
history:
  - date: 2026-06-23
    status: backlog
    who: fmazur
    note: >
      Spawned from the list-endpoint latency investigation (~1s/request in
      prod). CORS preflight max-age was split off and fixed under 0317; this
      task covers the CH query cost + the prod measurement to confirm
      attribution.
---

# PERF: list-endpoint CH cost

## Summary

The list endpoints (`/accounts`, `/liquidity-pools`, `/contracts`, `/assets`)
take ~1s/request in prod. Structural analysis (EXPLAIN on a local CH; numbers
scale to prod) points to one recurring cause: **the list sort column is not
aligned with the CH table primary key, so each page does a full-table scan +
sort** (often plus `FINAL` and an `accounts` reverse-id lookup). This task
(a) **measures prod first** to confirm the attribution, then (b) makes the
queries index-aligned.

The CORS-preflight half of the latency (an extra `OPTIONS` round-trip per
request) was the cheap win and is already handled under [[0317]] (API Gateway
`maxAge`). This task is the backend/CH half.

## Step A — measure prod (do this first)

Confirm where the ~1s actually goes before changing schema:

- Per-endpoint **TTFB** via `curl -w` (server time vs total) through the edge
  with a token; and/or
- prod `system.query_log` `read_rows` / `query_duration_ms` for the four list
  statements (rank by real cost).

Acceptance: a table of prod TTFB + read_rows per endpoint, so the fixes below
are prioritised by measured cost (not just structure).

## Step B — per-endpoint findings + fixes

Structural analysis (local EXPLAIN), ranked by likely prod cost:

1. **`/accounts`** (heaviest). `accounts FINAL` + `ORDER BY last_seen_ledger
DESC` (PK = `account_id`) → full scan + sort (~18M rows prod), plus a
   `LEFT JOIN account_balances_current FINAL`. Fix options: a CH **projection**
   ordered by `(last_seen_ledger DESC, id)` (preserves the API sort contract);
   drop `FINAL` via read-in-order + `LIMIT 1 BY` / `argMax` (cf. the 0317 events
   fix); resolve the native balance by a per-page key-seek instead of the FINAL
   join.
2. **`/liquidity-pools`**. `ORDER BY last_updated_ledger DESC` (PK = `pool_id`)
   → full sort + heavy per-page snapshot/position enrichment (code comment:
   ~55M rows/page). Documented as user-initiated, but a projection on
   `last_updated_ledger` + the pre-aggregated snapshot would cut it.
3. **`/contracts`**. `ORDER BY id DESC` (PK = `contract_id`) → full sort of
   contracts + `accounts` reverse-id lookup. Fix: projection on `id`, or drop
   the deployer `accounts` join from the list row (resolve on detail).
4. **`/assets`** (lightest — its `ORDER BY` is already PK-aligned). Cost is the
   `accounts`/`soroban_contracts` reverse-id lookups (`id → account_id`). Fix:
   **verify the `idx_acc_id` bloom filter is live on the prod CH** (applied in
   0290); add an `(id)` projection if still slow.

Cross-cutting: the **`id → account_id` reverse lookup** (id is not the
`accounts` PK) recurs across modules; a shared `(id)`-ordered projection /
lookup path would help several endpoints at once.

## Constraints

- **No full-table hash joins** in any rewrite (see the 0317 events bug: a naive
  `JOIN transactions`/`accounts` builds the hash side from the whole table →
  CH Code 241). Use page-then-key-seek.
- Changing the **sort column** to the PK (instead of a projection) changes the
  displayed order — that is a **product decision**, not a free win. Prefer
  projections to keep the API contract.
- CH schema changes (projections, `MATERIALIZE`) are an ingestion/ops change —
  coordinate with the maintenance window in [[0281]].

## Acceptance Criteria

- [ ] Step A: prod TTFB + `read_rows` table per list endpoint (attribution
      confirmed).
- [ ] Each list first page reads bounded rows (no full-table scan+sort); target
      a clear drop in TTFB (e.g. ~1s → low-hundreds-ms) on the heaviest.
- [ ] API sort contract preserved (or an explicit product decision recorded if
      changed).
- [ ] No full-table hash joins introduced; no new Code 241 risk.
- [ ] `idx_acc_id` bloom presence on prod CH verified.
- [ ] Docs/schema updated per ADR 0032 if projections/columns are added.
