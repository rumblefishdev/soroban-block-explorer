---
id: '0445'
title: 'FEATURE: per-ledger success/failed split in the ledgers table (read-time, no schema change)'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0171', '0420']
tags:
  [backend, api, frontend, clickhouse, ledgers, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/365'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment: "if you have the
      total tx count you can probably show here #count (#success % | #failed
      %)". Not covered by 0171 — that task was about account/contract counts and
      was archived as superseded by 0420. Scoped here to a bounded window,
      because an all-time total is a different (and much larger) piece of work.
  - date: '2026-08-12'
    status: backlog
    who: karolkow
    note: >
      Re-scoped. The original reading — a 24h network-wide aggregate on the
      transactions list header — did not match the report: the attached
      screenshot points at the TX Count column of the home "Latest Ledgers"
      table, so "here" means a PER-LEDGER split, one value per row. The 24h
      aggregate was dropped rather than deferred — nobody asked for it, it
      exists only as an artefact of the misreading, and the window-bounded
      scan reasoning behind it is reconstructible from the TPS query in
      `crates/api/src/network/queries.rs` if it is ever wanted.
      The old `## Out of scope` section was also wrong on the facts: it claimed
      a per-ledger split needs either an indexer counter table or a
      materialised column plus a historical backfill. Read-time aggregation
      over the sequence range of the page on screen was never considered, and
      measures at 176 KiB / 5 ms — no schema change, no backfill.
  - date: '2026-08-12'
    status: active
    who: karolkow
    note: 'Activated for implementation.'
  - date: '2026-08-12'
    status: completed
    who: karolkow
    note: >
      Implemented in 3 commits on `feat/0445_per-ledger-success-failed-split`
      (PR 392): 11 files, +325/-16. Backend + regenerated API types, docs,
      frontend. 1 new component + 4 new tests; no existing test modified.
      228 Rust tests and 232 web tests pass, typecheck and lint clean.
      SQL validated directly against production ClickHouse and cross-checked
      with Horizon. NOT deployed — the issue stays open until it is, per the
      close-at-deploy convention. The live page was never opened in a browser;
      see Issues Encountered.
---

# FEATURE: per-ledger success/failed split in the ledgers table

## Summary

Split the `TX Count` column of the ledgers table into successful and failed
counts, one pair per ledger row — e.g. `● 280  ● 85`. The column is rendered by
the shared `LedgersTable`, so a single change covers both the home "Latest
Ledgers" widget and the `/ledgers` list page.

Computed at read time from `transactions.successful`, which is already on the
row. No new column, no backfill, no parser change.

## Why read-time, not a stored column

Measured on production (2026-08-12, `chq`):

- One aggregate over the sequence range of a 10-ledger page costs
  **16,384 read_rows / 176 KiB / 5 ms**. The read is granule-bound, so a
  25-ledger page costs the same.
- The home widget polls at ~5.5s (`web/src/api/polling.ts`), so this is
  ~115 MiB/h per open tab against a 100 GB/h quota. Not material.

A stored column would buy nothing at this price and would cost a schema
change plus a 13.4M-ledger backfill.

## Two queries, not a JOIN

`fetch_list` is tuned for `optimize_read_in_order` — over-fetch ×3 then collapse
in Rust, because `FINAL` measured 26M rows and `LIMIT 1 BY` 4.5M rows against
1.35M for the current shape (0420, see the comment block at
`crates/api/src/ledgers/queries.rs:200-240`). Attaching a subquery or JOIN there
risks that plan.

Run a second query instead, after the page is deduped, keyed on the page's
min/max sequence. The codebase already uses this two-step shape
(`ch::fetch_tx_list_aggregates`):

```sql
SELECT ledger_sequence,
       uniqExactIf(application_order, successful) AS succ
FROM transactions
WHERE ledger_sequence BETWEEN ? AND ?
GROUP BY ledger_sequence
```

## Dedup

`transactions` is a `ReplacingMergeTree`, so aggregate on
`uniqExactIf(application_order, …)` rather than `countIf`, and never `FINAL`
(0420 measured 19x read amplification on a comparable read).

