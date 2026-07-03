---
id: '0353'
title: 'PERF: ctrevents read-in-order + acclist projection (deferred from 0345)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0345', '0338']
tags:
  [priority-medium, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0345 future work — the 2 of 7 entity endpoints needing a different technique than the id-IN resolver.'
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

### acclist (`accounts::fetch_list`) — schema

`accounts FINAL ORDER BY last_seen_ledger` (non-PK sort) full-scans the table.
Add a projection `ORDER BY (last_seen_ledger, id)` (+ drop FINAL, rely on
`LIMIT 1 BY id`). This is a schema change on a ~25M-row table — prod needs
`ALTER TABLE accounts ADD PROJECTION … + MATERIALIZE PROJECTION` (heavy one-time
scan + standing storage/write overhead). Sorting a list by a non-PK column has no
cheaper structure. Update `docs/architecture/**` (ADR 0032) for the projection.

## Acceptance Criteria

- [ ] `ctrevents` reads ~page-size, not the contract's whole event slice; output byte-identical (before/after diff on a hot contract); approach (A or B) chosen + its deploy cost noted
- [ ] `acclist` no longer full-scans `accounts`; output byte-identical; projection materialised on prod
- [ ] Also fold in `ctrinvoc` read-in-order residual (same `LIMIT 1 BY` shape) if approach B is taken
- [ ] Docs updated (ADR 0032) for the acclist projection
