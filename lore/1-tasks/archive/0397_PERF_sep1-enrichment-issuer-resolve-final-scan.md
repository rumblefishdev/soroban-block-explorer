---
id: '0397'
title: 'PERF: sep1 enrichment issuer resolve does `accounts FINAL WHERE id=?` — 24M rows/call, 4.5B/6h (one-line fix to bloom seek)'
type: PERF
status: completed
related_adr: []
related_tasks: ['0395', '0396', '0401', '0403']
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
  - date: 2026-07-22
    status: completed
    who: karolkow
    note: >
      One SQL statement, 3 files, PR #359. Dropped FINAL so the existing
      idx_acc_id bloom index seeks: 24.9M -> 24 576 read_rows/call (3 granules,
      15 ms), 100.22 bn -> ~0.1 bn per 7 days. 87 tests pass, none changed.
      AC1/AC3 closed; AC2 closed for the query shape (api_reader already runs it
      in prod, 16k calls/7d at ~21k read_rows) but the deployed worker's own
      number is deferred to 0403 — deploys are manual. Scope cut 4x mid-task: a
      devil's-advocate pass found 3 of the 4 "call sites" were #[cfg(test)]
      readbacks, killing the planned shared resolver + guard test. Five
      alternatives rejected with numbers. Original figures were stale and the
      "0387 option C" fallback referenced a deleted task. Four unowned
      follow-ups recorded, largest being balance_aggregates_mv at 12.5% of
      cluster reads; notes added to 0395/0401/0403.
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
- [x] Seek shape verified cheap on prod **independently of my own A/B**: the API
      already runs it against the same table as `api_reader` — 8 048 + 7 949
      single-key calls over 7 days at **20 711 / 21 097 avg read_rows**, 12–13 ms
      (`system.query_log`). Matches the 24 576 measured for this task's SQL.
- [ ] The deployed worker's own read_rows/call, post-deploy **(deferred to 0403)** — deploys here are manual and this needs a drain to appear
      (bursty: a quiet window shows nothing). 0403 is the established home for
      "shipped read-swap, correctness/measurement not yet verified in prod".
- [x] Enrichment output unchanged (issuer StrKey + home_domain identical) — see
      **Why the output cannot differ** below.

## Why the output cannot differ

Established from the code plus two prod checks, not from a live-CH run (the only
test covering this path is `#[ignore]`).

**Same row selected.** `FINAL` collapses the RMT by its sort key (`account_id`)
keeping `max(last_seen_ledger)`; the seek filters by `id` and takes
`max(last_seen_ledger)`. These agree iff `id → account_id` is 1:1 — verified:
`uniqExact(id) = uniqExact(account_id) = 14 361 780`, i.e. **no hash64
collision** anywhere in the table.

**No ambiguous tiebreak.** The only way the two shapes could diverge is two
versions sharing the top `last_seen_ledger` with different `home_domain`. All 4
prod accounts carrying >1 `home_domain` have pairwise-distinct
`last_seen_ledger` — the max is unique in every case.

**Missing issuer** is unchanged by construction: both shapes return 0 rows,
`fetch_optional` yields `None`, the sentinel path runs.

## Note on the dev_read / ingestion_writer gap

