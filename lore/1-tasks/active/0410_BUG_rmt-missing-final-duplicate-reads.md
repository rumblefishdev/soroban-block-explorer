---
id: '0410'
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

**Current state:** All **10 fixes (F0–F10) are implemented** on branch
`claude/ledgers-sorting-pagination-bug-6358ce` (7 files under `crates/api`,
~150 insertions). Each fix is verified against prod ClickHouse; `cargo check`
clean, **212 api unit tests pass**, and the `API types freshness` gate is green
(SQL-string-only changes ⇒ no `openapi.json` / generated diff). **Not committed
yet.**

Remaining open item: subtask **S1** (prod data dedup) — operator-gated, not
started.

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

- **F0 — Ledgers list** — `ledgers/queries.rs` `fetch_list` — `FROM ledgers l`
  → `FROM ledgers l FINAL`. Doubled rows + infinite-append. **Fixed.**

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

- [x] F0 ledgers-list `FROM ledgers l FINAL` (20 distinct vs 10-doubled without).
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
- [x] Build + tests: `cargo check -p api` clean, 212 api unit tests pass.
- [ ] Frontend: consider a defensive unique `rowKey` guard so future RMT dupes
      cannot re-trigger the React key-collision append (defense-in-depth).
- [ ] **Docs updated** — `N/A` — no change to system shape (schema/endpoints/
      pipeline unchanged; SQL-internal dedup only).
- [ ] **API types regenerated** — `N/A` expected — fixes are SQL-internal, no
      DTO/route/openapi change; confirm `check-generated` stays green before
      commit since paths under `crates/api/**` are touched.

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
