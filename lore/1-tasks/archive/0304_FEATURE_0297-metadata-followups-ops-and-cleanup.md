---
id: '0304'
title: 'FEATURE: 0297 metadata follow-ups — backfill, deploy/flip, validation, frontend amounts, cleanup'
type: FEATURE
status: completed
related_adr: ['0050']
related_tasks: ['0297', '0231', '0243']
tags:
  [
    clickhouse,
    soroban,
    ops,
    frontend,
    cleanup,
    validation,
    priority-medium,
    effort-large,
  ]
links: []
history:
  - date: 2026-06-18
    status: backlog
    who: karolkow
    note: >
      Spawned from 0297. 0297 ships the CODE (parser → soroban_contract_metadata
      side table → API read-compose; task 0297). This task carries everything
      that is NOT that core code implementation: backfill, deploy/flag-flip,
      perf validation, live tests, frontend amount rendering, and the legacy
      name-path cleanup that was too entangled to land safely inside 0297.
  - date: 2026-06-23
    status: active
    who: stkrolikiewicz
    note: Promoted from backlog to active.
  - date: 2026-06-24
    status: active
    who: stkrolikiewicz
    note: >
      Backfill bin slice: added
      crates/backfill-runner/src/bin/metadata-backfill.rs (decision: re-parse the
      archive, not an RPC dump). Code + xhigh review done; prod run pending the
      0281 window. See Implementation Notes.
  - date: 2026-07-02
    status: active
    who: karolkow
    note: >
      Reality audit + spec sync. Confirmed against prod CH (chq): backfill DONE
      (soroban_contract_metadata 3728 rows / 3724 contracts), read-flip DONE
      (prod all-CH, unflagged compose in queries_ch), live tests present
      (metadata_e2e), frontend amounts DONE (via 0331; single `scaleByDecimals`
      scaler, no redundancy). The whole "BLOCKED on 0243" section is DEAD — 0243
      archived, PG retired (prod datasource all `ch`). This session: dropped the
      dead `sc.name` contracts-LIST name-search clause (contracts/queries_ch),
      banner-marked the superseded PG endpoint-queries doc set, synced ADR-0032
      docs (backend-overview + CH 08/11 SQL). Column DROP delegated to task 0310
      (owns the assets dead-column prod ALTER). See Design Decisions → Emerged.
  - date: 2026-07-07
    status: completed
    who: karolkow
    note: >
      Closed. All 4 PRs merged (#278 backfill worker, #282 FE symbol, #284
      ADR-0032 sync, #306 drop dead name columns + cleanup). Remaining open
      boxes reconciled against prod reality: prod ALTER DROP COLUMN name is
      DONE (chq 2026-07-07 — name absent from soroban_contracts + assets); the
      PG-side name plumbing / contract_name_writes chain was removed by the PG
      retirement task 0244 (commit d72eeca2, PR #319). Only the galexie
      created-vs-updated cross-check remains — nice-to-have, non-blocking,
      carried in Future Work.
---

# 0297 metadata follow-ups (ops / validation / frontend / cleanup)

## Summary

[Task 0297](0297_FEATURE_contract-name-enrichment-and-bytes-decode/README.md)
implemented the on-chain Soroban token metadata pipeline in **code**
(`soroban_contract_metadata` side table, parser extract → indexer write → API
read-compose; task 0297).
This task bundles the remaining non-implementation work.

## Scope

### Backfill & data

- [x] Backfill `soroban_contract_metadata` — DONE in prod (re-parse the archive,
      not an RPC dump; RPC ~7-day retention can't reach archived instances). Prod
      holds 3728 rows / 3724 contracts (chq, 2026-07-02). Bin:
      `crates/backfill-runner/src/bin/metadata-backfill.rs`.
- [ ] Direct created-vs-updated confirmation on a representative deploy via the
      galexie archive (RPC retention can't reach old deploys; see 0297 Option B).
      Nice-to-have validation, non-blocking (see Future Work).

### Validation & perf

- [x] Read JOIN / `FINAL` / `argMax` cost — validated by the live prod read path
      (all modules on CH, compose unflagged and serving; no read-rows regression
      reported). Effectively confirmed by the prod flip below.
- [x] Live integration tests (real CH): `crates/db-clickhouse/tests/metadata_e2e.rs`
      exercises metadata write + read-compose.

### Deploy

- [x] Read path flipped in prod — prod datasource is all-CH; the CH compose
      (`queries_ch` COALESCE over `soroban_contract_metadata`) is unflagged and
      live for assets/accounts. Backfill (3728 rows) landed first.

### Frontend

- [x] Amounts render via `decimals` — `scaleByDecimals(raw, decimals)` across
      asset/account/supply views (landed under task 0331; single BigInt scaler,
      no redundant scaling anywhere in the FE).
- [x] `symbol` surfaced in asset views (code-or-`symbol` fallback).

### List endpoints

- [x] Contract LIST name-search: dropped the dead `sc.name` clause →
      `contract_id`-only substring (2026-07-02, this task). `sc.name` was empty
      in prod and the contract API surfaces no name (0297 #3); not repointed.

### Name-search / column-drop — UNBLOCKED (0243 done, PG retired)

**The "BLOCKED on 0243" premise is dead.** 0243 is archived: all 9 API modules
serve from ClickHouse and the prod datasource flags are all `ch` — PG is retired
(compiled only as a rollback fallback). So the PG-repoint items below are moot:

- [x] ~~Repoint PG global-search / assets-list / `search_vector`~~ — N/A. PG is
      no longer the prod path; the CH global search already resolves contract
      names from `soroban_contract_metadata` (`22_get_search.sql`) and the CH
      assets COALESCE reads `m.name`. Nothing prod-facing reads the dead columns.

**Column DROP — owned by 0304.** Both `soroban_contracts.name` and `assets.name`
are confirmed 100% NULL in prod (148,663 and 336,053 rows, 0 non-null; verified
2026-07-02) and have **no reader** left (the last one, the contracts-LIST
name-search, was dropped this task). These are the 0297/0304 dead columns, so the
drop lives here — NOT in 0310 (that task's dead-column story is 0293's
`assets.{total_supply,holder_count}` + engine swaps, a different lineage).

- [x] Code removal (2026-07-02, this task): `name` gone from `AssetRow` +
      `SorobanContractRow` (`rows.rs`) and the 7 `stage.rs` build sites
      (`678,1592` contract rows; `1134,1182,1219,1233,1264` `AssetRow::staged`);
      dropped from `init.sql` (both tables) + `lib.rs` note. Column-order pinning
      tests updated. Indexer now writes the column as DEFAULT NULL on the existing
      prod table — safe until the ALTER.
- [x] Prod `ALTER TABLE {soroban_contracts,assets} DROP COLUMN name` — **DONE**
      (chq 2026-07-07: `name` absent from `soroban_contracts` + `assets`). Ran in
      0310's assets deploy-drain window as planned; `assets.name` batched with
      0310's `total_supply`/`holder_count` ALTERs. Ownership stayed 0304.
- [x] Upstream parser/PG-staging `name` plumbing — **removed by PG retirement
      task 0244** (commit d72eeca2, PR #319: "remove dead PG contract-name-write
      chain"). Correction (2026-07-02): the earlier
      "always `None` / fully un-threaded" claim was wrong. `ExtractedContractDeployment.name`
      is still populated (`state.rs:125-141` second pass + `extract_contract_data_name_writes`)
      and consumed by the **PG** staging path (`indexer/process.rs:421,457`
      `contract_name_writes` → `indexer/.../staging.rs:607,1024` `ContractRow`/`AssetRow`
      with str-keys). It compiles fine after the CH-side drop because PG staging
      uses different structs. Ripping it out = gutting the compiled PG write path,
      so it belongs in the PG-retirement task (see Future Work), not here.

### Cleanup (code)

- [x] Legacy `contract_name_writes` / `Symbol("name")` path — **fully removed by
      PG retirement task 0244** (commit d72eeca2). 0297 removed the CH-side name
      consumption; 0244 then ripped out the extraction (`extract_contract_data_name_writes`,
      the deploy second pass), `ParseOutput.contract_name_writes`,
      `ExtractedContractDeployment.name`, and the PG staging path along with the
      whole compiled PG write path. No `name` extraction chain remains.

### Docs (ADR 0032)

- [x] Sync done (2026-07-02): `backend/backend-overview.md` (metadata read-compose + `name` column now reader-less, drop pending 0310), CH
      `08_get_assets_list.sql` + `11_get_contracts_by_id.sql` (stale "pending 0243
      cutover" / "sc.name read by name-search" notes corrected). The **PG**
      `endpoint-queries/` set was banner-marked SUPERSEDED (PG retired) rather
      than edited per-file — it backs the still-compiled PG fallback.

## Implementation Notes

### Backfill bin (2026-06-24) — code done, prod run pending

Decision for the "re-parse vs RPC dump" choice above: **re-parse the archive**.
RPC `getLedgerEntries` can't reach archived/evicted instances (~7-day retention);
re-parse is the established codebase pattern.

Added `crates/backfill-runner/src/bin/metadata-backfill.rs` — a targeted-write
worker modeled on `pool-ids-backfill` (task 0266):

- Per ledger: `parse_ledger` →
  `stage::build_metadata_rows(&parsed.contract_metadata_writes)` (skips the full
  staging fold) → `StagedLedger { metadata_rows, ..Default::default() }` → the
  existing `PartitionWriter`, so **only** `soroban_contract_metadata` is written
  (other tables' inserts never open — `writer.rs` early-returns on empty vecs).
- Safe vs current data: table is `ReplacingMergeTree(version)`, `version =
observed ledger`; a re-parsed observation can only lose to a newer live row or
  fill a gap → idempotent, re-runnable. Runs in the **0281 window with live
  ingest stopped**, so there are no concurrent writes at all.
- Resume via a `--watermark` file (own marker, not the `ledgers` table). 8-way
  parallel on disjoint `--start/--end` ranges (1 insert/worker → trivial CH load).

Verified: `cargo check` / `clippy -D warnings` / unit test green. Reviewed at
`/code-review xhigh`; fixes applied (Emerged below).

### Design Decisions — Emerged (review-driven)

1. `--dry-run` no longer advances the `--watermark` file — the documented
   "dry-run then real run on the same watermark" flow would otherwise mark the
   range done and make the real run a silent no-op.
2. `--start` is required (no genesis default) so 8-way parallel can't silently
   overlap ranges on a forgotten flag.
3. Missing-file path: warn-skip + freeze the watermark; a run with any skip now
   exits non-zero so the 0281 runbook can't read a partial run as success.
4. Malformed-file check returns `Err` (graceful partition abort) instead of
   `assert!`-panicking past the writer cleanup mid-run.
5. `--end` upper-bound assert keeps the partition-loop u32 arithmetic in range.

### Still pending (this slice)

- Prod run in the 0281 window: `--dry-run` one partition for a measured
  per-partition time → extrapolate → 8 workers over 0266's synced `--local-dir`,
  `--start 50457424 --end <L_stop>`.
- Pre-run: confirm prod CH `users.d/timeouts.xml` raises `http_receive_timeout`
  (sparse-table insert held open across a partition).
- Validation: created-vs-updated on a galexie deploy; cross-check
  decimals/symbol/name against an independent source.

**Update 2026-07-02:** backfill has since run in prod — `soroban_contract_metadata`
holds 3728 rows / 3724 contracts (chq). Read-flip is live (prod all-CH). So the
"pending prod run" above is done; only the created-vs-updated galexie
cross-check remains as nice-to-have validation.

### Design Decisions — Emerged (2026-07-02 audit/sync session)

6. **Contract-LIST name-search: dropped the clause, did not repoint.** `sc.name`
   is empty in prod and the contract API surfaces no name (0297 #3), so an
   `contract_id`-only substring is the honest behavior. (`contracts/queries_ch.rs`.)
7. **Column DROP owned by 0304, code done here; prod ALTER coordinates with 0310.** Initially delegated to 0310, corrected: the `name` columns are the
   0297/0304 lineage and are 0304's acceptance criterion. Ownership ≠ deploy
   batching — the code removal (structs + 7 `stage.rs` sites + `init.sql` +
   `lib.rs`) landed on this branch; only the destructive prod `ALTER` is deferred,
   and only its _timing_ couples to 0310 (shared `AssetRow`/`assets` table → one
   deploy-drain window).
8. **PG endpoint-queries docs: banner, not delete.** The PG doc set backs the
   still-compiled PG fallback (`queries.rs` ×9 cite it); deleting it would orphan
   ~15 live code-comments. Added a SUPERSEDED banner instead. Full PG code+doc
   removal is a separate retirement task (not spawned yet — see Future Work).
9. **LP amounts left DB-pre-scaled (Decimal128(7)), not migrated to raw+decimals.**
   Investigated during the FE-decimals consistency check: LP is always classic
   7dp, values are exact, and the raw+decimals→FE pattern exists only to handle
   arbitrary soroban-token decimals (which LP has none of). Forcing uniformity =
   a prod schema migration for zero behavior change. User decision: leave as-is.

## Future Work

- **Full PG retirement** — delete PG `queries.rs` ×9 + the `Pg` datasource arm +
  the PG `endpoint-queries/` doc set + fix ~15 code-comments. Removes the rollback
  fallback; prod is already all-CH. Own task (not yet spawned). **Also folds in the
  `name`-extraction removal:** `ExtractedContractDeployment.name`, the `state.rs`
  deploy second pass, `extract_contract_data_name_writes`, `ParseOutput.contract_name_writes`,
  and the PG staging `ContractRow`/`AssetRow` `name` fields — all dead once PG is
  gone, but load-bearing for the compiled PG path until then.

- **Galexie created-vs-updated validation** (nice-to-have) — direct
  created-vs-updated confirmation of a metadata write on a representative deploy
  via the galexie archive; cross-check decimals/symbol/name against an
  independent source. Manual/ops (needs archive access), not blocking.

## Acceptance Criteria

- [x] Backfilled + read flag flipped in prod; live tests green. (Perf: confirmed
      by the live prod CH read path, no read-rows regression reported.)
- [x] Frontend renders amounts via `decimals` (single `scaleByDecimals` scaler).
- [x] Legacy name path removed (CH side) — last reader dropped (`sc.name`
      name-search) + `name` removed from the CH structs/schema/tests. Both columns
      confirmed 100% NULL + reader-less. **Only the destructive prod `ALTER DROP
COLUMN` remains** (gated on indexer deploy-drain, runs in 0310's assets
      window; 0304 owns it). PG-side `name` plumbing stays until PG retirement.
- [x] Docs synced per ADR 0032 (backend-overview + CH assets/contracts SQL; PG
      set banner-superseded).
