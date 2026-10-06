---
id: '0628'
title: 'REFACTOR: no wildcard over TransactionMeta versions in the parser'
type: REFACTOR
status: active
related_adr: []
related_tasks: ['0604', '0573', '0393']
tags: ['xdr-parsing', 'effort-small', 'priority-medium']
links:
  - crates/xdr-parser/src/meta.rs
  - crates/xdr-parser/src/operation.rs
  - crates/xdr-parser/src/invocation.rs
  - crates/xdr-parser/src/ledger_entry_changes.rs
history:
  - date: 2026-10-07
    status: active
    who: claude
    note: >
      Filed and started on request after 0604, which removed the wildcards in
      the event model and the invocation tree and left two in the parser.
---

# No wildcard over TransactionMeta versions in the parser

## Summary

A `_ =>` arm over `TransactionMeta` versions turns a future meta version into
silently missing data instead of a compile error. `meta.rs` exists to keep
those matches exhaustive in one place (task 0393). Two wildcards are left in
production code: `operation.rs`'s `soroban_return_value` (an exact copy of
the one in `invocation.rs`) and `extract_ledger_entry_changes`. Behaviour
does not change: today's versions take the same arms.

## Implementation

- `meta::soroban_return_value`, exhaustive; the copies in `operation.rs` and
  `invocation.rs` call it.
- `extract_ledger_entry_changes` names V0–V2 instead of `_ => {}`.
- `meta.rs`'s module doc: the "still carry wildcard arms" paragraph made true.

## Acceptance Criteria

- [ ] No `_ =>` arm over `TransactionMeta` versions in `crates/xdr-parser/src`.
- [ ] One `soroban_return_value`, in `meta.rs`.
- [ ] All tests pass unchanged; golden event output untouched.
- [ ] PR title ends with `[refactor]`.
- [ ] Docs: `meta.rs` module doc updated; `docs/architecture/**` N/A — no
      change in what the parser extracts.
