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
latent (confirmed defects currently shielded from the UI). This task tracks the
already-applied ledgers fix plus the remaining fixes.

## Status: Active — AUDIT ONLY (no code applied)

**Current state:** This task is the **audit**. No implementation is in the
working tree. The full fix set (F0 ledgers + F3–F10) was drafted and each fix
**verified viable against prod ClickHouse**, then set aside per operator request
to keep this a task-spawn only — the diff lives in `git stash@{0}`
(message `lore-0410: RMT dedup fixes …`); `git stash show -p stash@{0}` to
review, `git stash pop` to re-apply. Findings + verified fix patterns are
recorded below so the work can be re-applied or re-derived later.

Decisions already taken: fix scope = all 10 (when implemented); F1/F2 KPIs = no
code change (resolve via the S1 data dedup, not a polled scan).

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
  → `FROM ledgers l FINAL`. Doubled rows + infinite-append. Fix drafted +
  verified, in `git stash@{0}`; **not applied.**

### Confirmed live (wrong output in prod now)

- **F1 — Total accounts KPI** — `network/queries.rs:~60` — reads
  `system.tables.total_rows` for `accounts` = raw part rows incl. dupes; shows
  ~14.77M vs real ~14.34M (~3% high). `total_rows` cannot take FINAL.
- **F2 — Total contracts KPI** — `network/queries.rs:~63` — same via
  `soroban_contracts`; ~138k vs ~130k (~6.6% high).
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

- **F1/F2 (KPIs):** decision needed — keep the cheap-but-inflated `total_rows`
  estimate, or switch to `count() … FINAL` / `countDistinct(key)` (accurate,
  more expensive on a polled endpoint). See Design Decisions.
- **F8:** add `LIMIT 1` to the latest-ledger sub-select (cheap, do regardless);
  dedup the TPS window sum.

## Acceptance Criteria

Fix approach for each is **drafted + prod-verified** (evidence in parens) but
**NOT applied** — the diff is in `git stash@{0}`. Checkboxes flip to `[x]` when
the fix actually lands in the tree.

- [ ] F0 ledgers-list `FROM ledgers l FINAL` (verified: 20 distinct vs
      10-doubled without). **The originally-reported bug — still unfixed in tree.**
- [x] F1/F2 KPI approach decided — **no code change.** `total_rows` estimate
      stays; a polled `count() FINAL` scan every ~5s is the wrong trade. KPIs
      self-correct once the RMT data is deduped → subtask S1.
- [ ] F3/F4 contract invocation + event counts (semi-join; verified 1.42M
      deduped, SQL runs, stats_sql regression tests pass).
- [ ] F5 LP chart ledgers join (`LIMIT 1 BY sequence`; verified 1403 vs 2806).
- [ ] F6 asset-search `soroban_contracts` join (page + `sc` CTE, mirrors
      `search_nfts`; verified executes).
- [ ] F7 LP list `l_snap` (`GROUP BY sequence`; verified 5 rows vs 10).
- [ ] F8 latest-ledger `LIMIT 1` + TPS `LIMIT 1 BY sequence` (verified, tps 62.4).
- [ ] F9 defensive `ledgers … FINAL` on tx-detail ops + invocations joins.
- [ ] F10 explicit `soroban_contracts sc FINAL` on account-balances join.
- [ ] Frontend: consider a defensive unique `rowKey` guard so future RMT dupes
      cannot re-trigger the React key-collision append (defense-in-depth).
- [ ] **Docs updated** — `N/A` — no change to system shape (schema/endpoints/
      pipeline unchanged; SQL-internal dedup only).
- [ ] **API types regenerated** — `N/A` expected — fixes are SQL-internal, no
      DTO/route/openapi change; confirm `check-generated` stays green before
      commit since paths under `crates/api/**` are touched.

## Subtasks

### S1 — OPS: dedup the RMT data in prod (blocked on operator go)

The code fixes above make every read correct regardless of merge state. This
subtask fixes the **underlying data** so the tables stop carrying ~2× physical
rows (storage + scan cost), and makes the F1/F2 KPI `total_rows` estimate
accurate again for free.

- [ ] `OPTIMIZE TABLE … FINAL` (or targeted dedup) on `ledgers`, `assets`,
      `accounts`, `soroban_contracts` (and audit the other RMT tables).
- [ ] Investigate WHY merges are ~98% behind on `ledgers` (merge settings /
      part explosion / re-ingest pattern) — a one-off OPTIMIZE without a root
      fix will just re-accumulate.
- [ ] Re-check F1/F2 KPIs read correct once deduped.

**Consent-gated CH write** — do NOT execute without explicit per-action operator
go (memory: no prod CH writes without consent). Needs the indexer-stopped /
`EXCHANGE TABLES` considerations from `docs/backfills.md`.

## Notes

- Investigation: 7 parallel subagents, one per domain query file, each
  prod-verified with `chq`. Full evidence in session transcript 2026-07-18.
- Frontend repro (live): Ledgers list asc showed pairs (424,424,425,425…);
  repeated sort clicks grew DOM to 30→40 rows for 10 unique ledgers (duplicate
  React keys).