Measured nuance worth keeping: sampling 7,000 ledgers across three ranges found
**zero** duplicate rows in `transactions` — unlike `ledgers`, where ~12.8M
sequences carry 2 physical rows. The dedup here is defensive, not a fix for an
observed defect.

## Data verified before scoping

- `ledgers.transaction_count == successful + failed` on **3,003 ledgers**
  sampled across three ranges (50.45M, 57.0M, 63.8M) — zero mismatches. So the
  API needs one new field; the frontend derives failed as `total − successful`.
- Cross-checked against Horizon on ledgers 63903902 and 63903903: succ/fail
  354/202 and 392/293, exact match. Horizon itself carries no total field —
  only `successful_transaction_count` and `failed_transaction_count`, with the
  total derived (`crates/audit-harness/src/bin/horizon-diff.rs:146`).

## Display

Two coloured absolute numbers on one line, no percentages:

```
     TX Count
   ● 280  ● 85
```

This fits the existing 110px right-aligned column
(`web/src/pages/ledgers/LedgersTable.tsx:71`) with no layout change, and
matches the only comparable explorer that shows the split at all — StellarChain
renders exactly `● 304  ● 47` in its ledger list. (stellar.expert shows no
transaction count on its ledger page.)

Deliberate deviation from the literal request, which asked for
`#count (#success % | #failed %)`: at ~450 tx per ledger the percentage is
noise, while two adjacent numbers read at a glance. Rename the column header to
`Transactions` — it is no longer a single count.

## Scope

1. `LedgerListItem`: add `successful_transaction_count`, **nullable**.
2. `fetch_list` (`crates/api/src/ledgers/queries.rs`): second query as above,
   merged onto the deduped page by sequence.
3. `LedgersTable` cell + header rename. `null` renders the plain total with no
   split — never a derived "100% failed" from missing rows.
4. Ledger detail summary (`web/src/pages/ledgers/LedgerSummary.tsx:129`) gets
   the same treatment via `LedgerDetailRow`.

## Out of scope (deliberately)

**Network-wide 24h totals** — a different unit of measurement (whole network
over a time window, one value per page) on a different screen. Considered and
dropped, not deferred: no request behind it.

**All-time totals** — still needs a rollup; a counter table appended by the
indexer needs exactly-once semantics under retry. Not planned.

## Acceptance criteria

- [x] Per-ledger successful count exposed on the ledgers list API, nullable
- [x] Second query, not a JOIN into `fetch_list` — the over-fetch + collapse
      read path is byte-identical in the diff. The ~1.35M row figure itself was
      **not** re-measured; unchanged code is the evidence, not a fresh reading.
- [x] Aggregate dedups via `uniqExact`, not `FINAL`
- [x] `read_rows` measured and recorded — 16,384 read_rows / 176 KiB / 5 ms for
      a 10-ledger page; granule-bound, so a 25-ledger page measured the same
- [x] Missing aggregate rows render the plain total, never a `0 successful`
      that reads as a total failure
- [x] Zero-transaction ledger renders without a divide-by-zero or empty dots —
      no percentages are computed at all, so there is no division; the 10 such
      ledgers in the whole table have no `transactions` rows, so they take the
      null path and render a plain `0`
- [x] Home widget and `/ledgers` both covered by the shared table change
- [x] **Docs updated** — both `04_get_ledgers_list.sql` and
      `05_get_ledgers_by_sequence.sql` per ADR 0032
- [x] **API types regenerated** — `openapi.json` + `generated/` committed
      alongside the API change

## Implementation Notes

Three atomic commits on `feat/0445_per-ledger-success-failed-split` (PR 392):

| Commit     | Scope                                                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `b476e07e` | `crates/api/src/ledgers/{dto,queries,handlers}.rs` + regenerated `libs/api-types` (same commit — CI gate `API types freshness`) |
| `69286dde` | `docs/architecture/.../0{4,5}_get_ledgers_*.sql`                                                                                |
| `c77d9963` | `TransactionCounts.tsx` + test, wired into `LedgersTable` and `LedgerSummary`                                                   |

