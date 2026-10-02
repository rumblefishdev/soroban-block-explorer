---
id: '0611'
title: 'Soroban pool activity: bound the cursor ledger before the window arithmetic'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0374']
tags: ['effort-small', 'priority-low', 'api']
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/405']
history:
  - date: 2026-10-01
    status: backlog
    who: karolkow
    note: 'Spawned from 0374 review of list_soroban_pool_activity.rs.'
  - date: 2026-10-02
    status: backlog
    who: karolkow
    note: 'Renumbered from 0605: the id collided with another task opened the same day.'
---

# Soroban pool activity: bound the cursor ledger before the window arithmetic

## Summary

`fetch_soroban_pool_activity` derives its first ledger window from the
cursor's `ledger_sequence` (`ls - span`, `ls - 1 + span`). The cursor is an
unsigned base64 JSON payload, so a caller can put any `i64` there; release
builds wrap on overflow. The loop still terminates, but a crafted value makes
one request run up to 45 window rounds instead of at most 8 (one of them a
whole-pool read). Debug builds (`bin/local`) panic instead.

## Context

Measured by simulating the loop with wrapping arithmetic (tip 64.7M):
`ledger_sequence` 10^18 → 41 rounds forward, −10^18 → 41 backward,
`i64::MIN`/`i64::MAX` → 45; a real cursor → ≤ 8. The classic activity path
does no arithmetic on the cursor and is not affected.

## Implementation Plan

- Reject a cursor whose `ledger_sequence` is outside `[0, tip]` with the
  extractor's `invalid_cursor` 400.
- Use `saturating_sub` / `saturating_add` for the window bounds.
- Remove the unreachable `(false, None)` start (backward paging always has a
  cursor; `extractors.rs` makes the first page forward).
- Unit test: out-of-range cursor → 400; extreme in-range values → ≤ 8 rounds.

## Acceptance Criteria

- [ ] Out-of-range cursor answers 400 `invalid_cursor`.
- [ ] No overflow in debug or release for any `i64` cursor value.
- [ ] Dead `(false, None)` branch removed.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`:
      N/A unless the error contract text changes.
