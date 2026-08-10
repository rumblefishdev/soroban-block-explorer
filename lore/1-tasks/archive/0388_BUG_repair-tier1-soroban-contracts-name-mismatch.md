---
id: '0388'
title: 'repair-tier1: soroban_contracts INSERT references non-existent `name` column — align before prod run'
type: BUG
status: completed
related_adr: []
related_tasks: ['0228', '0359', '0379', '0404']
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
    who: stkrolikiewicz
    note: 'Spawned from 0359 oaa backfill — pre-run schema-compat check of repair-tier1.'
  - date: 2026-07-14
    status: active
    who: stkrolikiewicz
    note: 'Activated to implement the stale-`name`-column fix (remove from soroban_contracts INSERT+SELECT).'
  - date: 2026-07-17
    status: completed
    who: stkrolikiewicz
    note: >
      Closed. The fix was one line (`861b445f`, PR #336, merged 07-14 as
      `7a99423c`: drop `name` from the soroban_contracts staging INSERT + SELECT),
      but the ACs deliberately asked for prod evidence, not a merge — and that
      evidence only arrived with the 0379 Phase-3 drain on **2026-07-16**, which
      this file was never updated to record.
      **It worked.** All five repairs ran with no unknown-column error, `dry ==
      real`: accounts 14.33M, lp_positions 107728, nfts 12835, nfts_pending 439062,
      **soroban_contracts 129121** — the last being the `deployer_id` /
      `deployed_at_ledger` reconstruction this task existed to unblock, and its
      non-zero corrected-row count is AC2 exactly. Numbers live in the 0379 archive.
      Column-parity (AC3) is the one AC a successful run does NOT prove — a *missing*
      prod column would silently `DEFAULT`-wipe rather than abort — so it was
      re-verified at close against `init.sql`: accounts 6/6, lp_positions 5/5, nfts
      8/8, nfts_pending 8/8, soroban_contracts 8/8, zero missing. Both gates were
      absent from this file and are now marked N/A with reasons (#336 touched exactly
      one non-API file, +2/-3).
      Spawned **0404** for the class this was a symptom of: all five repairs still
      hardcode their column lists, and `rebuild_soroban_contracts` — the function
      this whole task was about — has **no test** (only `rebuild_accounts` is
      covered). The fix here was correct but local.
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

- [x] `repair-tier1 --dry-run` completes on prod with **no unknown-column error** for
      any of the 5 tables — **met 2026-07-16.** All five ran in the 0379 Phase-3
      drain, `dry == real`, EXCHANGE row-count-preserving, `first_seen > last_seen` = 0.
- [x] soroban_contracts `deployer_id` + `deployed_at_ledger` reconstruction reports a
      non-zero corrected-row count in the dry-run stats — **met: 129121 rows.**
      The reconstruction this task existed to unblock actually ran.
- [x] Column-parity verified: every prod column of each of the 5 tables is present in
      that table's repair INSERT (no silent `DEFAULT`-wipe of a drifted column)
      — **met, verified twice.** Against prod `system.columns` 2026-07-14 (the check
      that surfaced this bug: `name` was the only mismatch across all 5 tables), and
      re-verified against `init.sql` 2026-07-17 at close: accounts 6/6, lp_positions
      5/5, nfts 8/8, nfts_pending 8/8, soroban_contracts 8/8 — **zero prod columns
      missing from any INSERT**, so no silent-wipe exposure. Note `nfts` and
      `nfts_pending` have identical schemas and share one parameterized
      `rebuild_nfts`, which is why 4 INSERT statements correctly cover 5 tables.
- [x] Fix landed on develop; box binary rebuilt + staged — **met.** `861b445f` via
      PR #336, merged 2026-07-14 as `7a99423c`; box binary cross-built (zigbuild)
      and scp'd to the prod box with #336 + 0394 before the 07-16 drain.
- [x] **Docs updated** — N/A. Backfill/ops tooling bug fix; no schema, API,
      ingestion-pipeline or infrastructure shape change, which CLAUDE.md names as a
      legitimate N/A case.
- [x] **API types regenerated** — N/A. PR #336 touched exactly one file
      (`crates/backfill-runner/src/repair_tier1.rs`, +2/-3); no `crates/api/**`,
      no `Cargo.toml` / `Cargo.lock`, no `libs/api-types/**`.

## Future Work

- **0404** — the class, not the instance: all five repairs still hardcode their
  column lists, so the next schema drift reproduces this bug. Worse in the other
  direction — a prod column _missing_ from an INSERT is a **silent `DEFAULT` wipe**,
  not a loud abort. Also carries the missing `rebuild_soroban_contracts` test.

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
