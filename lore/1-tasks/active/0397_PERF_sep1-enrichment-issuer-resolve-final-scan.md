---
id: '0397'
title: 'PERF: sep1 enrichment issuer resolve does `accounts FINAL WHERE id=?` — 24M rows/call, 4.5B/6h (one-line fix to bloom seek)'
type: PERF
status: active
related_adr: []
related_tasks: ['0387']
tags: [perf, clickhouse, enrichment, effort-small, priority-medium]
milestone: 3
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: karolkow
    note: >
      Found during 0387 read-path priority profiling of prod query_log. NOT an
      API endpoint — a background enrichment worker. Same account-resolve
      antipattern (FINAL scan) as 0387's txlist, different code path.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      Activated. Picked up for implementation.
---

# PERF: sep1 enrichment issuer resolve — `accounts FINAL WHERE id=?`

## Summary

The SEP-1 enrichment worker resolves an asset issuer's `account_id` +
`home_domain` (to fetch its `stellar.toml`) with a **`FINAL` scan** of the
`accounts` table for a single id:

```rust
// crates/enrichment-shared/src/enrich_and_persist/sep1_assets.rs:71
"SELECT nullIf(account_id,'') AS issuer_strkey, nullIf(home_domain,'') AS home_domain
 FROM accounts FINAL WHERE id = ? LIMIT 1"
```

`id` is NOT the `accounts` sort key (`ORDER BY account_id`), so `FINAL WHERE
id = ?` has no key range to bound and read-merges the whole table for ONE id.
Re-measured on prod 2026-07-22 (`system.query_log`, 7 days): **4 027 calls, avg
24.9M read_rows/call, 100.22 BILLION rows total**; peak hour 884 calls /
25.4 bn. Bursty (backfill drains), so a quiet day shows only ~128 calls. Table
is now 15.0M physical rows / 14.36M distinct (the 37M in the original note
predates the merges). Background work, but the single heaviest `accounts`
resolve in the cluster.

## Context

NOT an API endpoint — a background enrichment worker (SQS Lambda +
`backfill-enrichment-runner`). The original note referenced "0387 option C";
**0387 no longer exists** — it was deleted and renumbered in `18ba218b`, and
0397 is one of its three byproducts (with 0395/0396). No surrogate→StrKey
dictionary exists or is planned; only `transaction_hash_dict`, whose own
usefulness 0396 questions.

## Implementation

Drop `FINAL`, keep the same predicate — the `idx_acc_id` bloom index
(`bloom_filter(0.001)`, live since 2026-06-16) turns it into a seek:

```sql
FROM accounts WHERE id = ? ORDER BY last_seen_ledger DESC LIMIT 1
```

A/B on prod, same id, identical result:

|                             | read_rows              | granule    | ms       |
| --------------------------- | ---------------------- | ---------- | -------- |
| `FINAL WHERE id = ?`        | 1 909 317 – 16 679 579 | 176 – 1838 | 27 – 547 |
| seek + `ORDER BY … LIMIT 1` | **24 576**             | **3**      | **15**   |

24.6k rows is 3 granules — CH's floor. Bound the key, THEN dedup: the house
shape from `api::common::ch` and `assets::hydrate_sql` (task 0364). The `ORDER
BY last_seen_ledger DESC` is load-bearing — `home_domain` IS mutable
(SET_OPTIONS; **4 of the 1.01M** prod accounts that carry one have >1 value),
so a bare `LIMIT 1 BY id` would be wrong here.

Also fixed: the `init.sql` comment claiming `home_domain` is "write-once per
account" — contradicted by those 4 accounts.

### Rejected, with the numbers (so they are not re-litigated)

- **`dictGet`** — after the seek only 99M rows / 7d remain; a dictionary would
  remove 0.1% of the original cost for ~1 GB resident RAM (14.36M keys).
- **Carry the issuer StrKey in the SQS message** — the producer does hold it
  ([`enrichment_publish.rs:148`](../../../crates/indexer/src/handler/enrichment_publish.rs)),
  but `assets.issuer_id` stores no StrKey, so the backfill path must resolve
  anyway, and `home_domain` must be read fresh regardless. Fixes 1 column of 2
  for 1 caller of 2.
- **Batch the resolve into `select_sep1_chunk`** (phase-1 seek → phase-2
  hydration) — cannot beat 3 granules per distinct issuer, and forces a
  signature change on `enrich_asset_from_sep1`.
- **In-process memo cache** — 4 027 calls hit only **278 distinct issuers**
  (14.5×), but post-fix that redundancy is 99M rows / 7d, i.e. noise, and a
  cache would add staleness semantics to a mutable column.
- **Shared resolver fn in `db-clickhouse` + repoint "4 call sites"** — three of
  the four `accounts FINAL WHERE id` hits sit behind `#[cfg(test)]`
  (`bootstrap.rs:391`, `repair_tier1.rs:354`); they are ground-truth readbacks
  where `FINAL` is correct and costs nothing. Production sites: **one**. An
  abstraction for N=1, and repointing the tests would make them verify the
  helper against itself.

## Acceptance Criteria

- [x] `sep1_assets.rs` issuer resolve no longer uses `accounts FINAL WHERE id=?`.
- [ ] Measured read_rows/call drops from ~24.9M to ~24.6k, verified via
      `system.query_log` after a drain (bursty — a quiet window shows nothing).
- [ ] Enrichment output unchanged (issuer StrKey + home_domain identical).

## Open before deploy

The same query, same id, same part count measured **1.91M rows / 176 granules as
`dev_read` but 16.68M / 1838 as `ingestion_writer`** (the worker's user), cause
unexplained. The seek A/B above was run as `dev_read` only — I have no
`ingestion_writer` credentials. If the gap comes from that profile, the fix
could land flat. Re-run the seek as `ingestion_writer`, or accept that a flat
AC2 means the profile is the problem, not the SQL.
