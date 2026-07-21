---
id: '0426'
title: 'OPS: verify or retire backfills.md rule 4 — measurement says `--reindex` is safe and the rule is blocking it'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0425', '0356', '0266', '0304']
tags: [priority-medium, effort-small, clickhouse, backfill, docs]
links:
  - docs/backfills.md
history:
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Spawned from 0425, then **rewritten the same day after measuring**. The first
      version of this task asserted that 12 version-less RMT tables make re-parsing
      history unsafe, and proposed adding version columns. That assertion was copied
      from `docs/backfills.md` rule 4 and **was never measured** — the exact mistake
      0404 was re-scoped to stop repeating. Measured afterwards on ClickHouse 26.3
      (prod runs 26.3.10.60) and the claim did not reproduce in any shape tried.
      What version-less RMT actually does is keep the **last row inserted**, not an
      arbitrary one. A re-parse lands after the data it replaces by construction, so
      it wins: old rows inserted and merged, then re-parsed with a newer build →
      new value survives; re-parse split across 4 concurrent inserts → survives;
      read through `FINAL` without `OPTIMIZE` (how the API reads) → survives.
      The structural argument points the same way. Prod carries **15** version-less
      RMT tables, not 12, and each is one of two shapes: keyed by ledger (`ledgers`,
      `transactions`, `transaction_participants`, `transaction_hash_index`,
      `operations_appearances`, `operation_asset_appearances`, `operation_pools`,
      `soroban_events`, `soroban_invocations_appearances`, `nft_ownership`,
      `nft_ownership_pending`, `liquidity_pool_snapshots`) — so a re-parse only ever
      competes with its own earlier parse of the same ledger — or a pure function of
      an immutable input (`wasm_interface_metadata` keyed by `wasm_hash`; `assets`,
      whose only mutable columns `total_supply` / `holder_count` / `icon_url` are
      marked DEAD in `init.sql` and moved to `balance_aggregates` /
      `asset_enrichment`, leaving identity plus a deterministic `id`).
      Also spotted while enumerating: `assets_pre0339` (368,490 rows, 5.22 MiB) is
      still on prod. Not a leftover — 0339 kept it deliberately as a soak backup
      and its runbook warns it is NOT a full-table snapshot. 0339 is archived, so
      the soak is presumably over; that is a decision, not a cleanup.
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Re-measured on a real **ClickHouse 26.3.17.4 server** (not `clickhouse local`)
      to close the "single-node binary proves nothing about background merges" gap.
      Rule 4 does not reproduce in any shape: 40 unmerged old parts + a 4-way
      concurrent re-parse read through `FINAL` → new value wins, zero survivors;
      background merges only, never `OPTIMIZE` → new value wins (the merge dropped
      the old rows on its own); already-collapsed old data then re-parsed → new
      value wins; partial re-parse of half the keys → re-parsed keys update and the
      untouched half keeps its old value, no collateral damage.
      Rule 4 rewritten in `docs/backfills.md` and the clause-2 objection dropped
      from `crates/backfill-runner/README.md` (both on the 0425 branch). What
      remains needs prod writes and therefore an owner decision: a spot-check in a
      scratch database on the prod server, and whether `assets_pre0339` gets
      dropped.
---

# OPS: verify or retire `docs/backfills.md` rule 4

## Summary

Rule 4 says re-parsing history with a different parser build is unsafe on the RMT
tables that carry no version column, because ClickHouse "may keep the stale row".
Measurement on the prod major says the opposite: the **last row inserted** wins,
which is always the re-parse. If that holds on prod, rule 4 is not a correctness
rule — it is a **blocker on `run --reindex`**, and an expensive one.

## Why it matters

`--reindex` is the sanctioned way to rebuild history, and [0425](../active/0425_REFACTOR_delete-spent-one-off-backfill-subcommands.md)
made it the rule: no bespoke one-off binaries, re-parse the range. Rule 4 is what
made that rule look unfollowable — and it is plausibly why the bespoke binaries
were written at all. `metadata-backfill` (0304) and `pool-ids-backfill` (0266)
each carried their own partition loop, watermark file and resume logic in order to
write a single table and avoid a full re-parse. Both were deleted in 0425. If rule
4 is wrong, that entire shape was avoidable.

## Evidence (ClickHouse 26.3.17.4 **server**, background merges live)

| Scenario                                                                  | Result                                                      |
| ------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 40 unmerged old parts, then a 4-way concurrent re-parse, read via `FINAL` | **new value wins, zero survivors**                          |
| background merges only, never `OPTIMIZE`                                  | **new value wins** — the merge dropped the old rows unasked |
| old data already `OPTIMIZE FINAL`-collapsed, then re-parsed               | **new value wins**                                          |
| partial re-parse (half the keys)                                          | **re-parsed keys update; untouched half keeps its value**   |
| two rows for one key **inside a single insert**                           | **arbitrary** — "last" is the code's emission order         |

The last row is the real hazard, and it belongs to the **parser**, not the engine:
it is exactly what 0356 hit — the parser emitted the before- and after-image of a
pool per op, so one `(pool, ledger)` key got two rows with different reserves and
ClickHouse kept one at random. That fix landed. The lesson generalises to any
parse that can emit more than one row per key.

## Implementation

- [x] Reproduce on a real **server** (CH 26.3.17.4 in Docker, background merges
      live) rather than `clickhouse local`. Four shapes, all pointing the same way
      — see the evidence table above.
- [x] Enumerate the version-less RMT tables from `system.tables` — **15**, not the
      12 the doc claimed — and classify each. Every one is ledger-keyed or a pure
      function of an immutable input; **no table fell outside those two buckets**.
- [x] Rule 4 rewritten in `docs/backfills.md` around what was measured, and the
      objection block dropped from `crates/backfill-runner/README.md` clause 2.
      Also documents why the 11 versioned tables need their version (entity-keyed:
      a re-parse of old ledgers would otherwise roll current state backwards).
- [ ] **Owner decision — prod spot-check.** Repeat one shape in a scratch database
      on the prod server (`CREATE DATABASE`, throwaway table, `DROP DATABASE`),
      never a live table. Needs a prod write, so it is not done here. The local
      server matches prod's major and the mechanism is version-independent within
      26.3, so this is confirmation, not discovery.
- [ ] **Owner decision — `assets_pre0339`.** 368,490 rows / 5.22 MiB, kept by 0339
      as a deliberate soak backup (its runbook warns it is NOT a full-table
      snapshot). 0339 is archived. Drop it, or record why the soak continues.

## Acceptance Criteria

- [ ] Rule 4 is either gone from `docs/backfills.md` or restated in the form the
      measurement supports — no unmeasured claim survives in an operational guide.
- [ ] The prod reproduction is recorded in this task with the actual statements and
      outputs, not a summary.
- [ ] `run --reindex` has one unambiguous answer to "is this safe on my range",
      with no owner-judgement escape hatch.
- [ ] Docs updated — `docs/backfills.md`; `docs/architecture/**` `N/A` (no
      architecture shape change).
- [ ] API types regenerated — `N/A`: no `crates/api/**` or `Cargo.*` change.
