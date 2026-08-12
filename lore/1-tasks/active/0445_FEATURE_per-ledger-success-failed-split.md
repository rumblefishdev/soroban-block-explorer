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
  - date: '2026-08-12'
    status: completed
    who: karolkow
    note: >
      Reworked after a five-agent review (correctness, simplification,
      adversarial, requirements audit, UX) on the full branch diff. Four real
      defects fixed — unbounded aggregate range, 500 on a degradable failure,
      an over-generalised cost measurement, and a display carrying meaning in
      colour alone while dropping the total the report asked for — plus a
      broken Tier-1 SQL gate. The display was redesigned to total + failure
      rate; see `## Rejected`. Tests 4 → 9. Details in Issues Encountered.
  - date: '2026-08-12'
    status: active
    who: karolkow
    note: >
      Back to `active`, reversing the premature archive. The work is finished
      and the two `completed` entries above stand as the record of it, but the
      repo archives at DEPLOY, not at merge, and status-only moves belong
      straight on `develop` rather than riding a feature branch — this one was
      about to arrive as an `active/` → `archive/` rename inside PR 392 and
      collide with whatever `develop` does with the same file. Archive it after
      the deploy, on `develop`, together with the #365 close.
---

# FEATURE: per-ledger success/failed split in the ledgers table

## Summary

Show each ledger's transaction total with its failure rate underneath —
`412` over `22.6% failed`. Rendered by the shared `LedgersTable`, so one change
covers both the home "Latest Ledgers" widget and the `/ledgers` list page, and
by the same component on the ledger detail summary.

Computed at read time from `transactions.successful`, which is already on the
row. No new column, no backfill, no parser change.

## Why read-time, not a stored column

Measured on production (2026-08-12, `chq`), `index_granularity` 8192:

| page                             | read_rows | bytes   | ms  |
| -------------------------------- | --------- | ------- | --- |
| 10 ledgers (home widget)         | 16,384    | 160 KiB | 6   |
| 101 ledgers (`MAX_LIMIT` + peek) | 49,152    | 480 KiB | 19  |

So the cost is flat only while the page fits two granules; the widest supported
page is 3x that. The binding quota is **`read_rows` (2e9/h), not bytes**: at the
widget's ~5.5s cadence (~654 requests/hour) one open tab draws ~10.7M
read_rows/hour, leaving headroom for roughly 190 concurrent tabs on this query.
The bytes figure (~115 MiB/h/tab against 100 GB/h) is ~5x looser and measuring
only it would have been the wrong reassurance.

A stored column would buy little at this price and would cost a schema change
plus a 13.4M-ledger backfill.

## Two queries, not a JOIN — and the read guard

`fetch_list` is tuned for `optimize_read_in_order` — over-fetch ×3 then collapse
in Rust, because `FINAL` measured 26M rows and `LIMIT 1 BY` 4.5M rows against
1.35M for the current shape (0420). Attaching a subquery or JOIN there risks
that plan, so the counts come from a second query.

The second query must carry the same **read guard** as the shape it copies,
`ch::fetch_tx_list_aggregates`: an explicit key list plus a partition prune. A
`BETWEEN min AND max` span looks equivalent on a contiguous page and is not —
`persist::writer` commits `transactions` before the `ledgers` marker, so an
aborted partition leaves orphan transaction rows with no ledger row, and a page
straddling that hole sweeps all of them. Tasks 0243/0386 were quota outages in
exactly that shape. Measured: the guarded form reads the same 16,384 rows on a
contiguous page, so it is free.

```sql
SELECT ledger_sequence, countIf(successful) AS successful_count
FROM (
    SELECT ledger_sequence, application_order, successful
    FROM transactions
    WHERE ledger_sequence IN (<page sequences>)
      AND intDiv(ledger_sequence, 500000) IN (<partitions>)
    LIMIT 1 BY ledger_sequence, application_order
)
GROUP BY ledger_sequence
```

## Dedup

`LIMIT 1 BY` then `countIf` — the house idiom for a `ReplacingMergeTree`, the
same collapse the TPS query in `network::queries` uses. Never `FINAL` (0420: 19x
read amplification), and not `uniqExact`, which builds a per-group hash set —
`contracts::queries` records a measured OOM at 3.73 GiB from that on a wide
window, and this function is the obvious template for any wider variant.

Measured nuance worth keeping: sampling 7,000 ledgers across three ranges found
**zero** duplicate rows in `transactions` — unlike `ledgers`, where ~12.8M
sequences carry 2 physical rows. The dedup here is defensive, not a fix for an
observed defect.

## Failure degrades, it does not 500

