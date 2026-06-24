---
id: '0323'
title: 'Account "deleted" badge for merged accounts'
type: FEATURE
status: backlog
related_adr: []
related_tasks: []
tags: ['phase-future', 'effort-small', 'priority-medium', 'api', 'frontend']
links:
  - 'https://stellar.expert/explorer/public/account/GAP7STAMLIHYLII6VXZ5VF3G6WKEEILEKNTACWLSTVCJPBW2TMRHI4LW'
history:
  - date: 2026-06-24
    status: backlog
    who: karolkow
    note: 'Task created — explorer keeps merged-account rows but does not flag them deleted like stellar.expert.'
---

# Account "deleted" badge for merged accounts

## Summary

stellar.expert shows a `deleted` badge on accounts removed from the ledger via
`account_merge`. Our explorer keeps the account row but renders it as a normal,
live account — no indication it was merged/deleted. Derive a `deleted` status
from data we already ingest and surface it in the API + a frontend badge.

## Context

Investigated 2026-06-24. Findings:

- We DO NOT delete account rows on merge — the row persists with its
  last-seen state (confirmed in `crates/indexer/src/handler/persist/write.rs`,
  no deletion path).
- We DO ingest `account_merge` ops: `operations_appearances` stores them as
  `type=8` — **6,073,407** rows present. Columns: `source_id` (the deleted
  account), `destination_id` (the `into` account), `ledger_sequence`.
- `accounts` table has NO deleted/merged column:
  `id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain`.
- API `/accounts/{id}` is a plain `SELECT ... WHERE account_id=$1`
  (`crates/api/src/accounts/queries.rs`, handler `crates/api/src/accounts/handlers.rs`)
  — no deleted concept.

Verified on a real merged account in our window:
`GAP7STAMLIHYLII6VXZ5VF3G6WKEEILEKNTACWLSTVCJPBW2TMRHI4LW` — Horizon returns
404 (truly deleted), but we still hold the row and show it live.

Scope note: only Soroban-era (ledger ≥ 50457424, 2024-02-20) merges are
ingested. Accounts merged before that window never appear at all — out of
scope here.

## Implementation Plan

### Step 1: Derive `deleted` (no schema change for MVP)

An account is `deleted` ⟺ it is the `source_id` of a `type=8` op AND that
merge is its latest lifecycle event (handle key re-creation: a merged key can
be funded again later → NOT deleted).

Minimal correct rule:

```
deleted = (max ledger_sequence where type=8 AND source_id = a.id)
          >= a.last_seen_ledger
```

i.e. no activity after the last merge. Compute on-the-fly in the account
lookup query via a join/subquery on `operations_appearances`.

ponytail: derive at query time, NO new column / migration. Add a materialized
`deleted` flag (or `deleted_at_ledger`) on `accounts` only if this query
measurably slows the account endpoint.

### Step 2: API

- `crates/api/src/accounts/queries.rs` — add the derived `deleted` (and
  optionally `merged_into` account_id + `deleted_at_ledger`) to the account
  lookup.
- `crates/api/src/accounts/handlers.rs` — add field(s) to the response DTO.
- Regenerate API types: `npx nx run @rumblefish/api-types:generate`, commit
  `libs/api-types/src/{openapi.json,generated/}` (CI gate `API types freshness`).

### Step 3: Frontend

- Account page — render a `deleted` badge when `deleted=true`, ideally with
  "merged into <account>" link. Match stellar.expert affordance.

### Step 4: Docs

- Update `docs/architecture/**` if the account API contract is described there
  (per ADR 0032).

## Acceptance Criteria

- [ ] API `/accounts/{id}` returns `deleted` for merged accounts (validated on
      `GAP7STAMLIHYLII6VXZ5VF3G6WKEEILEKNTACWLSTVCJPBW2TMRHI4LW` → `deleted=true`).
- [ ] Re-created (funded-after-merge) accounts return `deleted=false`.
- [ ] Frontend shows a `deleted` badge on merged accounts.
- [ ] **Docs updated** — account API contract docs under `docs/architecture/**`
      if described there; else `N/A — reason`.
- [ ] **API types regenerated** — touches `crates/api/**` → run
      `npx nx run @rumblefish/api-types:generate`, commit the diff.

## Notes

- Optional extension: also expose `merged_into` / `deleted_at_ledger` so the
  badge can link to the destination account, matching stellar.expert.
- Live-edge lag: accounts merged after our max ledger (e.g. GABO3MI4... merged
  2026-06-24 while max ledger was 2026-06-15) only flag once that ledger is
  ingested. Expected, not a bug.
