---
id: '0606'
title: 'Pool activity: a row dropped in enrichment ends paging silently'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0374', '0491']
tags: ['effort-small', 'priority-low', 'api']
links: []
history:
  - date: 2026-10-01
    status: backlog
    who: karolkow
    note: 'Spawned from 0374 review of list_soroban_pool_activity.rs.'
---

# Pool activity: a row dropped in enrichment ends paging silently

## Summary

Both activity paths (classic `fetch_pool_activity`, soroban
`fetch_soroban_pool_activity`) fetch `limit + 1` rows, then pass them to
`enrich_activity`, which drops a row whose transaction or source account does
not resolve. `finalize_page` decides "is there a next page" from the row
count after that, so one dropped row makes the page look like the last one:
`next_cursor` disappears and the history ends without an error.

## Context

Not observed today: 4,118,989 of 4,118,989 soroban transactions resolve
(production, 2026-10-01); the classic path has had this shape since 0491. It
becomes visible the day the transaction tables lag the amount tables.

## Implementation Plan

- Decide "has more" from the pre-enrichment row count (or carry the peek row
  through enrichment separately).
- Log every dropped row with its position.
- Test with a seeded row whose transaction is missing: the next cursor still
  appears.

## Acceptance Criteria

- [ ] A dropped row no longer removes `next_cursor` (both paths).
- [ ] Dropped rows are logged.
- [ ] CH-gated test covering the case.