Both call sites swallow an aggregate error to `None` and log a warning. The wire
type is nullable, the frontend has a tested branch for "no split available", and
losing the whole ledgers list — with it the polled home widget — over a
decorative aggregate would be the wrong trade.

## Data verified before scoping

- `ledgers.transaction_count == successful + failed` on **3,003 ledgers**
  sampled across three ranges (50.45M, 57.0M, 63.8M) — zero mismatches. So the
  API needs one new field; the frontend derives failed as `total − successful`.
- Cross-checked against Horizon on ledgers 63903902 and 63903903: succ/fail
  354/202 and 392/293, exact match. Horizon itself carries no total field —
  only `successful_transaction_count` and `failed_transaction_count`, with the
  total derived (`crates/audit-harness/src/bin/horizon-diff.rs:146`).

## Display

Two lines, mirroring `TransactionTime` — the other two-line cell in this table,
whose row height (`EXPLORER_TABLE_ROW_HEIGHT_TALL`, 56px) already covers it:

```
   Transactions
            412
    22.6% failed     ← tertiary; error colour only above 50%
```

The total stays the primary value. It is the magnitude a ledgers list exists to
answer and the only comparable number in the row, so it keeps the single right
edge the column had, with `tabular-nums` because Satoshi digits are proportional
(`0` is 2.1x the width of `1`).

The rate rather than a second count, because the rate is the informative half.
Measured on production 2026-08-12: the per-ledger failure rate runs 13.9% (p05)
→ 26.5% (median) → 53% (p95), max 87%. A ~4x swing is not something a reader can
infer, which is what the original request asked for and what an earlier version
of this task wrongly dismissed as "noise at ~450 tx per ledger".

Nothing is carried by colour alone: the text reads identically in greyscale,
under any colour-vision deficiency, and to a screen reader via an `aria-label`
on the cell. Error colour is reserved for rates above 50% — roughly the top 5%
of ledgers — because failures are this chain's steady state and colouring every
row red would make red mean nothing, while strobing the live widget every 5.5s.

Column widened 110 → 120px: at the 720px table floor `tableLayout: fixed` pins
the declared width and the cell clips rather than ellipsising, and the second
line needs ~66px worst case against a 78px content box.

Where the two sources disagree (`successful > total`, impossible), the split is
dropped and the cell reads `split unavailable`. An earlier version clamped with
`Math.max(0, …)`, which rendered an impossible ledger as fact.

## Rejected

**Two coloured numbers, `● 280  ● 85`** (the original implementation, replaced
after review). It dropped the total the report explicitly asked to keep, put
the only distinguishing signal in colour — a screen reader announced "280 85",
and green/red is the worst pair for deuteranopia — and overflowed the 110px
column on ~44% of ledgers, clipping without an ellipsis so a truncated count
would have rendered as fact. The one explorer cited as precedent (StellarChain)
does render it that way; that was a single data point, and stellar.expert, the
other reference, shows no count at all.

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
- [x] Aggregate dedups without `FINAL` — `LIMIT 1 BY` + `countIf`, the house
      idiom (an earlier revision used `uniqExact`, which risks a per-group hash
      set on any wider variant)
- [x] `read_rows` measured and recorded at BOTH ends of the supported range —
      16,384 / 160 KiB / 6 ms at 10 ledgers, 49,152 / 480 KiB / 19 ms at the
      101-ledger maximum. Headroom computed against the binding `read_rows`
      quota, not bytes
- [x] Missing aggregate rows render the total plus an explicit
      `split unavailable`, never a `0 successful` that reads as a total failure
      and never a bare total that reads as a successful count
- [x] Zero-transaction ledger renders without a divide-by-zero — the rate is
      computed only when a split exists; the 10 such ledgers in the whole table
      have no `transactions` rows and take the unavailable path
- [x] Home widget and `/ledgers` both covered by the shared table change
- [x] **Docs updated** per ADR 0032 — `04_get_ledgers_list.sql` (with the
      `-- @@ split @@` separator its runner arm requires),
      `05_get_ledgers_by_sequence.sql`, the two `frontend-overview.md` sections
      describing these surfaces, the endpoint-queries `README.md` statement
      count, and both `run_endpoint_ch.sh` arms so the Tier-1 gate actually
      parses the new statements
- [x] **API types regenerated** — `openapi.json` + `generated/` committed
      alongside the API change

## Implementation Notes

Three atomic commits on `feat/0445_per-ledger-success-failed-split` (PR 392):