Backend: `fetch_successful_counts` (aggregate over a sequence range) plus
`attach_successful_counts` (fills the deduped page in place). The detail path
calls the same function one ledger wide.

Frontend: one new component, `TransactionCounts`, consumed by both the shared
ledgers table and the detail summary. No existing test was modified; 4 were
added.

### Data verified before and after

- SQL run on production exactly as committed — the `successful_count` alias
  does not collide with the `successful` column.
- Ledger 63903902 → 354 successful / 202 failed; Horizon returns the same.
- `transaction_count == successful + failed` on 3,003 ledgers across three
  ranges; 0 mismatches.
- 10 ledgers out of 13,458,693 carry `transaction_count = 0`. An anti-join over
  a 20,000-ledger range found no ledger with a positive count but no rows.

## Design Decisions

### From Plan

1. **Two coloured absolute numbers, no percentages.** At ~450 transactions per
   ledger the percentage is noise; two adjacent counts read at a glance and fit
   the existing 110px column.
2. **Nullable field.** `null` (no `transactions` rows) is not `0` (everything
   failed). The counts come from two different tables and diverge during a
   backfill window, which is exactly when a defaulted `0` would assert a
   failure that never happened.
3. **Second query, not a JOIN.** The list read is tuned for
   `optimize_read_in_order`; attaching an aggregate risks that plan.

### Emerged

4. **One shared `TransactionCounts` component** instead of editing the two call
   sites separately, as the scope implied. The detail summary and the table
   need identical semantics, including the fallback — duplicating that logic
   would have let the two drift.
5. **Column header renamed `TX Count` → `Transactions`.** Not in scope, but the
   cell no longer holds a single count and the old header would misdescribe it.
6. **`Math.max(0, total - successful)`** on the failed count. The two numbers
   come from different tables; a negative render would be nonsense if they ever
   drift. Cheap, and the clamp is documented at the call site.
7. **`i32::try_from(...).unwrap_or(i32::MAX)`** for the `uniqExactIf` `u64`.
   `as i32` would silently wrap; the ceiling is unreachable in practice but
   the cast should not be the thing that lies.
8. **Detail path takes a second round trip** rather than a scalar subquery on
   the header read. A subquery returns `0` for a ledger with no rows, which is
   indistinguishable from a total failure — the exact case decision 2 exists to
   preserve.

## Issues Encountered

- **Worktree package resolution.** `tsc` in a worktree resolved
  `@rumblefish/api-types` to the MAIN checkout (the worktree's `node_modules` is
  a symlink to it), so the regenerated field was invisible and typecheck failed
  with `Property 'successful_transaction_count' does not exist`. Fixed with a
  worktree-local `web/node_modules/@rumblefish/api-types` symlink; the shared
  `node_modules` was deliberately left untouched. Gitignored, not a code change.
  CI checks out a branch normally and is unaffected.

- **Vite loads no env file in a worktree, so the page was never opened.** The
  app dies at startup on `VITE_API_BASE_URL is not set`. Probed from inside
  `web/vite.config.ts`: `root`, `envDir` and `configFile` all resolve to the
  worktree's `web/`, the file is found, and `configResolved` shows
  `config.env` DOES contain `VITE_API_BASE_URL` — yet the value never reaches
  `import.meta.env` in the served module. The loss happens after
  `configResolved`, not during env-file lookup, so setting `envDir` explicitly
  would not help. Unrelated to this task and left unfixed; it blocks browser
  verification of any frontend change made in a worktree. **No lore task yet.**

- **Even with that fixed, the split could not have been seen.** The dev proxy
  targets the deployed API, which does not carry the field until this ships, so
  the page would have exercised only the fallback branch.

- **`lore-framework_set-task` resolved 0445 to its pre-rename path**
  (`backlog/0445_FEATURE_transaction-totals-success-failed-split.md`), which
  exists on no branch, and wrote the symlink outside this worktree. Pointed
  `current-task.md` at the real file by hand.

## Future Work

None arising from this task. The Vite env defect above is incidental
infrastructure, not a follow-up to this feature — it needs its own task if it
is to be tracked.
