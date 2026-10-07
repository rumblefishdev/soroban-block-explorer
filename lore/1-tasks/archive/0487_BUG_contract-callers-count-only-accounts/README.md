---
id: '0487'
title: 'BUG: contract callers count only accounts — "Unique callers 0" on 97% of contract pages, "—" on 27% of invocation rows'
type: BUG
status: completed
related_adr: []
related_tasks: ['0300', '0331', '0345', '0420', '0586']
tags:
  [backend, api, frontend, clickhouse, contracts, priority-high, effort-small]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found on production while verifying the 0472 deploy: a contract with
      4,593,403 invocations in the window reported 0 unique callers. Root
      cause and blast radius measured against production ClickHouse before
      filing; every number below is measured, not estimated.
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Activated after task 0586 moved every reader onto `contract_activity`
      (#513), which carries both caller columns. Decided (thread 282 A): one
      number — "Unique callers" counts accounts and contracts together.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      Merged as #517, deployed; checked on production 2026-09-30: Unique callers 81 = ClickHouse 81.
---

# BUG: a caller that is a contract is not a caller

## Summary

"Unique callers" counted only accounts and the Invocations tab showed "—" for
a contract caller: 97% of contract pages (16,434 of 16,941) read 0 callers
and 27% of invocation rows had no caller. Decided (282 A): one number,
accounts and contracts together.

## Acceptance criteria

- [x] Unique callers counts contract callers — KALE SAC (`CB23WRDQ…`): the
      deployed API's `recent_unique_callers` is 81 and ClickHouse
      `uniqExact(tuple(caller_id, caller_contract_id))` over the same 7 days is
      81 (accounts alone: 70). The criterion's 8 was the 2026-08-17 window.
- [x] `Caller` renders and links a contract caller instead of `—` — the
      deployed API's 500 latest invocations of `CB23WRDQ…` all carry a `C…`
      caller (one `caller` field since task 0600); the cell links it
- [x] Contract detail and transaction detail both fixed (#517); the
      transaction page's DB path is the archive-down fallback and was not
      exercised live, and that page does not render the caller
- [x] A test pins the account-caller, contract-caller and mixed cases (API
      stats SQL test; web `CallerCell` tests)
- [x] **Docs updated** — canonical SQL, frontend and technical overviews
- [x] **API types regenerated** (#517)

Root cause, measurements, fix and progress are in
[notes/R-task-record.md](notes/R-task-record.md).
