---
id: '0426'
title: 'OPS: verify or retire backfills.md rule 4 — measurement says `--reindex` is safe and the rule is blocking it'
type: OPS
status: completed
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
  - date: 2026-07-21
    status: completed
    who: karolkow
    note: >
      Closed the same day — the task was written to schedule work that turned out
      to be doable immediately, so it never needed a queue slot. The prod
      confirmation needed **no scratch write**: production already contained the
      experiment (127k path-payment ops in a fully-merged old range carrying
      `pool_ids` the original parser could not emit), so it was settled read-only.
      Rule 4 is rewritten in `docs/backfills.md`, the README clause-2 objection is
      gone, and the one piece of machinery it was propping up (`assets_id_backfill`'s
      staging + `EXCHANGE`, justified in its own header by "no version column, so a
      plain re-INSERT can't override") was already deleted by 0425.
      Not closed here, handed to **0400** instead of held open: `assets_pre0339`
      and the rest of the prod-vs-`init.sql` drift found while enumerating. That is
      0400's subject, not this task's.
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
- [x] **Confirmed on production — read-only, no scratch write needed.** A write
      test was unnecessary: prod already contains the experiment, run over real
      merge history. `operations_appearances` is version-less RMT and was
      originally ingested by a **pre-0261 parser that emitted no `pool_ids` at all
      on path-payment ops**; later writes from a post-0261 build (0266's backfill,
      then 0379's `--reindex`) targeted the same keys. Sampling ledgers
      50,500,000–50,510,000 — deep in the original range:

      | | |
      |---|---|
      | raw rows / distinct keys | 4,429,575 / 4,429,575 → **fully merged, one row per key** |
      | type 2 (PathPaymentStrictReceive) | 1,301,965 ops, **58,488 carry `pool_ids`** |
      | type 13 (PathPaymentStrictSend) | 566,419 ops, **68,546 carry `pool_ids`** |

      For each of those ~127k keys the original parse wrote a row with empty
      `pool_ids`, a later build wrote a different row for the same key, merges ran
      to completion — and **the later row is the one that survived**. That is
      precisely the case rule 4 said could keep the stale row. It does not.
      Stronger than any fixture: 19+ months of real merge history, at scale, on the
      actual server.

- [ ] **Owner decision — `assets_pre0339`.** 368,490 rows / 5.22 MiB, kept by 0339
      as a deliberate soak backup (its runbook warns it is NOT a full-table
      snapshot). 0339 is archived. Drop it, or record why the soak continues.

## What this makes unnecessary

Rule 4 was not only a doc claim — it was the stated reason for real machinery.
`assets_id_backfill`'s header said it plainly: _"`assets` is a
ReplacingMergeTree with NO version column, so a plain re-INSERT can't
deterministically override a row"_, and therefore built a staging table and
`EXCHANGE`d it, which in turn is why the guide lists it under **"must STOP the
indexer"**. That reasoning is now measured false: on a version-less table a plain
re-INSERT does override. The pass was deleted in 0425 anyway.

Checked the rest rather than assuming: of the six tables the surviving staging +
`EXCHANGE` passes touch, **`assets` was the only version-less one**. `accounts`,
`lp_positions`, `nfts`, `nfts_pending` and `soroban_contracts` all carry a
version, and `repair-tier1` must lower a value (`first_seen_ledger`) **without**
moving that version — a re-INSERT genuinely cannot do that. So the remaining
`EXCHANGE` machinery, and the indexer stop it forces, is justified. The
overengineering was one pass, and it is already gone.

## Acceptance Criteria

- [x] Rule 4 restated in `docs/backfills.md` in the form the measurement supports
      — no unmeasured claim left in an operational guide.
- [x] Production evidence recorded above with the actual counts, obtained
      **read-only**; no scratch database, no prod write.
- [ ] `run --reindex` has one unambiguous answer to "is this safe on my range",
      with no owner-judgement escape hatch.
- [ ] Docs updated — `docs/backfills.md`; `docs/architecture/**` `N/A` (no
      architecture shape change).
- [ ] API types regenerated — `N/A`: no `crates/api/**` or `Cargo.*` change.
