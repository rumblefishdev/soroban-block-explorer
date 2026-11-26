---
id: '0682'
title: 'OPS: data-check exporter beside ClickHouse, for checks that answer with a number'
type: OPS
status: backlog
related_adr: ['0062']
related_tasks: ['0681', '0476']
tags: [priority-medium, effort-medium, monitoring, prometheus, clickhouse]
links:
  - https://github.com/rumblefishdev/soroban-block-explorer/pull/762
history:
  - date: '2026-11-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created from ADR 0062 (PR #762). Builds the mechanism for data checks; 0476 is its first
      check. Starts once 0681's agent runs.
---

# OPS: data-check exporter beside ClickHouse, for checks that answer with a number

## Summary

ADR 0062 turns data checks, SQL against ClickHouse whose answer is a number, into Prometheus
series that the explorer's agent (0681) scrapes. This task adds the exporter that runs them, the
read-only ClickHouse user it reads with, and the rules every check gets, and proves them with the
first check, 0476's stalled `balance_aggregates_mv` refresh.

## Stan teraz

- Done: nothing yet.
- Next: after 0681's agent runs (its Step 2).
- In force: ADR 0062, decision 2 ("The exporter", "Every check exports", "Labels").

## Context

- **The precedent on this ClickHouse** is stellar-prices-api's `rollup-freshness-probe`, on the
  CloudWatch path. Its `mv_drift.rs`, `refresh_waits.rs` and `main.rs` show the traps:
  - `system.tables` filters rows by grant, so a missing grant reads as no rows
    (`MvDriftUnreadable`);
  - `system.view_refreshes` refuses a user without `SELECT` on it (Code 497, an error); for a user
    with it, it lists only the views that user may `SHOW`, so a missing per-view grant reads as no
    rows. `MvRefreshUnreadable` covers both;
  - `last_success_time` is NULL until a view's first success after a restart, and prices-api
    counts `exception != ''` as failing whatever it holds.
- **ClickHouse users** live in `crates/db-clickhouse/users.d/services.xml`, with profiles in
  `profiles.xml` and quotas in `quotas.xml`. Users defined in XML refuse SQL `GRANT` and `REVOKE`.
  The `<no_password/>` users admit loopback, `dev_shared` (admin) among them, so the exporter's
  hardening matters more than its user's grants.

## Implementation Plan

### Step 1: Choose the exporter

A small crate in this repository, or a digest-pinned third-party SQL exporter whose query file
lives here. Either way the queries, the series names and the rules change in one commit. Record
the choice and why in this task.

### Step 2: The ClickHouse user

`metrics_reader` in `users.d/services.xml`, loopback only:

- a profile of its own in `profiles.xml` (e.g. `metrics_read`: `readonly` 1, `max_execution_time`
  of a few seconds), not the shared `read_only` (30 s, used by `api_reader`); under `readonly` 1
  the exporter can't change settings per query;
- a quota in `quotas.xml`, as every service user has;
- grants on exactly what the checks read. For 0476 that is `SELECT ON system.view_refreshes`,
  plus `SHOW TABLES` on `default.balance_aggregates_mv` and `testnet.balance_aggregates_mv`,
  without which the table returns no rows.

### Step 3: The exporter service

- In `docker-compose.prod.yml`, in ClickHouse's network namespace: the image from Step 1 (built
  here, or pinned by digest), `read_only: true`, `cap_drop: [ALL]`, no published port. The agent's
  `data-checks` job scrapes it, and 0681's lifecycle steps name it with the agent.
- Each check exports three series, labelled `check` and `network` (`stellar:pubnet` for
  `default`, `stellar:testnet` for `testnet`):
  - `explorer_check_value`, absent when it can't be computed, never 0. For 0476 that is
    `last_success_time` NULL with an empty `exception`; a non-empty `exception` is a failing value;
  - `explorer_check_last_success_timestamp_seconds`;
  - `explorer_check_visible_objects`.

### Step 4: The rules every check gets

In the explorer's rules file (0681), re-pinned in soroban-infrastructure (`explorer_rules_commit`
/ `explorer_rules_sha256`) before Step 5's drills:

- the check's own threshold;
- its last success older than a few periods;
- its visible objects at 0;
- its last-success and visible-objects series absent while the exporter's `up` is 1, so a series
  renamed at one end pages;
- its value absent for longer than a few refresh periods (e.g. `for: 15m`) while the exporter is
  up, so a view that never succeeds after a restart pages and a restart's short wait doesn't;
- a testnet check at `warning` at most.

The exporter itself being down is 0681's `explorer-target-down`.

### Step 5: 0476 as the first check

- 0476's query for each network (`WHERE database = 'default'`, or `'testnet'`):
  `exception != ''`, or `now() - last_success_time` over about 10 minutes.
- On testnet, fire each rule once into the channel:
  - the threshold, with the view's refresh stopped or the threshold lowered;
  - a stale check, with the query broken on purpose;
  - visible objects at 0, with the testnet view's `SHOW TABLES` grant removed from `services.xml`
    on the box. It is a single-file read-only bind mount, so overwrite it in place, keeping its
    inode, for ClickHouse's reload to see it.

## Acceptance Criteria

- [ ] The exporter choice is recorded with its reason.
- [ ] `metrics_reader` holds only the grants its checks need, under its own profile and quota.
- [ ] 0476's check runs for both networks, and each of its rules has fired once into the channel
      (ADR 0062, rule 5), the visible-objects rule by removing the per-view grant.
- [ ] After a ClickHouse restart through the deploy (whole-project `up -d`), the check has no
      value until the view succeeds and nothing fires for that alone; a view failing from the
      start still reads as failing.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md` (the
      exporter, linking ADR 0062), `docs/architecture/security/clickhouse-rbac.md`
      (`metrics_reader`, its profile and quota) and `docs/runbooks/health.md` (a coverage row and
      a symptom entry for 0476's alert, which its rule's description links).
- [ ] **API types regenerated** — N/A if Step 1 picks a third-party exporter. A crate here adds a
      workspace member (`Cargo.toml`, `Cargo.lock`): then run
      `pnpm nx run @rumblefish/api-types:generate` and commit any diff.
