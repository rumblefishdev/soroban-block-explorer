---
id: '0423'
title: 'TEST: behavioural dedup regression harness for RMT read paths (seeded duplicates)'
type: TEST
status: backlog
related_adr: []
related_tasks: ['0420']
tags:
  ['area-api', 'area-clickhouse', 'testing', 'effort-medium', 'priority-medium']
links:
  - crates/db-clickhouse/tests/persist_e2e.rs
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from 0420. Ten of the eleven dedup fixes there are guarded only by
      manual prod queries, which already went stale mid-session. Deliberately
      not solved inside 0420: the useful version needs a test harness, not more
      assertions on SQL strings.
---

# TEST: behavioural dedup regression harness for RMT read paths

## Summary

Task 0420 fixed eleven read paths that returned duplicated rows or inflated
counts from ReplacingMergeTree tables. **Only one of them has a real test.** The
rest were verified by hand against production — evidence that is already stale:
mid-session the duplicate band shifted and one query (F3) stopped showing any
difference between the broken and fixed form. Nothing prevents a future change
from silently reintroducing the whole class.

## Why not just assert on the SQL strings

The tempting cheap option — extract each query builder into a pure function and
assert the string contains `FINAL` / `LIMIT 1 BY` / the semi-join — was
considered and rejected in 0420:

- It proves nothing about behaviour. The F0 fix in 0420 **contained `FINAL` and
  was still catastrophically wrong** (a 19× read regression). A substring
  assertion would have passed it.
- It requires refactoring query construction across six files that were just
  changed — real risk, weak payoff.

What actually needs guarding is the _behaviour_: given duplicate physical rows,
the endpoint returns distinct rows and un-inflated counts.

## Approach

`crates/db-clickhouse/tests/` already runs end-to-end tests against ClickHouse
(`persist_e2e.rs`, `metadata_e2e.rs`, `g9_verdict_routing_e2e.rs`). Reuse that
harness rather than inventing one.

- [ ] Fixture that seeds a table with deliberately duplicated rows (2× and 3×
      copies, matching the shapes measured in prod)
- [ ] One test per fixed read path from 0420: ledgers list, contract
      invocation/event counts, LP chart aggregation, LP list, asset search,
      network totals, tx-detail operations, account balances
- [ ] Each asserts the user-visible invariant — distinct rows / correct count —
      NOT the SQL text
- [ ] Wire into CI

## Acceptance Criteria

- [ ] A seeded-duplicate fixture exists and is reusable
- [ ] Every 0420 fix has a behavioural test that fails if the dedup is removed
- [ ] Tests run in CI and do not depend on production data
- [ ] Cost guard: at least the ledgers list asserts a bounded rows-read figure,
      so a correct-but-19×-more-expensive "fix" cannot pass again

## Notes

Already covered by real tests in 0420, so out of scope here:

- ledgers `dedup_consecutive` (Rust-side collapse) — unit-tested
- `ExplorerTable` colliding row keys — component-tested
