---
id: '0345'
title: 'PERF: entity-filtered endpoints read whole dimension tables — id-IN resolver + read-in-order fixes'
type: PERF
status: completed
related_adr: []
related_tasks: ['0338', '0344', '0353']
tags:
  [priority-high, effort-medium, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Spawned from 0338 load-test analysis — the ~25M-row full-scan cluster (tier 2 bottleneck).'
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: >
      Diagnosis corrected after mapping all 7 runtime queries (local CH, 25k-ledger
      sample). Original "skip-index / MV" framing is mostly wrong: the fact tables
      (soroban_events, soroban_invocations_appearances, transaction_participants)
      already LEAD their ORDER BY with the entity column, so entity seeks are
      PK-prefix, not scans. Real causes: 4× the SAME anti-pattern as 0344
      (`LEFT JOIN accounts x ON x.id = <surrogate>` reads the whole ~25M dimension),
      + 2 optimization-defeats (subquery-IN bypassing the bloom; inner `LIMIT 1 BY`
      defeating read-in-order), + 1 non-PK-sort-under-FINAL. So 0345 is mostly a
      query rewrite reusing 0344's id-IN resolver, plus one projection. No new
      skip indexes / MVs needed for 6 of 7.
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: >
      Steps 0-2 implemented + verified on the live local API (25k-ledger fresh DB):
      shared resolve_accounts/resolve_contracts in common/ch.rs; category-A id-IN
      rewrites for ctrdetail / accdetail(issuer) / acctxs(source) / ctrinvoc(caller);
      lplist iss-CTE → Rust-materialised literal IN. All 5 endpoints byte-identical
      before/after (jq -S diff empty) — the whole-`accounts` read is eliminated
      (ctrdetail 246k→1.5k local). Steps 3 (ctrevents) + 4 (acclist) DEFERRED — see
      Implementation. Files: common/ch.rs, {accounts,contracts,liquidity_pools}/queries_ch.rs
      (+ transactions/queries_ch.rs now imports the shared resolvers from 0344).
  - date: 2026-07-03
    status: completed
    who: fmazur
    note: >
      Completed at 5/7 endpoints (Steps 0-2). Steps 3 (ctrevents) + 4 (acclist)
      deferred to 0353 — ctrevents needs either a CH config change (settings blocked
      by prod readonly=1) or a Rust dedup+fallback rewrite; acclist needs an
      `accounts` projection (schema change on ~25M). The 5 shipped are query-only,
      byte-identical, no schema beyond 0344's `idx_sc_id`. PENDING: commit + deploy.
---

# PERF: entity-filtered endpoints read whole dimension tables

## Summary

Seven account/contract/pool-filtered endpoints read ~24–49M rows per request in
the 0338 load test (4–10 s each). Runtime-query mapping shows the dominant cost
is **not** a missing skip index — it is the same whole-dimension read as 0344
(`LEFT JOIN accounts x ON x.id = <surrogate>` builds its hash side from the
entire ~25M `accounts` table), plus a few query-shape issues that defeat an
optimization CH could otherwise use. Fix = reuse the 0344 id-IN resolver + a
handful of query rewrites + one projection.

## Context

Evidence: `crates/load-tests/out/2026-07-01T13-43-39Z/results.csv`. Root-cause
map (verified against runtime SQL in `*/queries_ch.rs`, not the idealized docs):

| endpoint    | query fn                                       | cause                                                                                                                                                        | category                     |
| ----------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------- |
| `accdetail` | `accounts::fetch_balances`                     | `LEFT JOIN accounts iss ON iss.id=a.issuer_id` (+contract) → whole `accounts`                                                                                | **A** dim hash-join          |
| `acctxs`    | `accounts::fetch_transactions` (step 2)        | `LEFT JOIN accounts src ON src.id=t.source_id` → whole `accounts`                                                                                            | **A**                        |
| `ctrdetail` | `contracts::fetch_contract`                    | `LEFT JOIN accounts deployer ON deployer.id=sc.deployer_id`                                                                                                  | **A**                        |
| `ctrinvoc`  | `contracts::fetch_invocation_appearances`      | `LEFT JOIN accounts caller ON caller.id=m.caller_id` (+ invocation PK seek)                                                                                  | **A**                        |
| `lplist`    | `liquidity_pools::fetch_pool_list` (`iss` CTE) | `WHERE id IN (SELECT … FROM page)` — subquery form does NOT trigger bloom `idx_acc_id` → scans `accounts.id`                                                 | **C** bloom bypassed         |
| `ctrevents` | `contracts::fetch_events`                      | contract seek is a genuine PK-prefix, but the inner `LIMIT 1 BY` defeats `optimize_read_in_order` early-termination → reads the contract's whole event slice | **C** read-in-order defeated |
| `acclist`   | `accounts::fetch_list`                         | `accounts FINAL ORDER BY last_seen_ledger` (non-PK sort) → full scan+sort+merge                                                                              | **D**                        |

Cross-cutting: category **A** is identical in effect to the 0344 `JOIN … FINAL`
trap (none of these use FINAL, but `x.id = <surrogate>` still full-reads the
dimension to build the hash side). The proven fix is 0344's `resolve_accounts` /
`resolve_contracts` (`WHERE id IN (…) LIMIT 1 BY id`, bloom-pruned) — already
used for the contract-list deployer, not yet for these. Latent secondary A:
`common/ch.rs:157` `JOIN soroban_contracts sc FINAL`.

## Implementation

- **Step 0 — refactor ✅ DONE:** promoted `resolve_accounts` / `resolve_contracts`
  to shared `common/ch.rs`; `transactions/queries_ch.rs` (0344) now imports them.
- **Step 1 — category A (id-IN resolver) ✅ DONE:** `accdetail` (issuer_id — the
  dominant `iss` join; `sc` contract join left as a smaller residual), `acctxs`
  (source_id), `ctrdetail` (deployer_id), `ctrinvoc` (caller_id) — dropped the
  `LEFT JOIN accounts …`, fetch the surrogate, resolve via id-IN. All 4
  byte-identical on the live local API. (Latent `common/ch.rs:157` contract join
  NOT yet touched — small table, secondary.)
- **Step 2 — `lplist` ✅ DONE:** `iss` CTE removed; page issuer surrogates returned
  and resolved in Rust via the shared resolver (literal IN, bloom-pruned).
  Byte-identical on the live local API; the `iss` accounts scan is gone.
- **Step 3 — `ctrevents` ⏸ DEFERRED.** Cause: the inner
  `LIMIT 1 BY (ledger, tx, event_index)` defeats `optimize_read_in_order` — measured
  892k read_rows to return 11 for a 12.3M-event contract (0 real duplicates in it).
  Two viable fixes, both with a catch:
  - **(1) `SETTINGS read_in_order_two_level_merge_threshold = 0`** (keep `LIMIT 1 BY`):
    output identical by definition, ~3× (892k→295k). BUT a per-query `SETTINGS`
    fails under the prod `api_reader` `readonly=1` (`Code: 164`, same class as the
    0344 `log_comment` block) — needs a `profiles.xml` change (add to
    `changeable_in_readonly`, or set in the `read_only` profile) + a prod CH
    container recreate. So NOT API-only.
  - **(2) drop inner `LIMIT 1 BY`, dedup in Rust with an exact fallback**
    (re-query the original `LIMIT 1 BY` form only when a duplicate is detected in
    the page window): API-only, ~8× (892k→106k), provably equivalent. More code.
  - A naive over-fetch buffer is NOT formally safe (rejected).
- **Step 4 — `acclist` ⏸ DEFERRED (schema).** Needs a projection on `accounts`
  `ORDER BY (last_seen_ledger, id)` (+ drop FINAL). Schema change on a ~25M table
  (`MATERIALIZE` on prod) — heaviest, lowest ROI (a list endpoint). Best split to a
  sub-task; the only one of the 7 that genuinely needs a schema change.

## Output-equivalence guarantee (per category)

- **A + lplist (Steps 1–2):** provably identical — resolves only the immutable
  StrKey (`account_id` / `contract_id`) via `LIMIT 1 BY id`, exactly the 0344
  substitution already proven byte-identical over 5.6M rows + 5 live-API tx.
- **ctrevents (Step 3):** dedup moves SQL→Rust — needs its own proof (identical
  set AND order). Verify before shipping.
- **acclist (Step 4):** projection is read-path only (same data); dropping FINAL
  for `LIMIT 1 BY id` must yield the same page — verify before shipping.
- **Method (all):** local API + `LOCAL_API` patch, per endpoint `curl` before/after
  → `jq -S` byte-diff must be empty, plus `read_rows` drop from `system.query_log`.

## Acceptance Criteria

- [x] Category-A endpoints (accdetail, acctxs, ctrdetail, ctrinvoc) resolve dimensions via id-IN; read_rows no longer scale with the accounts table size
- [x] `lplist` issuer resolution uses a literal `IN` list (bloom-pruned)
- [ ] `ctrevents` stops at LIMIT (read-in-order), dedup preserved — **deferred** (readonly/settings vs Rust-fallback trade-off, see Impl.)
- [ ] `acclist` no longer full-scans `accounts` under a non-PK sort — **deferred** (needs a projection = schema change)
- [x] Every changed endpoint (5/7): before/after JSON byte-identical on representative entities (local API)
- [x] Shared `resolve_accounts` / `resolve_contracts` in `common/ch.rs`, reused
- [ ] Docs updated (ADR 0032) for any schema change (acclist projection) — N/A for the 5 shipped (query-only, no schema); revisit if acclist projection lands

## Future Work

Deferred Steps 3 (`ctrevents`) + 4 (`acclist`) → **0353** (full detail + the
readonly/settings vs Rust-fallback trade-off for ctrevents, and the accounts
projection for acclist). Also: the latent `common/ch.rs:157`
`JOIN soroban_contracts sc FINAL` and the `accdetail` `sc`/assets residual joins
are smaller secondary offenders left untouched. Deploy: commit + prod deploy of
the 5 shipped (query-only, no CH recreate).
