---
id: '0588'
title: 'REFACTOR: retire the endpoint-query SQL reference set — the Rust queries and their ClickHouse tests are the reference'
type: REFACTOR
status: active
related_adr: ['0060']
related_tasks: ['0207', '0478', '0587']
tags: [docs, clickhouse, api, cleanup, effort-small, priority-medium]
links: []
history:
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Decided on review of 0478's PR, which built a CI gate to keep the set
      parseable. The set is a hand-kept copy of the Rust queries; the gate
      could only prove the copies plan, never that they match the Rust, while
      the Rust queries already run against the canonical schema in CI. 0478's
      PR closed unmerged; 0478 and 0587 superseded by this task.
---

# REFACTOR: retire the endpoint-query SQL reference set

## Summary

`docs/architecture/database-schema/endpoint-queries-clickhouse/` holds one
hand-written `.sql` file per public endpoint plus a runner script. Task 0207
wrote it in May as the ClickHouse pilot's reference set, before the Rust API
existed on ClickHouse. The Rust queries have long been the source of truth,
and the files now trail them. This task deletes the set and moves the few
notes worth keeping next to the Rust queries they explain.

## Context — measured 2026-09-28

- **Maintenance cost.** 92 of the 317 commits that touched `crates/api` in
  the last 90 days also had to edit these copies (0374, 0491, 0199, 0580 …).
- **Drift anyway.** Before 0478, seven files did not even parse. After it, `06`
  and `23` were still stale: `23` documents the `JOIN accounts FINAL` that
  0354 removed from the Rust.
- **The real check already exists.** Every API module (accounts, assets,
  contracts, ledgers, liquidity_pools, network, nfts, search, transactions)
  has ClickHouse-gated tests (`decode_smoke` / `ch_tests`). CI runs them in
  the "ClickHouse e2e" step against the canonical schema. They check the
  queries the API actually runs; a gate on the copies checks only the copies.
- **Exact executed SQL** is recorded in production `system.query_log`
  (`chq`), with no maintenance.
- **Readers.** Code comments of the form "wire shapes mirror canonical SQL"
  (~20 files in `crates/api`), the `compare-with-stellar-api` skill, one line
  in `backend-overview.md`, and an allow rule in `.claude/settings.json`.

## Implementation Plan

1. Record the decision as ADR 0060.
2. Move worthwhile header knowledge into comments beside the Rust query
   functions: index and `FINAL` rationale, and traps such as the Decimal tuple
   comparison. Only where it says something the code does not; drop anything
   the Rust already states or that is stale.
3. `git rm` the directory.
4. Update every reader:
   - the Rust comments pointing at the files;
   - the `compare-with-stellar-api` skill: the Rust query plus
     `system.query_log` for the executed SQL;
   - `backend-overview.md`;
   - the `.claude/settings.json` runner rule;
   - any other live doc that links the directory.
5. Leave history alone: archived tasks and ADRs keep their references.

## Acceptance Criteria

- [ ] ADR 0060 accepted, with the evidence above
- [ ] The directory is gone and no live (non-archive) file references it —
      shown by a `git grep`
- [ ] Every note kept from the headers sits beside its Rust query; the PR
      lists what was kept and what was dropped
- [ ] `compare-with-stellar-api` works without the set
- [ ] **Docs updated** — `docs/architecture/**` no longer links the set;
      ADR 0060 records why
- [ ] **API types regenerated** — N/A unless a doc comment on a DTO or handler
      reaches the OpenAPI spec; if it does, regenerate and commit
