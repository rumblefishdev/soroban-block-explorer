---
id: '0585'
title: 'REFACTOR: the parser emits a 0-based operation index (stellar-rpc convention)'
type: REFACTOR
status: backlog
related_adr: ['0059']
related_tasks: ['0372', '0538', '0573']
tags: [xdr-parser, clickhouse, effort-small, priority-medium]
links:
  - crates/xdr-parser/src/operation.rs
  - crates/xdr-parser/src/types.rs
  - crates/db-clickhouse/src/persist/stage/operations.rs
history:
  - date: '2026-09-25'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0372 future work (variant B, deferred by karolkow until
      after 0372's last PR). The tables store the 0-based operation_index
      since 0372; the parser still emits the 1-based Horizon position and the
      writer subtracts one.
---

# The parser emits a 0-based operation index

## Summary

`ExtractedOperation.operation_index` is 1-based (Horizon's
`application_order`), while `transaction_operations.operation_index`,
`pool_operation_amounts.operation_index` and the event ids are 0-based
(ADR 0059, stellar-rpc `operationIndex`). Every consumer converts at its edge:
the writer subtracts one (`checked_sub`, failing the ledger on a 0), the
archive extractor indexes the XDR array with `- 1`, value flow compares with an
offset. Make the parser emit the 0-based index — the one convention the chain
tooling uses — and move the `+ 1` to the single place that needs the 1-based
number: the API wire (`application_order`, the `#op-N` anchor).

## Context

- Found in task 0372 PR 2 review ("why does the table count from 0 when the
  parser counts from 1?"); variant B was written then, as 9 uncommitted files
  on the local branch `refactor/0372-operation-index-0-based` (based on
  0372 PR 2, before PR 4 removed the old tables — it needs rebasing onto
  `develop`; the old-table `+ 1` lines it carries are gone).
- Scope of that diff: parser `op_index = i`, `types.rs` docs, `value_flow.rs`
  `== t.op_index`, `extractors.rs` `rs.get(op.operation_index as usize)` with
  the wire `+ 1`, `stage/operations.rs` without the conversion, fixtures moved
  to 0-based. Unfinished: `operation_tests.rs` assertions (1,2,3 → 0,1,2).
- No schema change, no backfill: the stored values do not change.

## Acceptance Criteria

- [ ] `ExtractedOperation.operation_index` is 0-based; its doc says so
- [ ] No `- 1` / `checked_sub` conversion left between parser and tables
- [ ] API wire unchanged: operation `application_order` / `appearance_id`
      still 1-based (checked against production through the dev proxy)
- [ ] A test that fails if the parser goes back to 1-based
- [ ] **Docs updated** — `types.rs`, xdr-parsing overview if it names the
      convention