The OLD query, same id, same part count, measured **1.91M rows / 176 granules as
`dev_read` but 16.68M / 1838 as `ingestion_writer`** (the worker's user). Cause
not established, and I have no `ingestion_writer` credentials to isolate it.

It does not threaten this fix. The gap sits _inside_ the `FINAL` case — both
ends of it (1.91M and 16.68M) are 80–700× worse than the ~21k the seek shape
demonstrably costs in production for a production user. The size of the win is
uncertain within that range; that it is a win is not. Worth understanding on its
own terms, since it means read-cost estimates taken from a readonly account may
not describe what production executes.

## Implementation Notes

One SQL statement changed, in one file. Three files touched:
`sep1_assets.rs` (the query + the reasoning comment), `init.sql` (the
`home_domain` comment), this task file. `cargo check -p enrichment-shared`
clean, `cargo test -p enrichment-shared` 87 passed / 0 failed / 7 ignored. No
API-types regen (`crates/api/**` untouched). ADR 0032 docs: **N/A** — the
architecture docs describe `home_domain`'s cardinality
(`clickhouse-pilot.md:654`), not its mutability or the query shape. No test
changed. PR #359.

## Design Decisions

### From Plan

1. **Bloom seek over the alternatives.** The task proposed either the seek or a
   dictionary. The seek lands at 3 granules — ClickHouse's floor — so nothing
   below it is reachable; the dictionary would have removed 0.1% of the original
   cost for ~1 GB resident RAM.

### Emerged

2. **Kept `WHERE id = ?` rather than switching to `IN (?)`.** The task text
   proposed `IN`. Measured both: identical 24 576 read_rows. Kept `=` for the
   smaller diff.

3. **Left the three `#[cfg(test)]` sites alone.** They looked like production
   call sites in a grep. They are ground-truth readbacks where `FINAL` is
   deliberately correct — repointing them onto shared logic would make them
   verify that logic against itself.

4. **No shared resolver function, no guard test.** Both were planned and both
   were dropped once the site count turned out to be one. An abstraction for
   N=1, and a grep-guard that would have fired on the three legitimate test
   occurrences on its first run.

5. **AC3 closed by argument + prod data instead of a live-CH run.** The only
   test covering this path is `#[ignore]`. Rather than leave the AC open, the
   two ways the shapes could diverge were each checked against prod — see "Why
   the output cannot differ".

## Issues Encountered

- **The original task's numbers were stale.** 37M physical rows, 187 calls/6h;
  actual 15.0M / 4 027 calls per 7 days. Merges had run in between. Re-measured
  rather than trusted.

- **A dangling task reference.** The plan's fallback option pointed at "0387
  option C". 0387 was deleted and renumbered in `18ba218b` — 0397 is one of its
  byproducts. The referenced dictionary never existed.

- **Measured with the wrong CH user.** The A/B was run as `dev_read`; the worker
  runs as `ingestion_writer`, whose reads of the same query differed 8×. Surfaced
  by a devil's-advocate pass, then closed a different way — by finding the seek
  shape already running in production under `api_reader` (16k calls / 7d at ~21k
  read_rows). Recorded above as its own note.

- **Prettier flattens nested bullets under a checkbox** into one run-on
  paragraph (happened in `fc0b2b9c`, fixed in `d538e7e5` by promoting the
  evidence to a top-level section).

## Future Work

Surfaced while profiling for this task, **none owned yet** — deliberately not
spawned as backlog tasks in-session at the operator's request, pending a
walkthrough of what each one actually is:

1. **`balance_aggregates_mv` amortisation** — 457.5 bn rows / 7d (12.5% of all
   cluster reads), 4.26 CPU-h/day, 720 full recomputes/day over 90.8M rows to
   absorb 1.09M changed rows (60 000:1), serving 903 reads/day. Four variants
   costed; two refuted with data (read-time aggregation: top asset has 30.16M
   holder rows; aggregate projection: RMT duplication 1.82× inflates the sum).
2. **The `dev_read` / `ingestion_writer` discrepancy** — read-cost estimates
   taken from the readonly account may not describe production.
3. **`ingestion_writer`: `SELECT t.? AS asset_type…`** — 107 317 calls / 7d ×
   607 839 read_rows ≈ 65 bn.
4. **`ingestion_writer`: `max(sac_contract_id) … GROUP BY asset_*`** — 65 226 ×
   411 461 ≈ 27 bn.

Notes added to existing owners rather than new tasks: **0403** (the post-deploy
measurement for this task), **0395** (`accounts_recent_mv` is 670:1), **0401**
(45.8 bn / 7d flows through `FINAL` on `lp_positions` / `liquidity_pools`).
