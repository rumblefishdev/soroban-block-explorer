---
id: '0304'
title: 'FEATURE: 0297 metadata follow-ups — backfill, deploy/flip, validation, frontend amounts, cleanup'
type: FEATURE
status: active
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

- [ ] Backfill `soroban_contract_metadata` for existing contracts (table starts
      empty). Decide: re-parse historical ledgers vs RPC `getLedgerEntries` dump —
      archived/evicted instances need re-parse (RPC ~7-day retention).
- [ ] Direct created-vs-updated confirmation on a representative deploy via the
      galexie archive (RPC retention can't reach old deploys; see 0297 Option B).

### Validation & perf

- [ ] Validate read JOIN / `FINAL` / `argMax` cost on the CH snapshot for
      contract detail + asset detail/list before flipping the read flag
      (`read_rows` quota history).
- [ ] Live integration tests (real CH): metadata is written + read-composed.

### Deploy

- [ ] Flip the read path to surface metadata in prod (gated on perf + backfill).

### Frontend

- [ ] Render amounts using `decimals` (e.g. "1.5 USDC") across asset/amount views.
- [ ] Surface `symbol` / `name` where useful.

### List endpoints

- [ ] Contract LIST name-search still hits the dead `sc.name` column (empty →
      effectively contract_id-only). NOT repointed to the metadata side table:
      the contract API doesn't surface a name (0297 #3), so name-search is low
      value — decide drop-the-name-clause vs repoint. (The assets COALESCE
      already reads `m.name` from the side table.)

### Name-search / column-drop — BLOCKED on 0243 (search+assets → CH)

The legacy `name` columns (`soroban_contracts.name`, `assets.name`) have **no
writer since 0297** (empty going forward) but cannot be DROPped yet because the
new source (`soroban_contract_metadata`) is **CH-only** while these readers run
on **Postgres**:

- [ ] Repoint PG global-search label (`search/queries.rs` `COALESCE(sc.name,'')`)
      — blocked: `search` is PG-only (no CH variant); needs search ported to CH
      (task 0243) or it has no metadata source.
- [ ] Repoint PG assets list `a.name` (`assets/queries.rs`) — same blocker.
- [ ] Redefine PG `soroban_contracts.search_vector` (GENERATED from `name`, ADR 0042) so it no longer depends on the column.
- [ ] After backfill + read-flip + the above: `ALTER TABLE … DROP COLUMN name`
      on CH (`soroban_contracts`, `assets`) and the PG migration.

(CH-side: the assets COALESCE already reads `m.name` from the side table.
Contract name-search was NOT repointed — it stays on the dead `sc.name`
column, see the List-endpoints item above.)

### Cleanup (code)

- [x] Legacy `contract_name_writes` / `Symbol("name")` path fully un-threaded —
      done in 0297 PR (`extract_contract_data_name_writes`, deploy second pass,
      `ParseOutput.contract_name_writes`, PG `apply_contract_name_writes` + the
      `assets.name` mirror, all plumbing + tests). `ExtractedContractDeployment.name`
      kept (now always `None`) until the column drop above.

### Docs (ADR 0032)

- [ ] Finish sync: `backend/backend-overview.md`, and the reference SQL snapshots
      (`11_get_contracts_by_id`, `08`/`09_get_assets*`) in both `endpoint-queries`
      sets. (0297 did `database-schema-overview` + `xdr-parsing-overview`.)

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

The rest of the bundle (perf validation, read-flip — entangled with 0243,
frontend amounts, name-search cleanup + column drop, ADR-0032 docs sync) is
untouched.

## Acceptance Criteria

- [ ] Backfilled + read flag flipped in prod; perf validated; live tests green.
- [ ] Frontend renders amounts via `decimals`.
- [ ] Legacy name path removed; vestigial columns dropped.
- [ ] Docs fully synced per ADR 0032.
