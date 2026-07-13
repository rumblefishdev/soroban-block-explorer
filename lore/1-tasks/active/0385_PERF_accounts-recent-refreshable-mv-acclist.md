---
id: '0385'
title: 'PERF: accounts_recent — refreshable-MV last_seen-ordered seek for acclist under AC (supersedes 0353 projection-rejected known-issue)'
type: PERF
status: active
related_adr: []
related_tasks: ['0353', '0357', '0319', '0381']
tags:
  [priority-medium, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links:
  - crates/api/src/accounts/queries.rs
  - crates/db-clickhouse/schema/init.sql
history:
  - date: 2026-07-13
    status: active
    who: stkrolikiewicz
    note: >
      Spawned from the 0357 read-path perf cluster / 0353 acclist item.
      **Supersedes 0353's acclist verdict** ("projection REJECTED → launch
      known-issue, no server-side work, latest-accounts table likely YAGNI"):
      the refreshable-MV path is viable and precedented (`balance_aggregates_mv`),
      so acclist gets a real server-side fix under AC instead of relying on the
      FE 60s cache. Design co-designed + prod-sized this session (accounts 5-col
      footprint measured 1.56 GiB compressed). 0353 stays = ctrevents only.
---

# PERF: accounts_recent — refreshable-MV last_seen seek for acclist

## Summary

`acclist` (`GET /v1/accounts`, `accounts::fetch_list`) reads `accounts FINAL`
and sorts by the **non-PK** `last_seen_ledger` — a whole-dimension scan+sort of
~24M rows (~1115 ms CH) on every page. The natural fix (a projection ordered by
`(last_seen_ledger, id)`) is **engine-rejected** on this table (0353: CH 26.3
refuses `ADD PROJECTION` on a `ReplacingMergeTree`, Code 344), and re-keying the
base table is structurally impossible (see Context). Build a **refreshable
materialized view** `accounts_recent` — a plain `MergeTree` copy ordered by
`(last_seen_ledger, id)`, full-recompute + atomic EXCHANGE like the existing
`balance_aggregates_mv` — and swap `fetch_list` Step 1 onto it. The list then
**seeks** (~74k rows / ~5 ms measured for the raw read-in-order form) instead of
scanning 22M, landing acclist under the AC4 latency target with a **shared,
server-side fresh origin** (refresh interval) rather than a per-client FE cache.

## Context

- **Measured (0353, prod, page of 20, isolated):**

  | form                                           | read_rows | ch_dur   |
  | ---------------------------------------------- | --------- | -------- |
  | current (FINAL + non-PK `last_seen` ORDER BY)  | 24.0M     | 1115 ms  |
  | projection + `LIMIT 1 BY id` (rejected spec)   | 6.0M      | 105 ms   |
  | **raw read-in-order seek (no FINAL, no L1BY)** | **74k**   | **5 ms** |

  The 74k/5ms form is only reachable on a structure whose sort key **leads with
  `last_seen_ledger`** — which `accounts` is not, and cannot become.

- **Why re-key the base table is impossible.** `accounts` is
  `ReplacingMergeTree(last_seen_ledger)` `ORDER BY account_id`. RMT dedups by the
  **full sort key**; `last_seen_ledger` **mutates** (a new row per account each
  time it is seen). Put it in the sort key and the same account at two
  `last_seen` values gets two non-equal keys → **RMT no longer collapses it** →
  duplicate account rows that `FINAL` cannot fix. The sort key of a versioned
  dimension must be exactly the identity (`account_id`/`id`). A data-skipping
  index on `account_id` also cannot replace the current leading-PK prefix seek
  (a `minmax` over a non-sorted String prunes ~nothing; `bloom`/`tokenbf` reads
  candidate granules, no ordered prefix-range). So the alternate ordering must
  live in a **separate structure** — which is exactly what a projection is, and
  exactly what CH 26.3 refuses here. `accounts_recent` is that projection,
  hand-rolled.

- **Native-balance join already removed (0319).** `fetch_list` Step 1 is now
  purely `accounts FINAL` + the `last_seen` scan+sort; Step 2 is a bounded
  `balances` id-seek over the ≤page keys. Only Step 1 needs fixing.

- **Precedent.** `balance_aggregates_mv` (`REFRESH EVERY 2 MINUTE`, full
  recompute `FROM balances FINAL … TO balance_aggregates`, atomic EXCHANGE so
  reads need no FINAL) is the exact pattern, on a larger source, under the same
  6 GB box cap. `accounts_recent` mirrors it.

## Design decision — refreshable MV, not a projection or re-key

A refreshable MV writing a plain `MergeTree` ordered by the list's sort key.
Dodges the RMT-projection rejection (separate table, no projection), keeps the
base `accounts` identity ordering intact (point-lookups, `idx_acc_id` reverse,
detail/search seeks all unchanged), and moves the 22M scan+sort **off the request
path** into a periodic recompute.

## Implementation Plan

### Step 1: schema (`init.sql`)

```sql
CREATE TABLE IF NOT EXISTS accounts_recent (
    id                Int64,
    account_id        String,
    last_seen_ledger  Int64,
    first_seen_ledger Int64,
    home_domain       Nullable(String)
) ENGINE = MergeTree
ORDER BY (last_seen_ledger, id);

-- Full recompute + atomic EXCHANGE → reads need no FINAL (mirrors
-- balance_aggregates_mv). Source table `accounts` must already exist.
CREATE MATERIALIZED VIEW IF NOT EXISTS accounts_recent_mv
REFRESH EVERY 2 MINUTE
TO accounts_recent AS
SELECT id, account_id, last_seen_ledger, first_seen_ledger, home_domain
FROM accounts FINAL;
```

Columns are exactly what `fetch_list` Step 1 projects. Refresh interval starts at
2 min (relax to 5 if recompute pressure warrants); freshness of a "recently
active accounts" browse is non-critical.

### Step 2: read-swap (`accounts::fetch_list`)

Point Step 1 at `accounts_recent` (no `FINAL`), same cursor / `home_domain`
filter / `ORDER BY last_seen_ledger {order}, id {order}` / `LIMIT`. Reverse
(DESC) reads early-terminate via `optimize_read_in_order`. Step 2 (native-balance
id-seek) unchanged. Validate output **byte-identical** vs the current driver
(prod before/after, or a local range) allowing for ≤refresh-interval freshness
skew on the very newest rows.

### Step 3: prod rollout (OPS)

- Manual `CREATE TABLE` + `CREATE MATERIALIZED VIEW` on prod (init.sql is
  fresh-install-only; prod schema drift). First refresh backfills the table.
- Confirm the refresh recompute (`accounts FINAL` scan + sort) stays under the
  6 GB `max_memory_usage` cap (precedent: `balance_aggregates_mv` on the larger
  `balances`); if it ever approaches the cap, `max_bytes_before_external_sort`
  or a longer interval.

### Step 4: docs

Update `docs/architecture/**` schema page (new table + MV) per ADR 0032.

## Sizing (prod-measured 2026-07-13)

The 5 projected columns on the existing `accounts` table:
**1.56 GiB compressed** (1.97 GiB raw, ~71 B/row, ~22–24M rows). This is an
upper bound — `accounts_recent` holds the deduped set (≤ the versioned physical
count). Compression is poor (1.26×) because `account_id` (random 56-char strkey)

- `id` (surrogate) dominate and are effectively incompressible; `last_seen`
  (sorted leading key), `first_seen`, and mostly-NULL `home_domain` compress to
  near-nothing. Negligible next to the 690 GiB snapshot / 6.4B-row `oa`. A leaner
  variant (drop `account_id`, resolve id→account_id at read via `idx_acc_id`) would
  ~halve it but adds a per-page join — not worth it at 1.5 GiB.

## Acceptance Criteria

- [ ] `accounts_recent` + `accounts_recent_mv` live (init.sql + prod), refreshing
- [ ] `fetch_list` Step 1 reads `accounts_recent` (no `FINAL`); `read_rows`
      bounded to ~page size (seek), verified via `system.query_log` — not the
      24M scan
- [ ] Output byte-identical to the current driver (prod before/after), modulo
      ≤refresh-interval freshness on the newest rows; both sort directions +
      `home_domain` filter + cursor pagination covered
- [ ] acclist p95 under the AC4 target at idle (point from the ~5 ms CH seek +
      the bounded Step-2 balance seek)
- [ ] Refresh recompute stays under the prod 6 GB cap
- [ ] **Docs updated** — REQUIRED (new schema objects): schema page under
      `docs/architecture/**` per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md)
- [ ] **API types regenerated** — N/A (query-internal; no DTO/route change)

## Notes

- **Freshness tradeoff (accepted):** the list is ≤refresh-interval stale. This is
  a **shared, server-side** fresh origin — strictly better than the current
  per-client FE 60s cache — and freshness on a "recently active accounts" browse
  is cosmetic. AC is latency, not freshness; met.
- **Cost:** a second ~1.5 GiB narrow table + `accounts FINAL` recompute every
  ~2 min (~1.1 s CH, off-request ≈ 1% background). Modest; `balance_aggregates_mv`
  does the same on a larger source.
- **Not incremental:** full recompute (like `balance_aggregates`). If `accounts`
  grows a lot, relax the interval — no correctness impact.
- **Rejected alternatives:** (a) re-key `accounts` to `last_seen`-centric — breaks
  RMT dedup (mutable field in sort key); (b) `ADD PROJECTION` on RMT — CH 26.3
  Code 344; (c) skip-index on `account_id` — not a seek, regresses search.
- 0353 keeps its **ctrevents** half (read-in-order defeat, query-only / config);
  only the acclist item moves here.
