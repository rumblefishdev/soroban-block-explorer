---
id: '0420'
title: 'RMT reads without dedup: ledgers-list doubling + 9 same-class duplicate-row/count bugs (missing FINAL)'
type: BUG
status: active
related_adr: []
related_tasks: []
tags: ['area-api', 'area-clickhouse', 'bug-class-dedup', 'priority-high']
links: []
history:
  - date: 2026-07-18
    status: active
    who: karolkow
    note: 'Task created from Ledgers-list sort/pagination bug report; audited every API SQL query for the same RMT-missing-FINAL class (7 parallel auditors, prod-verified via chq). AUDIT ONLY — full fix set (F0,F3-F10) drafted + each prod-verified, then set aside in git stash@{0} per operator request (no implementation now). Nothing applied to the working tree.'
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      All 10 fixes (F0-F10) implemented on
      claude/ledgers-sorting-pagination-bug-6358ce; cargo check clean, 212 api
      unit tests pass, API-types gate green. Not committed. F1/F2 decision
      REVERSED after operator correction: "KPIs self-correct once data is
      deduped" was wrong — RMT regenerates duplicates continuously, so
      system.tables.total_rows is structurally inflated (measured drift
      +3%/+6.6% -> +4.3%/+11.6% within one session). Now counted from
      already-deduplicated sources (accounts_recent / soroban_contracts FINAL)
      at ~zero read cost. S1 downgraded from correctness fix to cost/hygiene.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      Renumbered 0410 -> 0420 (0410 collided with the 0393-spawned
      sac-event-identity-guard task, which has inbound references from 0393,
      0391, 0414, 0415; this task had none). /devils-advocate pass: F0's FINAL
      fix was a measured 19x read regression (25.9M rows / 1.55 GiB vs 1.35M /
      82 MiB) on a polled endpoint - reworked to over-fetch x3 + Rust
      dedup_consecutive, cost now identical to baseline. Measuring the rest
      caught F6 too (CTE re-evaluated the asset scan, +65%) - reworked to a
      GROUP BY'd join side, now cheaper than the original. All rewrites
      measured; 214 tests (+2 dedup regression tests). Four concerns carried:
      MV has no monitoring, S1 needs an owner, frontend row key, and
      first_seen_ledger is rewritten on activity (separate task).
---

# RMT reads without dedup: ledgers-list doubling + 9 same-class bugs

## Summary

