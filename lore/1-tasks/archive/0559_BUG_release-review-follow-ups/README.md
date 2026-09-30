---
id: '0559'
title: 'BUG: release review follow-ups — keyboard access to balance changes, cursor snapshot, silent reserve layout, stale node_modules'
type: BUG
status: completed
related_adr: []
related_tasks: ['0540', '0547', '0548', '0374', '0554']
tags:
  [
    frontend,
    api,
    xdr-parsing,
    tooling,
    accessibility,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: '2026-09-16'
    status: active
    who: karolkow
    note: >
      Task created from the automated review of release PR #453
      (production-2026.09.16-1). Each finding was verified against the code
      before it was accepted; the documentation-only findings landed directly
      on develop, these five change behaviour and go through a PR.
  - date: '2026-09-16'
    status: active
    who: karolkow
    note: >
      Steps 1 and 2 (the `+N` popover and the visible external-management
      sentence) declined in review after a local walkthrough and reverted
      (7cfdbf76, ecfe59ea); both UI elements stay as they were. Steps 3–5 kept.
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      Steps 3–5 merged (#461) and live; steps 1–2 declined in review. Closed 2026-09-30.
---

# BUG: release review follow-ups

## Summary

Five findings from the review of release PR #453 that survived verification
against the code and change behaviour: two keyboard-access gaps in the
frontend, a cursor built from a different snapshot than the page it closes, a
silent reserve-layout refusal in the pool parser, and a post-checkout hook that
leaves an out-of-date `node_modules` in place.

## Acceptance Criteria

- [ ] ~~`+N` opens a keyboard-reachable list of every change~~ — declined in
      review 2026-09-16, reverted; the hover tooltip stays
- [ ] ~~The external-management warning is visible without hover~~ — declined
      in review 2026-09-16, reverted; the chip tooltip stays
- [x] The assets-list cursor encodes the rank the key query sorted by; test
      covers a hydration value that differs from the key value
- [x] A known reserve key with an unreadable value logs an error naming the
      pool; test covers the key rule (the log line itself is not captured —
      the crate has no tracing test harness)
- [x] Branch checkout with an out-of-date `node_modules` re-syncs it; an
      up-to-date tree stays a fast no-op (measured 0.9 s)
- [x] **Docs updated** — N/A: no endpoint, schema, pipeline step or data
      contract changes; the cursor keeps its `(holder_rank, id)` shape
- [x] **API types regenerated** — run; the diff is empty

The plan, implementation notes and decisions are in
[notes/R-task-record.md](notes/R-task-record.md).
