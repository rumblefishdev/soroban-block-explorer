---
id: '0602'
title: 'ClickHouse upgrade 26.3 → 26.8 LTS — dormant until a problem it solves'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0420']
tags: ['clickhouse', 'dormant', 'effort-medium', 'priority-low']
links:
  - https://github.com/ClickHouse/ClickHouse/blob/master/CHANGELOG.md
history:
  - date: 2026-09-30
    status: backlog
    who: karolkow
    note: >
      Task created from a changelog review of 26.4–26.9 against production
      (26.3.10.60). Parked on purpose: pick it up when one of the triggers
      below shows up, not before.
  - date: 2026-09-30
    status: active
    who: karolkow
    note: >
      Activated for the first PR only: explicit dedup of joined RMT tables,
      which is correct on 26.3 too. The version bump stays parked.
---

# ClickHouse upgrade 26.3 → 26.8 LTS — dormant until a problem it solves

## Summary

Production runs ClickHouse **26.3.10.60** (LTS of 2026-03-26). **26.8 LTS**
(2026-08-27) improves several things this explorer fights with — `FINAL` on
`ReplacingMergeTree`, join and `GROUP BY` memory — but nothing is broken today.
This task records what the upgrade would buy, what it would break, and how to
do it safely, so that whoever hits one of the triggers below starts from here.

## When to pick this up

Promote the task when any of these happens:

- A read endpoint is slow or over its `read_rows` / memory quota because of
  `FINAL` over unmerged RMT parts (the dedup-on-read cost, task 0420).
- A query dies on the memory limit in a hash join or a `GROUP BY` and the fix
  would otherwise be another hand-written seek/prefilter workaround.
- 26.3 support is about to end (LTS lines get about a year, so roughly
  2027-03 — estimate; check the release policy).
- A feature of 26.9+ is wanted (see "Not in 26.8" below) — then aim at the
  next LTS (27.3) rather than a non-LTS release.

## What 26.8 would give us (from the changelog, not measured on our data)

| Version | Change                                                                                                                | Where it would help                                                   |
| ------- | --------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| 26.7    | Lazy materialization for RMT `FINAL` + filter + small `LIMIT`: wide columns read only for rows that survive the limit | list endpoints reading RMT tables with `FINAL`                        |
| 26.7    | Joins prune the left table by its primary key / skip indexes (`enable_join_runtime_filters_index_analysis`)           | the "seek `ledgers`, never join it whole" workarounds in `crates/api` |
| 26.7    | Hash join stores 8-byte row references; `ALL` joins as small as `ANY`                                                 | join memory                                                           |
| 26.7    | Aggregate each partition independently when the key fits the partition key                                            | full-table aggregates                                                 |
| 26.8    | `GROUP BY … LIMIT` keeps a bounded heap; adaptive parallel aggregator                                                 | high-cardinality `GROUP BY` under the reader memory limit             |
| 26.8    | Lightweight `UPDATE` patch parts v2 — bounded memory                                                                  | only if we ever use lightweight updates                               |

### Not in 26.8 (26.9, non-LTS)

- `SELECT … FINAL` on RMT: up to 40% less CPU.
- Refreshable MVs with `REFRESH … APPEND INCREMENTAL` — our refreshable MVs
  currently recompute in full.

## What it would break — check before upgrading

1. **`FINAL` on the left table of a `JOIN` no longer applies to the joined
   tables (26.7).** Measured 2026-09-30: on 26.3,
   `FROM transactions t FINAL INNER JOIN ledgers l` also dedups `ledgers`.
   `ledgers` then held 1,090,860 unmerged duplicates in 15,332,505 rows
   (7.1%); ledger 60534355 returned 222 rows and would return 444 after the
   upgrade — silently, no error. Known sites:
   `crates/api/src/transactions/queries.rs` (ledger transaction list) and
   `crates/api/src/contracts/queries.rs` (two joins to
   `wasm_interface_metadata`). The scan that found them only read literal SQL;
   queries assembled with `format!` need a manual pass.
2. `max_insert_threads` defaults to `auto` (26.8): `INSERT SELECT` in
   backfills and `repair-tier1` creates more parts. Pin it in the writer
   profile or re-measure.
3. Insert deduplication moves to one unified hash (26.6/26.7); the server
   refuses to start with a legacy `insert_deduplication_version`. Our config
   sets none (checked 2026-09-30) — re-check.
4. Default x86 build requires AVX2 (26.5). Verify the CPU of `ch-prod-01`
   (`lscpu | grep avx2`) — not checked.
5. `AggregatingMergeTree` rejects columns outside the sorting key that are not
   aggregate states (26.7). `asset_sac` complies (all plain columns are in
   `ORDER BY`); re-check any table added since.
6. Smaller ones: `http_max_fields` 1,000 (26.4), `X-ClickHouse-Format` header
   overrides `FORMAT` (26.8), `date_time_input_format` now `best_effort`
   (26.4). Read every "Backward Incompatible Change" section from 26.4 up to
   the target again — this list was made for 26.8.

## Implementation

Two PRs — production must act between them.

1. **Query fix, on 26.3 — built 2026-09-30.** An audit of all 77 SQL joins
   in `crates/` found one site that depends on propagation
   (`ledgers::fetch_transactions`) and one where it is masked by
   `fetch_optional` (`transactions::fetch_detail`); both now read
   `ledgers l FINAL` (same `read_rows` on prod: 24,576). Every other join
   dedups its own side or collapses duplicates later. Guard:
   `crates/api/tests/sql_conventions.rs` (text check, red on both sites
   before the fix). A docker e2e test could not fail on 26.3, which still
   propagates; the difference was shown on local 26.3 vs 26.8 images
   instead (bare join 2 vs 4 rows; with `l FINAL` 2 on both). Patch
   alternative not taken: the 26.8 compatibility setting.
2. **Version bump.** Image tag in `docker-compose.yml`, the Hetzner Ansible
   role and the docs; local run of the CH-gated tests on the new image;
   production recreate in a maintenance window (pages the self-clearing CH
   alarms); rollback = previous tag (keep `text_index_version` and similar
   format settings at the old value until the rollback window closes).

## Acceptance Criteria

- [x] No API query relies on `FINAL` propagating across a `JOIN`; a text
      check guards it (docker e2e cannot fail on 26.3 — see Implementation).
- [ ] Every breaking change from 26.4 up to the target is checked against our
      code and config, with the result written here.
- [ ] Local CH-gated tests pass on the target image.
- [ ] Production runs the target version; ledger 60534355's transaction count
      equals the number of distinct transactions in it.
- [ ] Before/after `read_rows` and memory of the slowest endpoints, measured
      from `system.query_log`, show whether the upgrade paid off.
- [ ] **Docs updated** —
      `docs/architecture/infrastructure/infrastructure-overview.md` (image
      version), `crates/db-clickhouse/README.md` (version note).
