---
id: '0324'
title: 'Account "deleted" badge for merged accounts'
type: FEATURE
status: completed
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
  - date: 2026-06-24
    status: completed
    who: karolkow
    note: >
      Implemented CH-only derived `deleted` flag (argMax last-op-in-ledger,
      anchored on last_seen_ledger → 1 granule) + red `Chip` badge on account
      detail. DTO: bare `deleted: bool`. 2 new web tests (91 green), 18 api
      tests green, types regenerated, docs Statement C added. Recovered from a
      wrong-worktree commit that hit shared develop (rewound + force-pushed).
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
`type=8` (account*merge) where it is the `source`. Since
`last_seen_ledger = GREATEST(all appearances)`, any deleting merge sits in that
ledger; `argMax` over `(transaction_id, application_order)` picks the
chronologically-last op \_within* the ledger, so a same-ledger re-create (merge
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

- [x] API `/accounts/{id}` returns `deleted` for merged accounts (validated on
      `GAP7STAMLIHYLII6VXZ5VF3G6WKEEILEKNTACWLSTVCJPBW2TMRHI4LW` → `deleted=true`
      against prod CH).
- [x] Re-created (funded-after-merge) accounts return `deleted=false` (validated
      on the merge destination `GA4N7346…` → `deleted=0`).
- [x] Frontend shows a `deleted` badge on merged accounts (red `error` Chip,
      verified via local stub render — screenshot in PR).
- [x] **Docs updated** — `docs/architecture/database-schema/endpoint-queries-clickhouse/06_get_accounts_by_id.sql`
      gains Statement C (the derived flag).
- [x] **API types regenerated** — `openapi.json` + `generated/` carry `deleted`.

## Implementation Notes

- **CH-only**, one new query `queries_ch::fetch_deleted_status(id, last_seen)`
  → `bool`. PG `fetch_deleted_for_source` branch returns `false` (prod = CH).
- Rule: `argMax(type=8 AND source_id=id, (transaction_id, application_order))`
  over `operations_appearances WHERE ledger_sequence = last_seen AND (source_id
= id OR destination_id = id)`. Anchored on the literal `last_seen_ledger` →
  1 partition granule (~8K rows) vs a 6.2B-row full scan (measured).
- DTO: single `deleted: bool` on `AccountDetailResponse`.
- Frontend: shared `Chip color="error" dot size="sm"` (same primitive as the
  Failed-status chip), rendered beside the "Account" title.
- Tests: 2 added (`AccountDetailPage.test.tsx`) — badge shown when deleted,
  hidden when live. 91 web tests green; 18 api tests green.

## Design Decisions

### From Plan

1. **Derive at query time, no schema/migration** — the cost concern is solved
   by anchoring on `last_seen_ledger`, not by materializing a column.
2. **CH-only** — prod serves accounts from CH; PG path stubs `false`.

### Emerged

3. **Bare `deleted: bool`** (dropped the planned `merged_into` /
   `deleted_at_ledger`) — user trimmed the contract mid-task. Query simplified
   accordingly.
4. **`argMax` last-op-in-ledger rule** (replaced the simpler `count()` EXISTS)
   — closes the same-ledger merge-then-recreate edge at identical 1-granule
   cost. Measured zero such cases across 6.2B ops, but free to be correct.
5. **Reused the design-system `Chip`** rather than a bespoke badge — no Figma
   exists for this; `error` palette = same red as the Failed-status chip.

## Issues Encountered

- **Wrong worktree.** Implemented + committed in the main (`develop`) worktree
  instead of the feature worktree; the feature commit landed on shared
  `develop` (local + remote). Recovered: moved the commit to
  `feat/0324_account-deleted-badge`, `git reset --hard` + `--force-with-lease`
  rewound `develop` to `1bcb2822`, then PR'd properly. No other branch had
  pulled the bad commit.
- **CH JOIN memory blow-up.** Anchoring the derive via a JOIN on `accounts`
  (`ledger_sequence = a.last_seen_ledger`) cannot partition-prune (anchor is a
  column from the other table) → scanned 6.2B rows, tripped the 3.73 GiB query
  memory limit. Fixed by a dedicated 2nd query with the literal bind.
- **Local backend can't run.** API CH client reads its mTLS cert from the AWS
  Secrets Manager Lambda Extension (Lambda-only). For the local screenshot the
  API was stubbed (real DTO shape); the `deleted` value itself was verified
  against prod CH separately.

## Notes

- Optional extension: also expose `merged_into` / `deleted_at_ledger` so the
  badge can link to the destination account, matching stellar.expert.
- Live-edge lag: accounts merged after our max ledger (e.g. GABO3MI4... merged
  2026-06-24 while max ledger was 2026-06-15) only flag once that ledger is
  ingested. Expected, not a bug.