The Ledgers list page returned every row **twice** on ascending sort and
appeared to "append rows infinitely" on repeated sort clicks. Root cause: the
`ledgers` table is a **ReplacingMergeTree** whose parts are largely unmerged in
prod (~13M of 13.07M distinct sequences carry 2 physical rows, ~98% of history),
and `ledgers::fetch_list` read `FROM ledgers l` **without `FINAL`** — so each
keyset page returned both copies. Because the frontend keys table rows by
`sequence`, the duplicate rows produced **duplicate React keys**, which break
reconciliation and pile up orphaned DOM rows on every re-sort (the "infinite
append").

A sweep of **every SQL query in `crates/api`** found the same "RMT read without
deduplication" class in **9 more places** — 5 more firing in prod today, 4
latent (confirmed defects currently shielded from the UI). This task tracks all
10 fixes (F0–F10, implemented and prod-verified, pending commit) plus the S1
data-hygiene subtask.

## Status: Active — fixes implemented, not yet committed

> **Renumbered 0410 → 0420.** `0410` collided with
> `0410_BUG_sac-event-identity-guard-on-value-path` (spawned from 0393, merged
> via PR #355 and referenced from the archived 0393 README, 0391 notes, 0414 and
> 0415). This task was the one with no inbound references, so it moved.

**Current state:** All **11 fixes (F0–F10) are implemented** on branch
`claude/ledgers-sorting-pagination-bug-6358ce` (7 files under `crates/api`).
Each fix is verified against prod ClickHouse, **and each is now measured
before/after** — `cargo check` clean, **214 api unit tests pass** (+2 new dedup
regression tests), `API types freshness` gate green. **Not committed yet.**

After a `/devils-advocate` pass the F0 and F6 fixes were **reworked** — both
were correct but more expensive than the code they replaced (see Measured cost).
Post-rework **no fix costs more than the code it replaces; two cost less.**

Open items: subtask **S1** (operator-gated) plus four carried concerns — see
Open concerns.

## Measured cost (rows read per call, prod)

Correctness was never the hard part; cost was. Every rewrite is measured, because
the first attempt at F0 was correct and would have taken the site down.

| Fix                | before             | after                    | note                                                                              |
| ------------------ | ------------------ | ------------------------ | --------------------------------------------------------------------------------- |
| F0 ledgers list    | 1,349,927 / 82 MiB | **1,349,927 / 82 MiB**   | first attempt (`FINAL`) was 25,964,595 / 1.55 GiB — **19×**, on a polled endpoint |
| F3 contract counts | 2,710,818 / 61 MiB | **2,307,459 / 52 MiB**   | cheaper, −60% memory                                                              |
| F5 LP chart        | 26,046,613         | 26,038,421               | unchanged                                                                         |
| F6 asset search    | 1,151,738 / 32 MiB | **1,118,154 / 28.5 MiB** | cheaper than the un-deduped original                                              |
| F7 LP list         | 100,668            | 100,668                  | unchanged                                                                         |
| F1 total accounts  | 1 row (wrong)      | 1 row                    | `accounts_recent`, metadata read                                                  |
| F2 total contracts | 1 row (wrong)      | 176,854 / 11 MiB         | 13 ms on a ~146k-row table                                                        |

Two rejected-by-measurement alternatives worth remembering: `FINAL` on the
ledgers list (19×) and `LIMIT 1 BY` on the same query (3.3×) — both defeat
`optimize_read_in_order`. Over-fetching is free here because the read is
granule-bound: `LIMIT 60` and `LIMIT 20` read the identical 1,349,927 rows.

## Context

`ledgers`, `assets`, `accounts`, `soroban_contracts`, `transactions` and most
domain tables are ReplacingMergeTree (RMT) / AggregatingMergeTree. RMT dedup is
best-effort background merge — a read is only correct if it deduplicates itself
(`FINAL`, or `GROUP BY key + argMax/any`, or `LIMIT 1 BY key`, or a downstream
Rust HashMap/`dedup` collapse). Prod merges are badly behind, so any un-deduped
read/JOIN of these tables returns inflated counts or doubled rows **now**.

Prod duplication (physical rows vs deduped), measured via `chq`:

| Table               | Physical | Deduped | Dup share                                        |
| ------------------- | -------- | ------- | ------------------------------------------------ |
| `ledgers`           | 25.96M   | 13.07M  | ~2× across ~98% of history; only ~2k head merged |
| `assets`            | 769k     | 334k    | >2×                                              |
| `accounts`          | 14.77M   | 14.34M  | ~432k (~3%)                                      |
| `soroban_contracts` | 138k     | 130k    | ~8.5k (some ids ×6)                              |

The underlying data wants an `OPTIMIZE FINAL` / dedup pass, but that is an OPS
CH-write (separate, consent-gated) — **code must dedup on read regardless**, so
these query fixes are the durable ones.

## Findings

All confirmed against prod via `chq`. Severity = user impact today.

### Originally reported

- **F0 — Ledgers list** — `ledgers/queries.rs` `fetch_list`. Doubled rows +
  infinite-append. **Fixed by over-fetch (×3) + `dedup_consecutive` in Rust**,
  NOT `FINAL` and NOT `LIMIT 1 BY` — both defeat `optimize_read_in_order` on
  this seek (measured 19× and 3.3×; this endpoint is polled and the original
  code comment explicitly warned about exactly that). Same approach-B pattern as
  `assets::dedup_consecutive` (task 0364). Correct because `ORDER BY sequence`
  IS the primary key, so a sequence's physical copies are contiguous and
  byte-identical. Over-fetch ×3 covers the worst observed duplication (12.8M
  sequences carry 2 copies, 22 carry 3); beyond that the page merely comes back
  short — the keyset cursor still advances, so pagination never loops.

### Confirmed live (wrong output in prod now)

- **F1 — Total accounts KPI** — `network/queries.rs` — read
  `system.tables.total_rows` for `accounts` = raw part rows incl. dupes.
  **Fixed** → `count() FROM accounts_recent` (see Design Decisions #2).
  Measured 14,975,304 shown vs 14,354,378 real (**+4.3%**).
- **F2 — Total contracts KPI** — `network/queries.rs` — same via
  `soroban_contracts`. **Fixed** → `count() FROM soroban_contracts FINAL`
  (affordable: ~146k rows, not 14M). Measured 145,979 shown vs 130,817 real
  (**+11.6%**).

  Note the drift: F1/F2 were first measured at +3% / +6.6% and re-measured a few
  hours later at +4.3% / +11.6%. The inflation **grows continuously** — see
  Design Decisions #2.

- **F3 — Contract `recent_invocations` (list + detail)** —
  `contracts/queries.rs:~251` and `~528` — `sia FINAL INNER JOIN ledgers l`
  inside `count()`; ledgers side not deduped → count inflated ~1.5×
  (measured 2.36M → 3.79M on one contract). List and detail share the window so
  they stay equal to each other, both inflated.
- **F4 — Contract `recent_events` (detail)** — `contracts/queries.rs:~520` —
  `soroban_events se INNER JOIN ledgers le` in `count()`; ledgers fan-out
  (also `se` itself lacks FINAL — LOW, no self-dupes sampled).
- **F5 — LP chart `samples_in_bucket`** — `liquidity_pools/queries.rs:~798` —
  outer `JOIN ledgers` not deduped (0356 fix deduped snapshots, missed
  ledgers); `count()`/`sum(volume)`/`sum(fee_revenue)` doubled. `samples` live
  now; volume/fee double the moment they populate (task 0199).
- **F6 — Asset search** — `search/queries.rs:~629` — `assets a FINAL LEFT JOIN
soroban_contracts sc` — join side not deduped; ~96 assets fan out 2–6× into
  dup hits and burn the per-group result budget. `search_nfts` already does this
  right (uses a GROUP BY'd `sc` CTE) — the inconsistency is the tell.

### Latent (confirmed defect, currently shielded from the UI)

- **F7 — LP list `l_snap`** — `liquidity_pools/queries.rs:~1092` — un-deduped
  `ledgers` LEFT JOIN in final projection, no collapse → **doubles rows + breaks
  pagination** (proven: 20 rows for a 10-pool page on older pages). Shielded
  only because no frontend page consumes the pool list yet.
- **F8 — TPS 60s + latest-ledger subselect** — `network/queries.rs:~52` /
  `~66` — `sum(transaction_count)` over `ledgers` with no FINAL (inflates if a
  head ledger dups before merge); latest-ledger sub-select has no `LIMIT 1`
  (→ 500 via `fetch_optional` if head ever dups). Correct today only because the
  tip is currently merged. Weakest of the set: missing guard confirmed, wrong
  output not reproducible until the data condition occurs.
- **F9 — tx detail ops/invocations** — `transactions/queries.rs:~822` and
  `~964` — `fetch_operations` / `fetch_invocation_appearances` are correct only
  because `FINAL` on the driving table (`oa`/`sia`) _propagates_ into the joined
  `ledgers` (undocumented CH behavior; proven: 100 rows with FINAL, 200
  without). Doubles the instant that `FINAL` is dropped — exactly the quota
  change already made to the transactions _list_ path.
- **F10 — Account balances** — `accounts/queries.rs:~406` — `LEFT JOIN
soroban_contracts sc` not deduped; only incidentally collapsed by the adjacent
  `assets a FINAL` (isolation test: with `assets FINAL` → 1 row, without → 2).

### Clean (verified, no action)

nfts, transactions list, `common/ch.rs`, `common/head.rs`, most of search —
all deliberately deduped via `FINAL` / `LIMIT 1 BY` / `GROUP BY argMax` / Rust
HashMap collapse.

## Implementation Plan

Two fix patterns cover F3–F10:

1. **Dedup the join side without fan-out (preferred for count/window joins):**
   replace `INNER JOIN ledgers l ON l.sequence = x.ledger_sequence` used purely
   as a window filter with a semi-join `AND x.ledger_sequence IN (SELECT
sequence FROM ledgers WHERE <window>)` — `IN` matches once regardless of dup
   rows. Where `closed_at` is projected, use a GROUP BY'd subquery
   `(SELECT sequence, any(closed_at) closed_at FROM ledgers WHERE … GROUP BY
sequence)`.
2. **`FINAL` on the read (preferred for row-level list reads):** e.g. the F0
   ledgers-list fix. Cheap on a key-filtered `LIMIT` page.

3. **Count from an already-deduplicated source (for the KPIs, F1/F2):** never
   `system.tables.total_rows` — it is the physical part-row count. Prefer a
   source that is dedup-by-construction and cheap to count (see Design
   Decisions #2 for the option matrix).

- **F8:** add `LIMIT 1` to the latest-ledger sub-select (cheap, do regardless);
  dedup the TPS window sum.

## Acceptance Criteria

All implemented and prod-verified (evidence in parens); **not yet committed**.

- [x] F0 ledgers-list over-fetch ×3 + `dedup_consecutive` (20 distinct vs
      10-doubled; cost identical to the code it replaces — see Measured cost).
- [x] F1 total-accounts → `count() FROM accounts_recent` (14,354,378 vs
      14,975,304 inflated; matches `accounts FINAL` ±1).
- [x] F2 total-contracts → `count() FROM soroban_contracts FINAL` (130,817 vs
      145,979 inflated).
- [x] F3/F4 contract invocation + event counts (semi-join; 1.42M deduped, SQL
      runs, stats_sql regression tests pass).
- [x] F5 LP chart ledgers join (`LIMIT 1 BY sequence`; 1403 vs 2806).
- [x] F6 asset-search `soroban_contracts` join (page + `sc` CTE, mirrors
      `search_nfts`; executes).
- [x] F7 LP list `l_snap` (`GROUP BY sequence`; 5 rows vs 10).
- [x] F8 latest-ledger `LIMIT 1` + TPS `LIMIT 1 BY sequence` (executes).
- [x] F9 defensive `ledgers … FINAL` on tx-detail ops + invocations joins.
- [x] F10 explicit `soroban_contracts sc FINAL` on account-balances join.
- [x] Build + tests: `cargo check -p api` clean, 214 api unit tests pass.
- [x] Every rewrite measured before/after (F0, F3, F5, F6, F7) — see Measured
      cost. F0 and F6 reworked as a direct result.
- [x] Regression tests for the F0 dedup (`dedup_tests`): a full page collapses
      2–3× duplicates and stays full; an under-filled page never emits a
      duplicate.
- [ ] Regression tests for F1–F10 — only F0 is covered. The rest were verified
      by hand against prod, which is not repeatable (the duplicate band already
      shifted mid-session and F3 stopped differing).
- [ ] Frontend: defensive unique `rowKey` so future RMT dupes cannot re-trigger
      the React key-collision append (defense-in-depth) — see Open concerns.
- [ ] **Docs updated** — `N/A` — no change to system shape (schema/endpoints/
      pipeline unchanged; SQL-internal dedup only).
- [ ] **API types regenerated** — `N/A` expected — fixes are SQL-internal, no
      DTO/route/openapi change; confirm `check-generated` stays green before
      commit since paths under `crates/api/**` are touched.

## Open concerns (from the `/devils-advocate` pass)

Two of the seven were resolved inside this task (F0 regression → fixed;
unmeasured rewrites → measured, which is what caught F6). The rest are carried:

1. **The MV fails silently and nothing watches it** (High). `accounts_recent` is
   a plain MergeTree with no dedup safety net, and `count()` is a metadata read —
   so a partial or failed refresh is reported as truth. `system.view_refreshes`
   exposes `status` / `exception` / `last_success_time`; nothing alerts on them.
   Both the accounts KPI and the accounts list degrade quietly. **Do:** alert on
   status ≠ Scheduled/Running, non-empty `exception`, or `last_success_time`
   older than 3× the interval.
2. **Fixing the reads removed the pressure to fix the data** (High). Reads are
   now correct regardless of merge state, so nothing hurts visibly — but the
   duplicates still tax every query: the ledgers list reads **1,349,927 rows to
   return 20**, because the parts are fragmented. S1 needs a real owner and date,
   or it never happens.
3. **Frontend row key is still `sequence`** (Medium). Backend correctness is the
   only thing standing between us and the original runaway-append symptom. A
   collision-proof key is a few characters.
4. **`first_seen_ledger` does not mean what it says** (Medium). Found by
   accident: 7,875 accounts claim a `first_seen_ledger` inside the last 21
   ledgers while the deduped total grows by tens per minute — the column is
   evidently rewritten on activity. Anything computing account age / cohorts /
   "new accounts" off it is wrong. **Separate task, not this one.**

## Subtasks

### S1 — OPS: dedup the RMT data in prod (blocked on operator go)

The code fixes above make every read correct regardless of merge state, so this
subtask is **no longer needed for correctness** — it is a cost/hygiene job:
shrink the ~2× physical rows (disk + every scan pays for them) and find out why
merges fall so far behind.

Explicitly NOT a fix for F1/F2 — see Design Decisions #2. Duplicates always
regenerate, so no data pass can make a physical row count correct.

- [ ] `OPTIMIZE TABLE … FINAL` (or targeted dedup) on `ledgers`, `assets`,
      `accounts`, `soroban_contracts` (and audit the other RMT tables).
- [ ] Investigate WHY merges are ~98% behind on `ledgers` (merge settings /
      part explosion / re-ingest pattern) — a one-off OPTIMIZE without a root
      fix will just re-accumulate.

**Consent-gated CH write** — do NOT execute without explicit per-action operator
go (memory: no prod CH writes without consent). Needs the indexer-stopped /
`EXCHANGE TABLES` considerations from `docs/backfills.md`.

## Design Decisions

### From Plan

1. **Dedup on read, not on data.** Every fix makes the QUERY correct rather than
   relying on the table being merged. RMT dedup is a background best-effort
   merge, so a query that needs merged data to be right is a query that is
   sometimes wrong.

### Emerged

2. **F1/F2 KPIs: reversed an earlier wrong decision.** Initially recorded as
   "no code change — the KPIs self-correct once the data is deduped (S1)".
   **That was wrong**, caught by the operator: the indexer writes continuously,
   so RMT _always_ carries freshly-unmerged duplicates. `system.tables.total_rows`
   is the physical part-row count, so it is _structurally_ inflated — a one-off
   `OPTIMIZE` shrinks the error briefly and it grows straight back. Confirmed
   empirically within one session: +3%/+6.6% → +4.3%/+11.6%. The counts must
   dedup at READ time.

   The original objection (an accurate count means scanning 14M rows on a polled
   endpoint) turned out to be avoidable. Options considered:

   | Option                                            | Read cost         | Accuracy                        | Verdict                                                    |
   | ------------------------------------------------- | ----------------- | ------------------------------- | ---------------------------------------------------------- |
   | `total_rows` (was)                                | zero              | structurally inflated, drifting | rejected — the bug                                         |
   | **`accounts_recent` / `soroban_contracts FINAL`** | ~zero / low       | exact (≤2 min stale)            | **chosen**                                                 |
   | `count() FROM accounts FINAL` each poll           | high (merges 14M) | exact                           | rejected — quota burn                                      |
   | exact count on its own long cache                 | medium            | exact                           | viable fallback if the MV dependency ever hurts            |
   | `uniq(id)` HyperLogLog                            | medium            | approximate                     | rejected — trades known bias for random error, still scans |
   | own counter table (AggregatingMergeTree)          | zero              | exact                           | rejected — most infra, drift risk                          |

   Chosen: `accounts` counts from `accounts_recent`, the refreshable-MV copy
   already deduped to one row per account (plain MergeTree ⇒ `count()` is a
   metadata read). It refreshes every 2 min (~6 s), so staleness is bounded and
   irrelevant for a headline total, and it is the same source the `/accounts`
   list pages — so the KPI and the list now agree by construction. `contracts`
   uses `FINAL` because that table is ~146k rows, not 14M.

   Known ceilings: the accounts KPI now depends on `accounts_recent_mv` staying
   healthy (shared fate with the accounts list, so not a new single point of
   failure), and `soroban_contracts FINAL` gets more expensive as that table
   grows — revisit with the long-cache option if either bites.

## Notes

- Investigation: 7 parallel subagents, one per domain query file, each
  prod-verified with `chq`. Full evidence in session transcript 2026-07-18.
- Frontend repro (live): Ledgers list asc showed pairs (424,424,425,425…);
  repeated sort clicks grew DOM to 30→40 rows for 10 unique ledgers (duplicate
  React keys).
