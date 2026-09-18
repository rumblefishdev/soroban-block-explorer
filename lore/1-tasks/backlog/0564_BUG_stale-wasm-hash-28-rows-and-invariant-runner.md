---
id: '0564'
title: 'BUG: 28 contracts carry a pre-upgrade wasm hash — repair them, and put the 0320 invariant on a schedule'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0320', '0325', '0316', '0304', '0392', '0425', '0548']
tags:
  [backend, clickhouse, data-quality, soroban, priority-medium, effort-small]
links: []
history:
  - date: '2026-09-18'
    status: backlog
    who: karolkow
    note: >
      Found while auditing whether other extractors share task 0210's
      skipped-`removed` defect. Separate from 0325, which owns reclassification
      and the pool registry; this one is the stale rows and the missing runner.
---

# 28 contracts show a pre-upgrade wasm hash

## Summary

`soroban_contracts.wasm_hash` is meant to be the contract's current on-chain
executable. For 28 contracts it is the one before their last upgrade. The
upgrade event is in our data and carries the right hash; only the row write was
dropped, for 13 days in July 2026. Task 0320's two safety nets — a recovery
backfill and its invariant as a tripwire — were both removed within a month of
landing, so nothing repaired or reported it.

## Measured (production, 2026-09-18)

- **Violations of 0320's invariant** (`lore/1-tasks/archive/0320_BUG_wasm-upgrade-not-reclassified/notes/G-invariant-wasm-hash-current.sql`): **28**. It read 0 after the 2026-06-29 repair run (task 0326).
- **Population:** 1,758 contracts have an `executable_update` SYSTEM event (5,300 events), of 147,271 non-SAC contracts carrying a hash.
- **The window is sharp:** every contract whose last upgrade fell in ledgers 63,308,319–63,495,140 is stale (28 of 28); none outside it (0 of 1,730). Last clean upgrade before: 63,307,435 (2026-07-03). First clean after: 63,504,477 (2026-07-16).
- **Chain check** (`getLedgerEntries`, contract instance executable): 68 of 68 sampled clean upgraders match, 40 of 40 never-upgraded match, 0 of 28 violators match. In all 28 our own event's new hash equals the chain's.
- One of the 28 is a registered Soroban pool (`CBENABXP…`), so its pool page inherits the wrong code identity.

## Cause

1. Task 0304 dropped `soroban_contracts.name` (2026-07-03) while the prefetch that feeds the upgrade writer still selected it, so every prefetch failed with `UNKNOWN_IDENTIFIER`.
2. That prefetch was fail-open: on error it returned an empty map, and `build_wasm_upgrade_rows` skips a contract with no prior row (`crates/db-clickhouse/src/persist/stage.rs:456`) rather than NULL-clobber its identity columns. Every upgrade in the window was silently skipped.
3. Fixed on 2026-07-15 (lore-0392, `9cb3834e`). The prefetch is fail-closed since 2026-09-14 (lore-0548, `crates/db-clickhouse/src/persist.rs:130`), so the window cannot reopen the same way.
4. The `wasm-upgrade-backfill` pass that 0320 named as its recovery was broken by the same column drop and deleted on 2026-07-21 (task 0425) without a re-run.
5. The invariant exists only as a `.sql` file in 0320's notes; the repo has no ClickHouse invariant runner.

## What a user sees on those 28 contract pages

`GET /v1/contracts/{id}` and `/interface` join `wasm_interface_metadata` on the stored hash (`crates/api/src/contracts/queries.rs:480-501`, `:806-822`), so: the pre-upgrade hash shown as a present-tense fact, the old ABI, a decompilation of the old binary, and a "No self-upgrade" chip derived from the old binary's imports — an affirmative claim that can now be false.

## Implementation

- [ ] **Repair the 28 rows in ClickHouse, no re-parse.** `INSERT … SELECT` the new hash from each contract's latest `executable_update` event, carrying every other column from the current row (`soroban_contracts` is `ReplacingMergeTree` keyed on `contract_id` with no version column, so the last-inserted row wins a merge). Statement and its read-only `SELECT` prepared 2026-09-18 under `.artifacts/wasm-hash-repair/`. Spot-checked against the chain. **A production write — the user runs it.**
- [ ] Re-run the invariant afterwards: 28 → 0.
- [ ] **Give the invariant a home that runs.** Task 0325's open acceptance criterion already asks for the transition scan as a runnable check in the release routine; this one belongs beside it, or in whatever CH invariant runner lands first. Decide which, then wire it.
- [ ] Re-check `contract_type` for the 28: the upgrade path carries the old classification forward on purpose (`stage.rs:414-417`). Reclassification is task 0325's, but say whether any of the 28 flipped class.

## Acceptance Criteria

- [ ] The 0320 invariant reads 0 on production.
- [ ] The invariant runs on a schedule, and a non-zero result reaches a human.
- [ ] Task 0325 records which of the 28, if any, need reclassification.

## Out

- Reclassification on upgrade and the pool registry that still lists an upgraded-away pool — task 0325.
- The RMT whole-row clobber class — task 0316.
