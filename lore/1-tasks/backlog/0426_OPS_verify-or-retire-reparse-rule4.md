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
      Also spotted while enumerating: `assets_pre0339` is a leftover backup table
      still sitting in prod.
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

## Evidence so far (local CH 26.3 — needs prod confirmation)

| Scenario                                                                | Result                                              |
| ----------------------------------------------------------------------- | --------------------------------------------------- |
| old rows inserted + `OPTIMIZE FINAL`, then re-parsed with a newer build | **new value survives**                              |
| re-parse split across 4 concurrent inserts                              | **new value survives** (the last insert)            |
| read via `FINAL`, no `OPTIMIZE` (how the API reads)                     | **new value survives**                              |
| two rows for one key **inside a single insert**                         | **arbitrary** — "last" is the code's emission order |

The last row is the real hazard, and it belongs to the **parser**, not the engine:
it is exactly what 0356 hit — the parser emitted the before- and after-image of a
pool per op, so one `(pool, ledger)` key got two rows with different reserves and
ClickHouse kept one at random. That fix landed. The lesson generalises to any
parse that can emit more than one row per key.

## Implementation

- [ ] Reproduce the four scenarios **against prod's ClickHouse** (a scratch
      database on the prod server, never a live table). A single-node local binary
      is not proof for a server running concurrent background merges.
- [ ] Enumerate the version-less RMT tables from `system.tables` (15 today, not
      the 12 the doc claims) and classify each: ledger-keyed, or pure function of
      an immutable input. A table in neither bucket is a genuine finding.
- [ ] If it holds: **delete rule 4**, replacing it with the narrow true rule — _a
      parse must emit at most one row per key per insert; version-less RMT keeps
      the last row inserted_. Update the `--reindex` guidance that leans on it, and
      drop the objection block from `crates/backfill-runner/README.md` clause 2.
- [ ] If it does not hold: capture the counter-example, and only then discuss
      remedies (version column, or delete-then-insert per partition — the latter is
      the simpler shape and matches how the team already thinks about it).
- [ ] Separately: `assets_pre0339` is a leftover backup table in prod. Confirm it
      is unreferenced and drop it, or record why it stays.

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
