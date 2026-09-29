---
id: '0588'
title: 'REFACTOR: retire the endpoint-query SQL reference set — the Rust queries and their ClickHouse tests are the reference'
type: REFACTOR
status: completed
related_adr: ['0060']
related_tasks: ['0207', '0478', '0587']
tags: [docs, clickhouse, api, cleanup, effort-small, priority-medium]
links: []
history:
  - date: '2026-09-28'
    status: active
    who: karolkow
    note: >
      Decided on review of 0478's PR, which built a CI gate to keep the set
      parseable. The set is a hand-kept copy of the Rust queries; the gate
      could only prove the copies plan, never that they match the Rust. 0478's
      PR closed unmerged; 0478 and 0587 superseded by this task. (Corrected
      2026-09-28: this entry first said the Rust queries already run against
      the schema in CI — they do not; see Context and task 0480.)
  - date: '2026-09-28'
    status: completed
    who: karolkow
    note: >
      Closed in the release sweep of production-2026.09.28-1: every lore-0588
      commit is an ancestor of the tag, and the tag holds no endpoint-queries-
      clickhouse path. Docs and refactor only — nothing on production changes.
---

# REFACTOR: retire the endpoint-query SQL reference set

## Summary

`docs/architecture/database-schema/endpoint-queries-clickhouse/` holds one
hand-written `.sql` file per public endpoint plus a runner script. Task 0207
wrote it in May as the ClickHouse pilot's reference set, before the Rust API
existed on ClickHouse. The Rust queries have long been the source of truth,
and the files now trail them. This task deletes the set and moves the few
notes worth keeping next to the Rust queries they explain.

## Context — measured 2026-09-28

- **Maintenance cost.** 92 of the 317 commits that touched `crates/api` in
  the last 90 days also had to edit these copies (0374, 0491, 0199, 0580 …).
- **Drift anyway.** Before 0478, seven files did not even parse. After it, `06`
  and `23` were still stale: `23` documents the `JOIN accounts FINAL` that
  0354 removed from the Rust.
- **The real check exists, but not in CI.** Every API module (accounts,
  assets, contracts, ledgers, liquidity_pools, network, nfts, search,
  transactions) has ClickHouse-gated tests (`decode_smoke` / `ch_tests`) that
  check the queries the API actually runs. They need `CH_URL`, which no
  workflow sets, so CI skips them: the "ClickHouse e2e" step tests only
  `db-clickhouse`, `backfill-runner` and the enrichment crates (task 0480
  tracks the gap). Retiring the set removes no CI check either way — the
  copies never checked the Rust, and the gate on them was not merged.
  (Corrected 2026-09-28; the first version of this bullet said CI runs them.)
- **Exact executed SQL** is recorded in production `system.query_log`
  (`chq`), with no maintenance.
- **Readers.** Code comments of the form "wire shapes mirror canonical SQL"
  (~20 files in `crates/api`), the `compare-with-stellar-api` skill, one line
  in `backend-overview.md`, and an allow rule in `.claude/settings.json`.

## Implementation Plan

1. Record the decision as ADR 0060.
2. Move worthwhile header knowledge into comments beside the Rust query
   functions: index and `FINAL` rationale, and traps such as the Decimal tuple
   comparison. Only where it says something the code does not; drop anything
   the Rust already states or that is stale.
3. `git rm` the directory.
4. Update every reader:
   - the Rust comments pointing at the files;
   - the `compare-with-stellar-api` skill: the Rust query plus
     `system.query_log` for the executed SQL;
   - `backend-overview.md`;
   - the `.claude/settings.json` runner rule;
   - any other live doc that links the directory.
5. Leave history alone: archived tasks and ADRs keep their references.

## Acceptance Criteria

