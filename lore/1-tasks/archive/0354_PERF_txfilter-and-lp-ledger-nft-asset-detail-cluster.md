---
id: '0354'
title: 'PERF: txfilter (137M) + LP/ledger/nft/asset detail cluster — next read_rows bottlenecks'
type: PERF
status: completed
related_adr: []
related_tasks: ['0338', '0345', '0355', '0356']
tags:
  [priority-high, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Surfaced by the post-0344/0345 prod load test (2026-07-03) — endpoints not exercised in the 07-01 run, now the top read_rows offenders.'
  - date: 2026-07-03
    status: completed
    who: fmazur
    note: >
      Shipped 5/8 endpoints — query-only, byte-identical, no schema change:
      ldgdetail / lpparts / lptxs / asttxs (id-IN resolver swap, reusing 0345's
      common/ch.rs) + txfilter (Statement B rewritten to the two-step key-seek,
      eliminating the whole-head-partition scan that WAS the 137M). All verified
      on the live local API; txfilter proven byte-identical across 8 scenarios
      (single-tx, truncation at limits, a 3.2M-tx contract, page-2 pagination).
      nftdetail → 0355 (no local NFT data). lpdetail/lpchart → 0356: cannot be
      done output-identically query-only — `liquidity_pool_snapshots` is a
      ReplacingMergeTree with NO version column AND has differently-valued
      re-ingest duplicates, so dropping FINAL is not reproducible (also spawned a
      data-quality bug for those duplicates). txfilter's driver read (ops-arm) is
      a separate residual, deferred.
---

# PERF: txfilter + LP/ledger/nft/asset detail cluster

## Summary

After 0344 + 0345 shipped (txdetail 228×, ctrdetail 95×, ctrinvoc 45×, 0 errors),
the 2026-07-03 prod load test exercised **more endpoints** than the 07-01 run and
revealed the next tier of heavy `read_rows` — led by **`txfilter` at 137M/request**
(bigger than txdetail's original 102M). These were `0` in the earlier run (not
harvested), so they are newly-measured, not regressions.

## Context

Evidence: `crates/load-tests/out/2026-07-03T14-25-44Z/results.csv` (10-VU smoke).
Max read_rows / p95 (ms), newly-visible endpoints:

| endpoint     |       read_rows |   p95 | module                       |
| ------------ | --------------: | ----: | ---------------------------- |
| **txfilter** | **137,218,757** |  3124 | transactions (filtered list) |
| lptxs        |      45,794,890 |  4916 | liquidity_pools              |
| asttxs       |      28,085,988 |  5535 | assets                       |
| ldgdetail    |      27,358,277 |  5437 | ledgers                      |
| nftdetail    |      27,195,936 | 10689 | nfts                         |
| lpparts      |      27,025,542 |  8043 | liquidity_pools              |
| lpdetail     |      15,959,797 |  4342 | liquidity_pools              |
| lpchart      |      14,060,619 |  1860 | liquidity_pools              |

Related residuals already tracked elsewhere (NOT this task): `lplist` snapshots/sac
residual (0345), `accdetail`/`acctxs` residual joins (0345), `acclist`/`ctrevents`
(0353).

## Diagnosis (all 8 mapped to runtime SQL)

Categories: **A** = whole-dimension `JOIN accounts/soroban_contracts` (reuse the shipped
0345 `common/ch.rs` resolvers); **B** = fact-table scan by a non-PK join key;
**D** = `liquidity_pool_snapshots FINAL` scanning a pool's whole snapshot history.

| endpoint          | fn (file)                                           | dominant read                                                                                                                                                                                                                                                                                                                                | cat     | fix                                                                                                                                                                                                                 | schema?              |
| ----------------- | --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| **txfilter** 137M | `transactions::fetch_list` (Stmt B)                 | `SELECT * FROM transactions WHERE <partition>` streams the WHOLE head partition (~1e8): the contract filter lives only in the ≤80-row driver `m`, and CH can't push it through `t.id = m.transaction_id` (`id` is not a PK prefix; `transactions` ORDER BY `(ledger_sequence, application_order)`). Plus a whole-`accounts` `LEFT JOIN src`. | **B+A** | two-step: take `m`'s ≤limit `(ledger_sequence, transaction_id)` keys, seek `transactions` by `(ledger_sequence, id) IN (…)` + partition prune (as assets/LP already do); resolve `source_id` via `resolve_accounts` | **no**               |
| **lptxs** 46M     | `liquidity_pools::fetch_pool_transactions` (step 2) | STEP-2 `INNER JOIN accounts src ON src.id=t.source_id` builds the whole ~23M accounts hash (STEP-1 driver already read-in-order)                                                                                                                                                                                                             | **A**   | resolve `source_id`                                                                                                                                                                                                 | **no**               |
| **asttxs** 28M    | `assets::fetch_transactions` (page_sql)             | `LEFT JOIN accounts src` whole ~23M (driver blooms prune)                                                                                                                                                                                                                                                                                    | **A**   | resolve `source_id`                                                                                                                                                                                                 | **no**               |
| **ldgdetail** 27M | `ledgers::fetch_transactions`                       | `transactions t FINAL` is a 1-ledger PK seek (tiny), but `LEFT JOIN accounts src FINAL` reads/merges whole ~23M accounts                                                                                                                                                                                                                     | **A**   | drop FINAL + resolve `source_id`                                                                                                                                                                                    | **no**               |
| **nftdetail** 27M | `nfts::fetch_by_composite`                          | `nfts n FINAL` tiny PK seek; the two `LEFT JOIN accounts/soroban_contracts ON …id=surrogate` build whole-dimension hashes (~23M+~25M). (Comment "cheap bloom probe" is WRONG — a JOIN can't use `idx_acc_id`, only `WHERE id IN (lit)` does.)                                                                                                | **A**   | resolve `current_owner_id` + `contract_id`                                                                                                                                                                          | **no**               |
| **lpparts** 27M   | `liquidity_pools::fetch_participants`               | `lp_positions FINAL` seeks by `pool_id` (small); `JOIN accounts acc FINAL ON acc.id=lpp.account_id` reads whole ~23M accounts                                                                                                                                                                                                                | **A**   | resolve the page's `account_id`s                                                                                                                                                                                    | **no**               |
| **lpdetail** 16M  | `liquidity_pools::fetch_pool_by_id`                 | `liquidity_pool_snapshots FINAL` ×2 (latest + min) + `lp_positions FINAL` — pool_id is leading PK so each seeks the pool's slice, but FINAL merges the whole slice, scanned twice                                                                                                                                                            | **D**   | drop FINAL; latest via `ORDER BY ledger_sequence DESC LIMIT 1` read-in-order; min-ledger from the same slice                                                                                                        | mostly **no**        |
| **lpchart** 14M   | `liquidity_pools::fetch_pool_chart`                 | `liquidity_pool_snapshots lps FINAL` GROUP BY — merges the hottest pool's full ~1.84M-snapshot history to bucket it                                                                                                                                                                                                                          | **D**   | drop FINAL, dedup via `argMax(_, version)` (snapshots unique per `(pool_id, ledger_sequence)`)                                                                                                                      | query-only **or MV** |

## Implementation

- **Step 1 — category A resolver swaps (query-only, reuse `common/ch.rs`):** `lptxs`,
  `asttxs`, `ldgdetail`, `nftdetail`, `lpparts`. Mechanical repeats of 0345 — swap the
  enrichment-side `JOIN accounts/soroban_contracts (FINAL)` for
  `resolve_accounts`/`resolve_contracts` on the page's ≤limit surrogate ids.
- **Step 2 — `txfilter` (priority, 137M):** restructure Statement B to the two-step
  shape assets/LP already use — collect `m`'s ≤limit `(ledger_sequence, transaction_id)`
  keys, then `SELECT … FROM transactions WHERE (ledger_sequence, id) IN (…) AND
intDiv(ledger_sequence,500000) IN (…)`, and resolve `source_id` via `resolve_accounts`.
  Removes BOTH the 137M partition scan AND the 23M accounts read. Fix the residual
  whole-`accounts` join in Statements B/C (SLIM_PROJECTION) too.
- **Step 3 — LP snapshots (category D):** `lpdetail` + `lpchart`. Drop `FINAL` on
  `liquidity_pool_snapshots`; take latest per pool via read-in-order `ORDER BY
ledger_sequence DESC LIMIT 1`; dedup aggregates via `argMax(_, version)`. Query-only
  first; ONLY if the hottest pool (~1.84M snapshots) still exceeds target → a schema
  object: a per-bucket snapshot MV (lpchart) / "latest snapshot per pool" projection
  (lpdetail).

## Output-equivalence guarantee

- **Steps 1-2 (category A + txfilter):** provably identical — resolves only the immutable
  StrKey via the already-proven 0344/0345 resolver; txfilter's two-step keys fetch the
  exact same `transactions` rows (verify the diff).
- **Step 3 (snapshots FINAL → argMax):** dedup moves SQL→`argMax(_, version)` — needs its
  own proof that `argMax(latest version)` == the row `FINAL` would pick. Verify before shipping.
- **Method (all):** local API + `LOCAL_API`, per endpoint `curl` before/after → `jq -S`
  byte-diff must be empty, plus `read_rows` drop from `system.query_log`.

## Acceptance Criteria

- [x] Each listed endpoint mapped to its query fn + dominant-read root cause (see Diagnosis)
- [x] `txfilter` — whole-head-partition scan (the 137M) eliminated via the two-step key-seek; residual driver ops-arm deferred
- [~] LP/ledger/asset cluster reduced — `lptxs`/`lpparts`/`asttxs`/`ldgdetail` done; `nftdetail`→0355; `lpdetail`/`lpchart`→0356 (blocked)
- [x] Every SHIPPED endpoint (5): before/after JSON byte-identical on the live local API (txfilter across 8 scenarios)
- [x] No new skip index / projection — all 5 are query-only (no `init.sql` change)
- [x] Docs (ADR 0032): N/A — query-only, no schema change

## Implementation Notes

- Shipped (query-only, reuse `common/ch.rs` resolvers): `ldgdetail`
  (`ledgers::fetch_transactions`), `lpparts` (`liquidity_pools::fetch_participants`),
  `lptxs` (`liquidity_pools::fetch_pool_transactions`), `asttxs`
  (`assets::fetch_transactions`) — dropped the whole-`accounts` `JOIN`, select the
  surrogate, resolve via `resolve_accounts`. `txfilter`
  (`transactions::fetch_list` Statement B) — rewritten to fetch the ≤lim_over
  driver keys then SEEK `transactions WHERE (ledger_sequence, id) IN (…)` +
  `resolve_source_and_closed_at` (the same shape Statement A/assets/LP use).
- Perf shape: the whole-`accounts` read (∝ table size = ~25M on prod) becomes a
  bounded id-IN granule seek (∝ #ids on the page); txfilter's ~1e8 partition scan
  becomes a key-seek. Local numbers understate it (local `accounts` is only 246k).

## Issues Encountered

- **lpdetail/lpchart NOT shippable query-only (the Step-3 blocker).**
  `liquidity_pool_snapshots` is `ReplacingMergeTree(<no version>)` ORDER BY
  `(pool_id, ledger_sequence)`, and it HAS re-ingest duplicates whose values
  DIFFER (6/558k locally; the differing-duplicate check returned 6/6). Without a
  version column, `FINAL`'s row choice among differing duplicates is not
  query-reproducible, and `lpchart`'s `sum()`/`count()` would double-count if
  FINAL were simply dropped → no query-only replacement is output-identical.
  Deferred to 0356 (+ a data-quality bug for the differing duplicates).

## Design Decisions

### Emerged

1. **txfilter fix is the two-step key-seek, not a resolver swap** — the 137M was
   the streamed whole head partition (`FROM (SELECT * FROM transactions WHERE
<partition>) t`), because CH can't push `t.id = m.transaction_id` into the
   scan. Verified byte-identical across 8 scenarios before shipping.
2. **Stopped at 5/8 rather than force lpdetail/lpchart** — dropping FINAL there
   is not output-identical (see Issues); honoured the "identical output + no
   schema beyond an index" constraint over completeness.

## Future Work

- `nftdetail` — resolver swap, needs NFT test data → **0355**.
- `lpdetail`/`lpchart` — needs a snapshot version column (schema) or a data fix,
  not query-only → **0356**; that task also carries the differing-duplicate
  data-quality bug.
- `txfilter` driver ops-arm residual read — a skip-index/seek follow-up (separate).
