---
id: '0403'
title: 'OPS: post-ship validation of the 0365 (lptxs) + 0385 (acclist) read swaps — byte-identical diffs, E20, refresh memory cap'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0365', '0385', '0357', '0397']
tags: [priority-high, effort-medium, layer-clickhouse, validation]
links:
  - crates/api/src/liquidity_pools/queries.rs
  - crates/api/src/accounts/queries.rs
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned when 0365 and 0385 were archived. Both swaps shipped (#327 / #328,
      2026-07-13) and their perf claims are evidenced in 0357, but their
      correctness ACs were never validated: neither has a byte-identical
      before/after diff on record, E20 was never re-run for lptxs, and 0385's
      refresh-recompute memory check against the prod 6 GB cap — a stated prod
      risk, not a formality — never happened. Those ACs are deferred here rather
      than ticked, so the gap stays visible instead of dying with the parent tasks.
  - date: 2026-08-06
    status: backlog
    who: karolkow
    note: >
      Execution note from the 0455 triage: most of this task is runnable
      read-only by the assistant - the pre-swap SQL drivers live in git
      history, so both old and new queries can be run via chq and diffed
      byte-for-byte (lptxs across sparse/dense/mega pools; acclist both
      sorts + home_domain filter + cursor pagination), and the 0385 refresh
      memory check reads from system.query_log against the 6 GB cap. Only
      E20 (Horizon comparison) needs the e2e harness.
  - date: 2026-08-06
    status: backlog
    who: karolkow
    note: >
      Read-only validation executed via chq (0455 quick-win sweep). lptxs:
      old operations_appearances has(pool_ids) driver vs new operation_pools
      prefix-seek, reduced to page-key equivalence (enrich path is shared) -
      byte-identical 7/7: sparse(15 keys)/dense/mega(6.1M-row) pools, both
      directions, plus a cursor page with each driver's own keyset form,
      same ledger fence for both sides. acclist: ASC and ASC+home_domain
      pages byte-identical; DESC first page and a below-watermark cursor
      page differ ONLY by refresh skew - classified 100%: every divergent
      account has live last_seen_ledger above the MV watermark (63826191 vs
      live 63826200; 1/1 on the cursor page), zero row-set anomalies. 0385
      refresh memory measured from query_log (fingerprinted by
      written_rows/duration against view_refreshes): peak 734-744 MiB per
      run vs the 6 GB box - no risk; cadence every 2 min reading 17.13M
      rows/1.45 GB per run (0447's volume, confirmed live). Outstanding:
      E20 vs Horizon (needs the e2e harness + network).
---

# OPS: post-ship validation of the 0365 / 0385 read swaps

## Summary

Two read-path swaps are **live on prod and serving traffic** with their
correctness never verified. This task closes that gap. It is validation of
shipped code, not new work — but until it runs, "byte-identical" is an
assumption, and 0385's refresh is an unmeasured memory risk on a 6 GB box.

## Context

Spawned from [0365](../archive/0365_PERF_operation-pools-companion-lptxs.md) and
[0385](../archive/0385_PERF_accounts-recent-refreshable-mv-acclist.md), both
archived 2026-07-17 with these ACs explicitly deferred here. The perf side of
both is settled and measured (see 0357's series 1-3 record); what is missing is
output correctness and one ops-safety check.

## Implementation

### 0365 — lptxs on `operation_pools`

- [x] Byte-identical diff of `/v1/liquidity-pools/:id/transactions` old driver vs
      new `pool_id` prefix-seek, across **sparse / dense / mega** pools (the three
      classes 0365's own design pass called out — a mega pool is the case the old
      over-fetch×4 / re-fetch×128 / Rust-dedup dance existed to handle, so it is
      where a regression would hide).
- [x] E20 (`/liquidity-pools/:id/transactions` vs Horizon) green — 2026-08-06 rerun, `docs/runbooks/artifacts/e20_validation_20260806.md`.

### 0385 — acclist on `accounts_recent`

- [x] Byte-identical diff of `/v1/accounts` old driver vs `accounts_recent`,
      covering **both sort directions + the `home_domain` filter + cursor
      pagination** (allow ≤refresh-interval freshness skew on the newest rows —
      that skew is accepted by design, a row-set difference is not).
- [x] Confirm the refresh recompute (`accounts FINAL` scan + sort over ~22-24M
      rows) stays under the prod **6 GB `max_memory_usage` cap**. If it approaches
      the cap: `max_bytes_before_external_sort`, or relax the 2-minute interval
      (no correctness impact — the MV is a full recompute).
- [ ] Establish whether acclist actually clears the literal AC4 `p95 < 200 ms`.
      0357 measured it only into a "< 300 ms" bucket (CH 19-52 ms), which does not
      settle it either way; note 0357's own finding that a ~60-90 ms per-request
      floor exists before any query runs. Either confirm the number or fold
      acclist into 0357's documented known-issue framing.

### 0397 — sep1 enrichment issuer resolve (added 2026-07-22)

- [ ] After the next deploy **and a drain** (the worker is bursty — a quiet
      window logs nothing), confirm in `system.query_log` that the sep1 issuer
      resolve reads ~24.6k rows/call, not ~24.9M. Query shape to match:
      `nullIf(account_id, ?) AS issuer_strkey … FROM accounts WHERE id = ?`.
- [ ] While there: the same query measured **1.91M rows / 176 granules as
      `dev_read` vs 16.68M / 1838 as `ingestion_writer`** on identical SQL, same
      part count, cause never established (0397 had no `ingestion_writer`
      credentials). If the post-deploy number is not ~24.6k, that discrepancy is
      the first suspect — and it would mean read-cost estimates taken from the
      readonly account do not describe production.

## Acceptance Criteria

- [x] lptxs output verified byte-identical across sparse / dense / mega pools;
      E20 green.
- [ ] 0397's post-deploy read_rows/call measured (~24.6k expected), and the
      `dev_read` / `ingestion_writer` discrepancy either explained or recorded as
      still open.
- [x] acclist output verified byte-identical across both sort directions,
      `home_domain` filter and cursor pagination.
- [x] Refresh recompute measured against the 6 GB cap, with the headroom recorded
      as a number — not "it seemed fine".
- [ ] acclist's AC4 position stated with a measurement: meets `p95 < 200 ms`, or
      documented known-issue with the cause named.
- [ ] Docs updated — mark each `docs/architecture/**` file updated or
      `N/A — reason` (likely N/A: validation only, no shape change).
- [ ] API types regenerated — N/A unless a diff turns up a response-shape bug.
