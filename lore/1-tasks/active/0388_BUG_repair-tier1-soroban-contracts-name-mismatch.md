---
id: '0388'
title: 'repair-tier1: soroban_contracts INSERT references non-existent `name` column — align before prod run'
type: BUG
status: active
related_adr: []
related_tasks: ['0228', '0359', '0379']
tags:
  [
    'phase-future',
    'effort-small',
    'priority-high',
    'clickhouse',
    'repair-tier1',
  ]
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: claude
    note: 'Spawned from 0359 oaa backfill — pre-run schema-compat check of repair-tier1.'
  - date: 2026-07-14
    status: active
    who: claude
    note: 'Activated to implement the stale-`name`-column fix (remove from soroban_contracts INSERT+SELECT).'
---

# repair-tier1: soroban_contracts `name` column mismatch

## Summary

`backfill-runner repair-tier1` rebuilds the `soroban_contracts` staging row with an
INSERT column list that includes `name`, but `name` does **not** exist in
`soroban_contracts` (absent from `init.sql`, from `SorobanContractRow`, and from prod
`system.columns` — all verified 2026-07-14). `create_staging_like` clones the real
8-column schema, so the staging table also lacks `name`; the
`INSERT INTO … (…, name) SELECT …, sc.name` therefore fails at runtime with
**unknown column `name`**. Net effect: the `deployer_id` + `deployed_at_ledger`
reconstruction — the sharpest correctness need after the 0359 `--reindex` backfill —
will NOT run until this is fixed.

The other four repairs have **exact column parity** with prod and are safe as-is:
`accounts.first_seen_ledger`, `lp_positions.first_deposit_ledger`,
`nfts.minted_at_ledger`, `nfts_pending.minted_at_ledger`.

## Context

The 0359 backfill runs `run --reindex`, which re-parses already-ingested ledgers and
re-writes ~22 tables via `write_ledger`. The reconstruction columns
(first_seen / first_deposit / minted_at / deployer) are written **window-local** by
the parser and ride on the RMT winner (each table is
`ReplacingMergeTree(<latest-activity-ledger>)`), so they are not authoritative after a
bulk re-ingest. `repair-tier1` (task 0228 Phase 5) is the standard reconstruction pass
to run afterward — it aggregates the true `MIN` / deploy-identity from the intact,
deterministic raw appearance tables (`transaction_participants`,
`operations_appearances`, `nft_ownership*`) via staging + `EXCHANGE TABLES`.

A pre-run schema-compat check (prod `system.columns` vs the INSERT column lists in
`repair_tier1.rs`) surfaced the `name` mismatch on `soroban_contracts` only. Contract
name lives in the separate `soroban_contract_metadata` table — the `name` reference in
the soroban_contracts repair is stale.

## Implementation

- `crates/backfill-runner/src/repair_tier1.rs`, soroban_contracts repair fn (~L307):
  remove `name` from the staging INSERT column list **and** from the `SELECT`
  (`sc.name`).
- Re-run the column-parity check for all 5 tables against prod `system.columns` before
  the real run (guard against any further drift).
- Rebuild the box binary (cross-compile x86_64 via cargo-zigbuild, scp to ch-prod-01 —
  see [[project_0359_backfill_run]] toolchain notes) so the fixed binary is staged for
  the post-backfill run.

## Acceptance Criteria

- [ ] `repair-tier1 --dry-run` completes on prod with **no unknown-column error** for
      any of the 5 tables
- [ ] soroban_contracts `deployer_id` + `deployed_at_ledger` reconstruction reports a
      non-zero corrected-row count in the dry-run stats
- [ ] Column-parity verified: every prod column of each of the 5 tables is present in
      that table's repair INSERT (no silent `DEFAULT`-wipe of a drifted column)
- [ ] Fix landed on develop; box binary rebuilt + staged

## Notes

- **Run order** (post-0359 backfill): indexer STOP → `repair-tier1 --dry-run` →
  `repair-tier1` → `nft-reclassify` → validate oaa vs Horizon → indexer START.
  repair-tier1 must run **before** any `OPTIMIZE FINAL` (e.g. nft-reclassify) so the
  deployer un-FINAL history (`argMin(deployer_id, wasm_uploaded_at_ledger)` over
  non-NULL rows) is still present to reconstruct from.
- Prod `soroban_contracts` columns (2026-07-14): `id, contract_id, wasm_hash,
wasm_uploaded_at_ledger, deployer_id, deployed_at_ledger, contract_type, is_sac` (8).
- This is a **loud** failure (runtime unknown-column error), not a silent wipe — so
  running the current binary can't corrupt data, it just aborts the soroban_contracts
  step. Fix is required only to make the deployer reconstruction actually execute.