- [x] ADR 0060 accepted, with the evidence above (one correction: the api
      crate's ClickHouse tests do not run in CI — see Issues)
- [x] The directory is gone and no live (non-archive) file references it —
      shown by a `git grep` (remaining hits: links to the archived 0207 task
      file, a dated 2026-05 audit, three lore tasks left as history, and the
      `.claude/settings.json` rule — Emerged 7)
- [x] Every note kept from the headers sits beside its Rust query; the PR
      lists what was kept and what was dropped
- [x] `compare-with-stellar-api` works without the set
- [x] **Docs updated** — `docs/architecture/**` no longer links the set
      (backend-overview, clickhouse-pilot, database-schema-overview); ADR 0060
      records why
- [x] **API types regenerated** — three doc comments reach the OpenAPI spec
      (network stats handler, search handler, `ParticipantItem`); regenerated,
      `check-generated` green

## Implementation Notes

Commits on `refactor/0588_retire-endpoint-sql-set`: ADR 0060; notes moved into
Rust; set deleted + readers updated + API types regenerated; this record.

**Deleted:** 25 files (23 `NN_*.sql`, `README.md`, `run_endpoint_ch.sh`),
3,719 lines.

**Kept / dropped, per file** (each checked against the current Rust before
moving):

| File                    | Outcome                                                                                                                                                                                                                                                                                                                                                     |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| README                  | dropped — its FINAL table was stale (`asset_aggregates`, `account_balances_current`, "FINAL on every RMT read"); the live rules sit beside each query and in `init.sql`; tiers and reviewer guide describe the set itself                                                                                                                                   |
| 01 network stats        | dropped — SQL stale (`system.tables.total_rows`, replaced in 0420); the Rust comment and `NetworkStats` docs carry TPS, counts, `generated_at`                                                                                                                                                                                                              |
| 02 transactions list    | **kept** one fact: the measured cost of statement C's `transaction_operations` scan (2026-09-25, partition 115, LIMIT 80: 63–147M rows for types 1/19/24) replaces the older "~8e7 at worst" in `transactions/queries/list_transactions.rs`. The rest is in the module doc or history (`contract_ids` removed in 0386)                                      |
| 03 transaction by hash  | dropped — hash-prefix seek, position filter, fee-refund sentinel and the `contract_activity` measurement are in `transactions/queries.rs` / `hash_lookup.rs`                                                                                                                                                                                                |
| 04 ledgers list         | dropped — the Rust has the stronger measured reasoning (over-fetch vs FINAL vs `LIMIT 1 BY`)                                                                                                                                                                                                                                                                |
| 05 ledger detail        | **kept** one trap: ClickHouse rejects a correlated subquery with ORDER BY / LIMIT that reads the outer row, which is why prev/next bind the sequence — beside the header query in `ledgers/queries.rs`. Its `(ledger_sequence, id)` cursor text was stale                                                                                                   |
| 06 account detail       | dropped — the `deleted` rationale and chain verification are on `fetch_deleted_status`; the balances part was pre-0331                                                                                                                                                                                                                                      |
| 07 account transactions | dropped — driver seek, FINAL cost and inlined bounds are in `accounts/queries.rs`; balance-change rules in `balance_changes.rs`                                                                                                                                                                                                                             |
| 08 assets list          | dropped — ordering, cost (61 ms / 1.02M rows), XLM display match and the no-ranking decision are in `assets/queries.rs` + tests. Not moved: "if the read quota ever binds, the cheaper shape is a driver over `balance_aggregates` itself (measured 11 ms), not a return to the alphabet" — advice for a future change; kept here instead (Emerged 5)       |
| 09 asset detail         | dropped — the three resolution forms and the issuer seek are in `assets/queries.rs`                                                                                                                                                                                                                                                                         |
| 10 asset transactions   | dropped — driver and the 0575 second-arm removal (46% / 1.8%) are on `fetch_transactions`                                                                                                                                                                                                                                                                   |
| 11 contract detail      | dropped — tri-state `upgradeable`, the caller-pair `uniqExact` (tests) and the `asset_sac` cost are in `contracts/queries.rs`. Its `wim FINAL` + rationale is not in the Rust (see Issues)                                                                                                                                                                  |
| 12 contract interface   | dropped — nothing beyond the Rust                                                                                                                                                                                                                                                                                                                           |
| 13 invocations          | dropped — `LIMIT` inside the subquery (22.3M vs 4.4M rows) and the ~38× FINAL cost are in `list_invocations.rs`                                                                                                                                                                                                                                             |
| 14 contract events      | dropped — payload misnomer, 1.2 GiB vs 0.6 GiB, position join are in `contracts/queries.rs`                                                                                                                                                                                                                                                                 |
| 15–17 NFTs              | dropped — derived `minted_at_ledger`, `event_type = 0`, `nullIf` / `join_use_nulls`, `leadInFrame` are in `nfts/queries.rs`; the files carried drift notices                                                                                                                                                                                                |
| 18 pools list           | **kept** one trap: ClickHouse re-evaluates a `WITH` subquery at every reference (the list's `page` CTE has six), so a per-request max over `pool_state_changes` cost 37–45M rows/page vs 8–10M with the `pool_activity` MV (2026-09-24) — on `ACTIVITY_LEDGER` in `list_pools.rs`. Activity ordering (699/770), pair matching, min_tvl rejection were there |
| 19 pool detail          | dropped — USD analytics and the `current_price_usd` rejection are in `usd_analytics.rs`                                                                                                                                                                                                                                                                     |
| 21 pool chart           | dropped — ASOF join, `1 AS k`, 48 h cap, partial-enrichment and negative-close caveats are in `get_pool_chart.rs` / `usd_analytics.rs`                                                                                                                                                                                                                      |
| 22 search               | dropped — bucket dispatch, account-prefix no-FINAL cost, ranking tiers, Code 241 trap are in `search/queries.rs`                                                                                                                                                                                                                                            |
| 23 pool participants    | dropped — stale (the 7-day window and `JOIN accounts FINAL` are gone); the Decimal-tuple trap is in `list_participants.rs`                                                                                                                                                                                                                                  |
| 24 pool activity        | dropped — the design note (no GROUP BY, measurements, op source 41%) is in `list_pool_activity.rs`                                                                                                                                                                                                                                                          |
| `run_endpoint_ch.sh`    | dropped — the set's runner                                                                                                                                                                                                                                                                                                                                  |

Counts: 3 notes moved into 3 Rust files (`ledgers/queries.rs`,
`transactions/queries/list_transactions.rs`, `liquidity_pools/queries/list_pools.rs`);
everything else dropped.

**Readers updated:** 31 files in `crates/api` and one line in
`crates/xdr-parser/src/state.rs` (every "canonical SQL NN", "canonical NN",
`NN_get_*.sql` and `endpoint-queries-clickhouse` pointer, rewritten to name the
Rust query); `backend-overview.md` (seven places), `clickhouse-pilot.md`
(reference-set section + references), `database-schema-overview.md`
("Canonical query references"); the `compare-with-stellar-api` skill (starts
from an endpoint or Rust query; optional `chq` read of `system.query_log` for
the executed text); `lore/3-wiki/manual-endpoint-audit.md` (pre-reading +
step 1). OpenAPI + generated types regenerated (`check-generated` green).

**Left on purpose** (history, not live readers): links to the archived task
file `0207_FEATURE_clickhouse-endpoint-queries-reference-set.md` (its name
contains the string), `docs/audits/2026-05-13-0197-step0/…` (a dated audit of
the older Postgres set), and the lore tasks below.

**Lore tasks that still tell a reader to update the set** (not rewritten):
0579 (README line 67), 0298 (backlog, line 50), 0539 (backlog, docs checklist
line 104). Others mention it as history only: 0381, 0396, 0487, 0586, 0349, 0581.

## Issues Encountered

- **The api crate's ClickHouse tests do not run in CI.** The Context above
  says CI runs them in the "ClickHouse e2e" step; it does not. They read
  `CH_URL`, which no workflow sets, and that step tests `db-clickhouse`,
  `backfill-runner` and the enrichment crates only. Task 0480 (backlog)
  already records it. ADR 0060 says so under Consequences.
- **Contract detail and `wasm_interface_metadata` FINAL.** File 11 joined
  `wim FINAL` so a re-insert could not transiently read as Unknown. The Rust
  does not, and its comment says the table is a plain `MergeTree` that rejects
  FINAL — but `init.sql` defines it `ReplacingMergeTree` since lore-0293. Not
  moved (not true of the current Rust); a finding for whoever next touches
  `fetch_contract`.
- **Comments already false, fixed in passing** (on lines being rewritten
  anyway): the network handler's "planner row-count estimates" (deduped counts
  since 0420); `transactions.id` as the list keyset tie-break (every list keys
  on the position); account detail balances "from `account_balances_current`"
  (from `balances` since 0331); `TxListCursor` "inside one partition" (the
  contract and account lists are not partition-bound); backend-overview's pool
  sentinel paragraph citing per-query sentinel predicates the ClickHouse
  queries do not have.

## Design Decisions

### From Plan

1. **Delete, don't gate** — ADR 0060.
2. **Keep only facts the Rust does not state and that are still true.** Every
   file was read against its Rust query; 3 notes moved.
3. **History untouched.** Archived tasks, ADRs (0047 included) and dated audits
   keep their references.

### Emerged

4. **Pointers rewritten to name the Rust function, not deleted.** "Shape pinned
   to `NN_*.sql`" became the query function (`queries::list_pools`,
   `queries::fetch_transfers`, …) so the context survives.
5. **The `balance_aggregates` driver note stays in this task, not in Rust.**
   `assets/queries.rs` (1,189 lines) is over the size limit and must not grow.
6. **ADR 0060 records the CI gap instead of claiming CI coverage.** The
   retirement stands: the copies never checked the Rust either.
7. **`.claude/settings.json` not edited.** The `run_endpoint_ch.sh` allow rule
   is dead but harmless; settings changes are Karol's.
8. **Manual endpoint-audit wiki pointers updated.** It linked the older
   Postgres set (deleted in 0244); pre-reading and step 1 now point at the Rust
   queries and the skill. The rest of that page (psql, PG-era) is left.
