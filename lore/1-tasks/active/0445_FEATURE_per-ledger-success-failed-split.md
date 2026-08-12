---
id: '0445'
title: 'FEATURE: per-ledger success/failed split in the ledgers table (read-time, no schema change)'
type: FEATURE
status: active
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

- [ ] Per-ledger successful count exposed on the ledgers list API, nullable
- [ ] Second query, not a JOIN into `fetch_list`; the over-fetch + collapse
      read path is unchanged and still reads ~1.35M rows
- [ ] Aggregate dedups via `uniqExact`, not `FINAL`
- [ ] `read_rows` measured and recorded for a full page
- [ ] Missing aggregate rows render the plain total, never a `0 successful`
      that reads as a total failure
- [ ] Zero-transaction ledger renders without a divide-by-zero or empty dots
- [ ] Home widget and `/ledgers` both covered by the shared table change
- [ ] **Docs updated** — `docs/architecture/database-schema/endpoint-queries-clickhouse/04_get_ledgers_list.sql`
      and the endpoint contract per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`
