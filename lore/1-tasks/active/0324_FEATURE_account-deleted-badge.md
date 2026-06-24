---
id: '0324'
title: 'Account "deleted" badge for merged accounts'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: ['phase-future', 'effort-small', 'priority-medium', 'api', 'frontend']
links:
  - 'https://stellar.expert/explorer/public/account/GAP7STAMLIHYLII6VXZ5VF3G6WKEEILEKNTACWLSTVCJPBW2TMRHI4LW'
history:
  - date: 2026-06-24
    status: backlog
    who: karolkow
    note: 'Renumbered 0323 -> 0324 (0323 taken by undeployed-sac task). Task created — explorer keeps merged-account rows but does not flag them deleted like stellar.expert.'
  - date: 2026-06-24
    status: active
    who: karolkow
    note: 'Promoted to active to begin implementation.'
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

## Decisions (locked 2026-06-24)

1. **Backend: ClickHouse only.** Prod serves accounts from CH
   (`API_DATASOURCE_ACCOUNTS: 'ch'` in infra/compute-stack). Code default is
   `Pg`, used only as dev/fallback. Implement the derive in
   `queries_ch.rs`; PG path returns the default (`deleted=false`) with a
   ponytail note — not worth dual maintenance.
2. **Derive at query time, no schema change** — but see anchoring below; the
   naive rule is catastrophic, the anchored one is free.
3. **Rule verified** on `GAP7STAM…` → `deleted=true`, `merged_into=GA4N7346…`.
4. **Response surface: bare `deleted: bool` (A)** — dropped `merged_into` /
   `deleted_at_ledger` (decided against the earlier B; keep the contract
   minimal). Single `argMax` over the anchored granule (last op in last-seen
   ledger is a merge-as-source).
5. **Detail endpoint only** (`/accounts/{id}`). No list badging.
   (Earlier claim that stellar.expert badges in lists was unverified/retracted.)

## Implementation Plan

### Step 1: Derive — two-step, anchored on `last_seen_ledger`

An account is `deleted` ⟺ its **last op in its last-seen ledger** is a
`type=8` (account_merge) where it is the `source`. Since
`last_seen_ledger = GREATEST(all appearances)`, any deleting merge sits in that
ledger; `argMax` over `(transaction_id, application_order)` picks the
chronologically-last op _within_ the ledger, so a same-ledger re-create (merge
then `create_account` at higher app order) correctly yields `false`.

```
deleted ⟺ argMax(type = 8 AND source_id = <id>, (transaction_id, application_order))
          over operations_appearances
          WHERE ledger_sequence = <last_seen_ledger>
            AND (source_id = <id> OR destination_id = <id>)
```

Measured on prod (6.2B ops): cross-ledger reopen is common (1.57M pairs,
handled correctly — `last_seen` advances past the merge); **same-ledger**
merge-then-recreate = **0** occurrences, but the app-order anchor closes that
theoretical gap at the same 1-granule cost.

**Critical — must be a SEPARATE query with `last_seen_ledger` as a literal
bind, NOT a JOIN.** `operations_appearances` is `PARTITION BY
intDiv(ledger_sequence, 500000)`, ORDER BY `(ledger_sequence, transaction_id,
application_order)` — no sort key on `source_id`/`type`. Partition pruning only
fires when `ledger_sequence` is a constant. Measured (`EXPLAIN ESTIMATE`, prod):

| variant                         | rows read                      |
| ------------------------------- | ------------------------------ |
| anchored (literal ledger)       | **8 192** (1 granule)          |
| naive (`source_id`+`type` only) | **6 199 823 062** (full table) |

The JOIN form (anchor = column from `accounts`) cannot prune → blew the 3.73 GiB
query memory limit. So: `fetch_account` already returns `id` + `last_seen_ledger`;
a 3rd query `fetch_deleted_status(id, last_seen_ledger)` does the anchored lookup.

ponytail: query time, no column/migration. Materialize only if the extra
8192-row granule read ever matters (it won't).

### Step 2: API (CH path)

- `crates/api/src/accounts/queries_ch.rs` — add `fetch_deleted_status`
  (anchored `argMax` over last-op-in-ledger, literal binds) → `bool`.
- `crates/api/src/accounts/dto.rs` — add `deleted: bool` to `AccountDetailResponse`.
- `crates/api/src/accounts/handlers.rs` — call after `fetch_account`, fill DTO.
  PG branch returns `false`.
- Regenerate API types: `npx nx run @rumblefish/api-types:generate`, commit
  `libs/api-types/src/{openapi.json,generated/}` (CI gate `API types freshness`).

### Step 3: Frontend

- Account detail page — render a `deleted` badge when `deleted=true`.

### Step 4: Docs

- Update `docs/architecture/**` account API contract (per ADR 0032).

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
