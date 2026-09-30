---
id: '0594'
title: 'Workflow bottlenecks: follow-through on the 2026-09-29 audit'
type: OPS
status: completed
related_adr: []
related_tasks: ['0525', '0555']
tags: [priority-medium, effort-small, process]
links: []
history:
  - date: 2026-09-29
    status: active
    who: karolkow
    note: 'Task created from the workflow audit; requester: karolkow'
  - date: '2026-09-30'
    status: completed
    who: karolkow
    note: >
      #544, #545, #546 merged 2026-09-29; Rust CI median measured 2026-09-30.
---

# Workflow bottlenecks: follow-through on the 2026-09-29 audit

## Summary

An audit of CI runs, PRs, git history, lore flow and 413 agent sessions
found that the loop is limited by review and reading time, not by build or
CI speed. This task carries the repo-side changes the owner picked.

## Stan teraz

- Done outside the repo: short-answer rule in the global CLAUDE.md;
  `move-split-guard` and a new `rm-guard` hook wired in the user settings.
- Done in the repo: `/pr` requires a `## Verified` section (real-data
  evidence); 0525 names `stage.rs` as the next split; 13 stale backlog tasks
  archived (5 done, 6 superseded, 2 merged into 0447 and 0503).
- Merged 2026-09-29: #544 task-file size ratchet + "Stan teraz" in the
  template; #545 CI split; #546 AWS SDK trim (api −6.5%, indexer −9.6%).
- Memory cleanup (outside the repo): 97 → 47 notes, index 15.2 → 7.6 KB.

## Acceptance Criteria

- [x] A task file over 150 lines cannot grow in a commit; the message says
      to move detail into `notes/`.
- [x] The task template opens with a "Stan teraz" section kept current.
- [x] CI: the swagger-ui step runs only its own test; ClickHouse e2e runs in
      a parallel job; the Rust job's median drops (measured on 5 runs).

## Closing check (2026-09-30, read-only)

Every successful CI run since 2026-09-26, split by whether it has the
separate `Rust (ClickHouse e2e)` job (GitHub Actions API):

| job                   | before (n=41) | after (n=30) |
| --------------------- | ------------- | ------------ |
| Rust (clippy, test)   | 8.6 min       | 4.4 min      |
| Rust (ClickHouse e2e) | —             | 3.0 min      |
| Rust (lambda build)   | 5.1 min       | 5.9 min      |

Medians. The lambda build is now the longest Rust job.
