---
id: '0353'
title: 'PERF: ctrevents read-in-order + acclist projection (deferred from 0345)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0345', '0338', '0385']
tags:
  [priority-medium, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0345 future work — the 2 of 7 entity endpoints needing a different technique than the id-IN resolver.'
  - date: 2026-07-06
    status: backlog
    who: stkrolikiewicz
    note: >
      acclist spec CORRECTED — measured on prod that `LIMIT 1 BY id` defeats
      read-in-order (6M rows) vs a raw seek (74k/5ms), so the projection must use
      approach-B (raw over-fetch + Rust dedup), not `LIMIT 1 BY`. Decision: projection
      over edge-cache (0346) for freshness. Kept in backlog (0353 id collides with
      0353_REFACTOR — untangle before promoting); acclist implemented under the 0357
      cluster. ctrevents part unchanged.
  - date: 2026-07-06
    status: backlog
    who: stkrolikiewicz
    note: >
      acclist projection REJECTED after a CH-26.3 local spike: ADD PROJECTION on a
      ReplacingMergeTree is refused by default (Code 344; rebuild=write-amp,
      drop=useless). Value case also collapsed — acclist is a low-traffic browse page,
      already FE-cached 60s, freshness cosmetic. Verdict: launch known-issue, no
      server-side work; edge-cache/latest-table post-launch only if traffic warrants.
      See the RESOLVED note. ctrevents part still open.
  - date: 2026-07-13
    status: backlog
    who: stkrolikiewicz
    note: >
      acclist item EXTRACTED to 0385 and its known-issue verdict SUPERSEDED. The
      refreshable-MV path (a `last_seen`-ordered `accounts_recent` MV, mirror of
      `balance_aggregates_mv`) is viable — dodges the RMT-projection rejection via
      a separate plain-MergeTree table — so acclist gets a real server-side fix
      under AC instead of the FE 60s cache. This task now = ctrevents only.
---

# PERF: ctrevents read-in-order + acclist projection

## Summary

The last 2 of the 7 entity-filtered endpoints from the 0338 load test. Unlike the
5 fixed in 0345 (whole-`accounts` FINAL joins → id-IN resolver), these need
different techniques and were deferred: `ctrevents` is a read-in-order defeat,
`acclist` needs a schema-level projection.

## Context

Diagnosis + measurements in **0345**. Both still read ~25M on prod.

## Implementation

### ctrevents (`contracts::fetch_events`) — API-only OR small CH config

`soroban_events` seeks by `contract_id` (leading PK), but the inner
`LIMIT 1 BY (ledger_sequence, transaction_id, event_index)` defeats
`optimize_read_in_order`: measured **892k read_rows to return 11** for a 12.3M-event
contract (with 0 real duplicates). Two viable fixes:

- **(A) `SETTINGS read_in_order_two_level_merge_threshold = 0`** (keep `LIMIT 1 BY`):
  output identical by definition, ~3× (892k→295k). BUT a per-query `SETTINGS`
  fails under prod `api_reader` `readonly=1` (`Code: 164`, same class as the 0344
  `log_comment` block) → needs a `crates/db-clickhouse/users.d/profiles.xml` change
  (add to `changeable_in_readonly`, or set the value in the `read_only` profile) +
  a prod CH container recreate. NOT API-only.
- **(B) drop inner `LIMIT 1 BY`, dedup in Rust with an exact fallback** (re-query
  the original `LIMIT 1 BY` form only when a duplicate is detected in the page
  window): API-only, ~8× (892k→106k), provably equivalent. More code.
- A naive over-fetch buffer is NOT formally safe (rejected — can under-fill the
  page / shift the cursor under re-ingest duplicates).

### acclist (`accounts::fetch_list`) — schema (projection) + approach-B

`accounts FINAL ORDER BY last_seen_ledger` (non-PK sort) full-scans the table.
Add a projection `ORDER BY (last_seen_ledger, id)` so the list SEEKS instead of
scanning. Schema change on the 22M-row table — prod needs
`ALTER TABLE accounts ADD PROJECTION … + MATERIALIZE PROJECTION` (heavy one-time
scan + standing storage/write overhead). Update `docs/architecture/**` (ADR 0032).

**Correction (2026-07-06 measurement) — do NOT pair it with `LIMIT 1 BY id`.**
That defeats `optimize_read_in_order` exactly like ctrevents/asttxs. Measured on
prod (page of 20, isolated), read_rows / ch_dur:

| form                                                 | read_rows | ch_dur   |
| ---------------------------------------------------- | --------- | -------- |
| current (FINAL + non-PK ORDER BY)                    | 24.0M     | 1115 ms  |
| projection + `LIMIT 1 BY id` (the old spec)          | 6.0M      | 105 ms   |
| **raw read-in-order seek (no FINAL, no LIMIT 1 BY)** | **74k**   | **5 ms** |

So the projected query must use **approach-B** — raw over-fetch on the projection's
`ORDER BY last_seen_ledger DESC` + Rust consecutive-dedup by account_id (the asttxs
pattern), NOT `LIMIT 1 BY`. That lands acclist at ~5 ms ch (~74k rows) →
~110–150 ms total, **fresh + load-resistant, under AC4**. `LIMIT 1 BY` leaves it at
6M/request.

**RESOLVED — 2026-07-06: projection REJECTED, acclist = launch known-issue.**
A CH-26.3 local spike (throwaway container == prod version) killed it: `ADD
PROJECTION` on a `ReplacingMergeTree` is refused by default
(`deduplicate_merge_projection_mode = throw`, Code 344) — CH itself flags the
projection + RMT-dedup combo as unsafe. Both allowed modes lose: `rebuild`
re-sorts the whole projection on every merge of the hot 22M table (heavy standing
write-amp), `drop` deletes the projection on merge (useless on a constantly-merging
table). The value case collapsed too: acclist is a **low-traffic browse page**
(`GET /v1/accounts`, only the FE `AccountsListPage`, sole sort = last_seen), already
**FE-cached 60 s** (React Query `listPolicy`), and its freshness is **cosmetic** (a
≤60 s-stale "recently active" list is fine). So:

- **Launch:** no server-side work. FE cache absorbs repeats; the residual cold
  first-load (~5 s) is a documented known-issue, NOT an AC4 blocker (account
  point-lookups are fast; this is only the browse list).
- **Post-launch, only if traffic data warrants:** edge-cache (0346) is marginal at
  low traffic (cold edge); a fresh origin would need a latest-accounts table
  (indexer-maintained collapsing structure) — a real indexer task, likely YAGNI.
- The ctrevents part of this task is unaffected.

## Acceptance Criteria

- [ ] `ctrevents` reads ~page-size, not the contract's whole event slice; output byte-identical (before/after diff on a hot contract); approach (A or B) chosen + its deploy cost noted
- [~] `acclist` — projection REJECTED (CH 26.3 blocks RMT projections; spike-confirmed); accepted as a launch known-issue (FE-cached 60 s, low-traffic browse, cosmetic freshness). Not fixed server-side. See RESOLVED note above.
- [ ] Also fold in `ctrinvoc` read-in-order residual (same `LIMIT 1 BY` shape) if approach B is taken
- [ ] ~~Docs (ADR 0032) for the acclist projection~~ — N/A, projection rejected (no schema change)