| Commit     | Scope                                                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `b476e07e` | `crates/api/src/ledgers/{dto,queries,handlers}.rs` + regenerated `libs/api-types` (same commit — CI gate `API types freshness`) |
| `69286dde` | `docs/architecture/.../0{4,5}_get_ledgers_*.sql`                                                                                |
| `c77d9963` | `TransactionCounts.tsx` + test, wired into `LedgersTable` and `LedgerSummary`                                                   |

A fourth commit reworked the change after a five-agent review (correctness,
simplification, adversarial, requirements audit, UX). What it changed is in
Issues Encountered; the sections above describe the result, not the first
attempt.

Backend: `fetch_successful_counts` (aggregate over an explicit key list) plus
`attach_successful_counts` (fills the deduped page in place, degrading to `None`
on error). The detail path calls the same function with a one-element list.

Frontend: one new component, `TransactionCounts`, consumed by both the shared
ledgers table and the detail summary. No existing test was modified; 9 were
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

1. **Nullable field.** `null` (no `transactions` rows) is not `0` (everything
   failed). The counts come from two different tables, so the wire type has to
   be able to say "no split". Note the justification originally given — that the
   two diverge during a backfill window — is backwards: `persist::writer` writes
   `ledgers` last as a commit marker, so a visible ledger row implies its
   transactions are visible. The reachable null case is the 10 empty ledgers,
   plus a failed aggregate (below).
2. **Second query, not a JOIN.** The list read is tuned for
   `optimize_read_in_order`; attaching an aggregate risks that plan.
3. **Total plus failure rate, two lines.** The rate is the informative half — a
   measured 13.9% → 53% swing across p05–p95 — and the total stays the scan key.
   Chosen after review replaced the original two-coloured-numbers design; see
   `## Rejected`.

### Emerged

4. **One shared `TransactionCounts` component** instead of editing the two call
   sites separately, as the scope implied. The detail summary and the table
   need identical semantics, including the fallback — duplicating that logic
   would have let the two drift.
5. **Column header renamed `TX Count` → `Transactions`.** Not in scope, but the
   cell no longer holds a single count and the old header would misdescribe it.
6. **A disagreeing split is dropped, not clamped.** `successful > total` is
   impossible; rendering `Math.max(0, …)` would show an impossible ledger as
   fact. The cell says `split unavailable` instead. (The clamp was the original
   choice and a test pinned it — both replaced.)
7. **Plain `as i32` cast with a bound comment**, matching the six other CH `u64`
   aggregates in the crate. `application_order` is `Int16`, so a per-ledger
   count cannot exceed 65,536 — the bound is provable, and the original
   `unwrap_or(i32::MAX)` would have rendered 2,147,483,647 as a real count.
8. **Detail path takes a second round trip** rather than a scalar subquery on
   the header read. A subquery returns `0` for a ledger with no rows, which is
   indistinguishable from a total failure — the exact case decision 1 exists to
   preserve. Not parallelised with `try_join!`: the endpoint caches for 300s and
   splitting the `let-else` costs more readability than the latency is worth.
9. **Explicit key list + partition prune on the aggregate**, not a `BETWEEN`
   span. See "Two queries, not a JOIN — and the read guard".
10. **Aggregate failure degrades to `None`** instead of propagating. See
    "Failure degrades, it does not 500".

## Issues Encountered

- **The first implementation shipped four real defects, found by review, not by
  the tests.** Recorded because each was invisible to the checks that passed:

  1. The aggregate used `BETWEEN min AND max` while its own comment cited
     `ch::fetch_tx_list_aggregates` as the pattern — that function's key list
     and partition prune are labelled "the load-bearing read guard", and only
     the two-step half had been copied. On a page straddling orphan transaction
     rows this sweeps unboundedly; 0243/0386 were outages in that shape.
  2. Both call sites propagated the aggregate error with `?`, so a failed
     decorative query returned 500 for the whole ledgers list and the polled
     home widget — while the nullable field and a tested frontend fallback for
     exactly that state sat unused.
  3. The cost claim "granule-bound, so a wider page costs the same" was
     generalised from two samples that happened to sit in the same 2-granule
     bucket. Re-measured: 3x at the 101-ledger maximum. Headroom had also been
     computed against the bytes quota when `read_rows` binds ~5x tighter.
  4. The display carried its meaning in colour alone (a screen reader announced
     "280 85"), dropped the total the report asked to keep, and overflowed the
     110px column on ~44% of ledgers — clipped, not ellipsised, so a truncated
     count would have rendered as fact.

  Also fixed: `04_get_ledgers_list.sql` gained a second statement without the
  `-- @@ split @@` separator its runner arm requires, and neither runner arm was
  extended, so the Tier-1 syntax gate was silently not checking the new SQL.

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
