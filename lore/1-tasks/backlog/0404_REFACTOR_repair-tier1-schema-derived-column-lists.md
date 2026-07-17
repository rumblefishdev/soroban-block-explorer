---
id: '0404'
title: 'REFACTOR: repair-tier1 — schema-derived column lists + parity assert (kill the drift-fragility 0388 was a symptom of); test rebuild_soroban_contracts'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0388', '0228', '0379']
tags: [priority-medium, effort-small, clickhouse, repair-tier1, robustness]
links:
  - crates/backfill-runner/src/repair_tier1.rs
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned when 0388 was closed. 0388 fixed one stale column (`name` in the
      soroban_contracts repair) but not the class: all five repairs still hardcode
      their column lists, so the next schema drift reproduces the same bug. Flagged
      during the 0359 backfill review as a non-blocking follow-up; verified unowned
      2026-07-17.
---

# REFACTOR: repair-tier1 — schema-derived column lists + parity assert

## Summary

`repair_tier1.rs` hardcodes the column list of every staging INSERT. 0388 was
what that costs: a column (`name`) that never existed in `soroban_contracts`
sat in the INSERT until a pre-run check caught it, and it would have aborted the
`deployer_id` / `deployed_at_ledger` reconstruction — the sharpest correctness
need after a `--reindex` backfill.

## Context

Spawned from [0388](../archive/0388_BUG_repair-tier1-soroban-contracts-name-mismatch.md).
The fix there was a one-line column removal, correct but local. The structural
point stands: five hardcoded lists against a schema that is known to drift
(prod vs `init.sql` — see 0400), each guarded only by someone remembering to run
a manual parity check before a prod run.

Two failure modes, asymmetric:

- **Extra column in the INSERT** → loud runtime `unknown column` abort. This is
  what 0388 was. Recoverable, no data harm.
- **Prod column missing from the INSERT** → **silent `DEFAULT` wipe** of that
  column on `EXCHANGE`. No error. This is the dangerous one, and it is exactly
  what a manual check is worst at catching.

## Implementation

- [ ] Derive each repair's column list from the live schema (`system.columns`, or
      the `create_staging_like` clone that already reads it) instead of a string
      literal.
- [ ] If a literal list is kept for clarity, add a **parity assert** that fails
      loudly before the write when the list and the live table disagree in either
      direction — the silent-wipe direction is the one that needs it.
- [ ] Add a test for `rebuild_soroban_contracts`. It is currently **untested** —
      only `rebuild_accounts` has coverage (`clickhouse_rebuild_accounts_*`),
      despite `rebuild_soroban_contracts` being the function 0388 was about.
- [ ] `nfts` / `nfts_pending` already share one parameterized `rebuild_nfts` and
      have identical schemas — keep that, it is why one list correctly covers two
      tables.

## Acceptance Criteria

- [ ] A prod column absent from a repair's INSERT fails loudly instead of
      silently DEFAULT-wiping the column.
- [ ] `rebuild_soroban_contracts` has test coverage equivalent to
      `rebuild_accounts` (dry-run leaves live untouched + real run writes the
      corrected value).
- [ ] Adding a column to any of the 5 tables does not require editing a hardcoded
      list — or, if it does, the parity assert catches a missed edit.
- [ ] Docs updated — mark each `docs/architecture/**` file updated or
      `N/A — reason` (likely N/A: ops tooling, no architecture shape change).
- [ ] API types regenerated — N/A unless `crates/api/**` or `Cargo.*` change.
