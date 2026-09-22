---
id: '0530'
title: 'REFACTOR: `legs` as the only leg source — backfill classic, migrate the readers, drop the pair columns'
type: REFACTOR
status: backlog
related_adr: ['0058']
related_tasks: ['0374']
tags:
  [
    'clickhouse',
    'api',
    'frontend',
    'schema',
    'phase-future',
    'effort-large',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-09-01
    status: backlog
    who: karolkow
    note: >
      Spawned from 0374. The end state and its four steps were decided and
      recorded inside 0374 while the soroban work was in flight, but never
      given a task of their own — so the "do not add new readers" rule had no
      owner. Filed now because the review of PR #438 had to add a
      `pool_kind = 0` guard that exists ONLY because the pair columns survive,
      and that guard is meant to die here.
  - date: '2026-09-07'
    status: backlog
    who: karolkow
    note: >
      Step 2 stops being work of its own: task 0518 added the pool tables to
      the targeted re-parse, so the 0540 full-range backfill emits a
      `liquidity_pools` row per changed ledger with `legs` filled, for all
      52 800 classic pools. Step 1 is answered by construction (the Rust job
      computes the surrogate natively, so the hash64-vs-cityHash64 question
      never has to be settled). The step-2 acceptance check was written and
      run early against the 7 509 classic pools the live writer had already
      migrated: 7 509 of 7 509 consistent, 0 mismatches.
  - date: '2026-09-22'
    status: backlog
    who: karolkow
    note: >
      No separate implementation: PR #455 carries step 3 and the code half of
      step 4. Step 2 verified on all 52 974 classic pools (0 mismatches). The
      two classic legs with no `assets` row were repaired before the drop.
      Closes after PR #455 deploys — see "Status 2026-09-22".
---

# REFACTOR: `legs` as the only leg source, and the pair columns dropped

## Summary

`liquidity_pools` carries two representations of the same fact. The six
**LEGACY pair columns** (`asset_a_type/code/issuer_id`, `asset_b_*`) describe a
two-leg pool, and `legs Array(Int64)` describes any pool. Classic rows fill the
pair columns; soroban rows fill `legs`. The end state — decided in 0374, not
open — is **`legs` as the only leg source in both worlds, with the six columns
dropped**.

Two facts make the pair shape wrong rather than merely redundant: 3- and 4-leg
stable pools exist on mainnet, and a pair cannot express them; and a soroban row
must put SOMETHING in the pair columns, so it writes defaults that read as
meaningful values downstream.

## Context — why this is now worth its own task

The schema says `-- LEGACY pair shape` on all six columns and carries the rule
**"do not add new readers"**, but that rule had no task behind it, so it relied
on whoever happened to read the comment.

The review of PR #438 (2026-09-01) showed what the survival costs. A soroban
registry row writes `asset_a_type = 0`, and `0` in that column means _native
XLM_ to every classic reader — so the shared asset-code predicate rendered
**every soroban pool as `XLM/XLM`**. Measured on production, live:

|                  | classic (52,677)        | soroban (497)         |
| ---------------- | ----------------------- | --------------------- |
| leg A type 0     | 11,735 — genuine native | **497 — placeholder** |
| leg B type 0     | **0**                   | **497 — placeholder** |
| both legs type 0 | **0**                   | **497**               |

A classic pool never has both legs at type 0 (a pool cannot be XLM/XLM, and
CAP-38 orders the pair so native sorts first). A soroban pool always does. So
an `XLM` filter returned 15,005 real classic pools plus **497 false ones**, and
an `XLM/XLM` filter returned 754 plus the same 497.

That was fixed by gating the predicate to `pool_kind = 0` — correct as an
interim, since the predicate reads only the legacy columns and those are
classic-only, but it is a guard that exists solely because this migration has
not happened. **It should be deleted as part of step 3, not preserved.**

## Implementation — the four steps recorded in 0374

Each step has its own verifier; they are ordered and cannot be reshuffled.

1. **Settle the surrogate question.** Determine whether our `hash64` surrogate
   equals ClickHouse's `cityHash64`. One sample comparison decides whether
   step 2 is a SQL mutation or a Rust job. (They are known NOT to be
   bit-equivalent in general — `cityhash-rs::cityhash_102_128` lower 64 bits
   versus the CH builtin — so assume the Rust job until measured otherwise.)
2. **Backfill `legs` for the ~52,620 classic rows**, versioned on each row's
   own `last_updated_ledger`. Kind 0 legs are ASSET surrogates
   (`pool_leg_asset_id`, the `lp_operation_amounts` join key), NOT the
   token-contract surrogates kind 1 uses — the id space is per-kind and
   `pool_kind` says which.

### Steps 1 and 2 ride the 0540 backfill (2026-09-07)

Neither step needs its own pass any more. Task 0518 put `liquidity_pools`
into the targeted re-parse's table list, so the full-range 0540 backfill
(`50 457 424 .. 64 317 019`) emits a registry row per changed ledger with
`legs` computed by the same staging code the live writer uses. Coverage is
total: **no classic pool has its last change below the ingest floor**
(measured — the oldest is 50 458 737), so every one of the 52 800 is reached.

Step 1 is moot: the migration goes the Rust route by construction, so whether
our surrogate equals ClickHouse's `cityHash64` never has to be decided.

Two things the run owes, both recorded in the 0518 commit: a cadenced
`OPTIMIZE TABLE liquidity_pools FINAL` (a re-emitted row ties on version with
the original ingest's, and until the merge a read picks between them
arbitrarily), and the acceptance check below.

**The step-2 acceptance check, and the trap inside it.** The criterion is
`legs` spot-verified against the pair columns while both still exist — after
step 4 the comparison is impossible. Run against the 7 509 classic pools the
live writer had already migrated: **7 509 of 7 509 consistent, 0 mismatches**.

The trap: a naive comparison reports a **52% failure rate that is not real**.
`legs` resolve through `assets`, which stores no `asset_type = 2` at all — it
collapses alphanum4 and alphanum12 into type 1 — while the legacy pair column
keeps the raw XDR distinction. Code and issuer agree exactly; only the type
digit differs. The check must therefore normalise both sides to
"native vs credit" before comparing, or it fails on every pool whose asset
code is longer than four characters.

```sql
WITH pools AS (
  SELECT pool_id, argMax(legs, last_updated_ledger) AS legs,
         argMax(asset_a_type, last_updated_ledger) AS a_type,
         argMax(asset_a_code, last_updated_ledger) AS a_code,
         argMax(asset_a_issuer_id, last_updated_ledger) AS a_iss,
         argMax(asset_b_type, last_updated_ledger) AS b_type,
         argMax(asset_b_code, last_updated_ledger) AS b_code,
         argMax(asset_b_issuer_id, last_updated_ledger) AS b_iss,
         argMax(pool_kind, last_updated_ledger) AS kind
  FROM liquidity_pools GROUP BY pool_id),
ad AS (SELECT id, any(asset_type) AS t, any(asset_code) AS c,
              any(issuer_id) AS i FROM assets GROUP BY id)
SELECT count() AS checked,
       -- `x.id = p.legs[1]`: a LEFT JOIN miss yields defaults (id 0, '', 0),
       -- which a native leg would otherwise match
       countIf(NOT (x.id = p.legs[1] AND y.id = p.legs[2]
               AND  (if(x.t=0,0,1) = if(p.a_type=0,0,1)) AND x.c=p.a_code AND x.i=p.a_iss
               AND  (if(y.t=0,0,1) = if(p.b_type=0,0,1)) AND y.c=p.b_code AND y.i=p.b_iss)
              ) AS mismatched
FROM pools p LEFT JOIN ad x ON x.id = p.legs[1]
             LEFT JOIN ad y ON y.id = p.legs[2]
WHERE p.kind = 0 AND length(p.legs) = 2
```

After the backfill, `checked` must be ~52 800 and `mismatched` must be 0.
An `id` maps to exactly one identity triple (measured: 0 of 452 592 ids carry
more than one), so the join is unambiguous.

3. **Migrate the ~612 pair-shaped call sites** (API queries, classifier,
   frontend) to `legs`. The largest piece, and the one that lands
   incrementally behind a read-time coalesce bridge. `asset_codes_predicate`
   is rewritten here to match through `legs` + the asset dimensions, which
   makes it work uniformly for both kinds — and the `pool_kind = 0` guard
   inside it disappears with the columns it protects.
4. **Drop the columns.** `ALTER TABLE liquidity_pools DROP COLUMN
asset_a_type, …` (operator) together with the removal from `init.sql`, the
   row struct, and the column-order guard. These three can only move in
   lockstep — `column_order_liquidity_pools` enforces it — and `init.sql`
   cannot lose them earlier, because the driver validates inserts against the
   live table.

## Status 2026-09-22 — carried by PR #455

**No separate implementation.** PR #455 (`feat/0374-soroban-pools-read-half`)
already does step 3 and the code half of step 4: every reader moved to `legs`,
`asset_codes_predicate` matches through `legs` + `assets` with no `pool_kind`
guard, the six columns are gone from `init.sql`, `LiquidityPoolRow` and the
column-order test, and `docs/deployment.md` carries the deploy order (API →
indexer pause → gate → `DROP` → indexer → runbook mutation A). On that branch
no production reader names `asset_a_*` / `asset_b_*`.

**Measured on production (read-only):**

- Step 2: all 52 974 classic pools checked with the query above —
  0 mismatched, 0 with a leg count other than 2, 0 empty `legs`
  (2026-09-16). 53 522 classic pools on 2026-09-22, still 0 empty.
- Classic parity, pair-column predicate vs the PR #455 `legs` predicate, on
  `XLM`, `USDC`, `KALE`, `AQUA`, `yUSDC`, `XLM/XLM`, `USDC/USDC`, `USD/USDC`,
  `XLM/USDC`, `AQUA/XLM`: identical classic pool sets (e.g. `XLM` 15 073 on
  both sides).
- Soroban: the pair-column predicate matches all 770 soroban pools as `XLM`
  and `XLM/XLM` (placeholders). The `legs` predicate matches none today,
  because 1 455 of 1 553 soroban legs are keyed on SAC contracts with no
  `assets` row. Simulated with the runbook mutation A map: `XLM` 286, `USDC`
  166, `AQUA` 156, `XLM/XLM` 9, `XLM/USDC` 27, no pool with only native legs —
  every `XLM/XLM` hit is a real pair such as XLM + yXLM. One leg stays
  unresolved (pool `8FE06922…`, token contract absent from
  `soroban_contracts`).
- Gate: 2 classic legs had no `assets` row — `XLM/PIF` and `XLM/SUR810`,
  pools created by the asset's own issuer, who needs no trustline, so no
  `assets` row was ever emitted. After the drop their code and issuer would
  exist nowhere in the database, so both rows were inserted on 2026-09-22
  (`id` = the value in `legs`, code and issuer from the pair columns).
  Orphaned classic legs since: 0 of 107 044. The writer fix is task 0571.

**Left before this task can close:**

1. Merge PR #455. As of 2026-09-22 it conflicts with `develop` in
   `persist/stage.rs` (the pool-row builder moved to `classic_pools.rs`,
   which still fills the pair fields and is also called by the checkpoint
   seed) and in `api/src/assets/queries.rs` (tests moved to a sibling file).
2. Deploy, `DROP` the six columns (operator, `chw`), run runbook mutation A.
3. On production: the `XLM` and `XLM/XLM` filters show no soroban
   placeholders, and soroban pools match by their real leg codes.
4. Archive this task.

## Not in scope

- **`pool_kind` itself stays.** It is a real user-facing distinction: the
  classic/soroban list filter, and the id rendering (`L…` SEP-23 for classic,
  `C…` for soroban — the same 32 bytes rendered the wrong way produce a
  well-formed WRONG key). Only the guard inside the asset-code predicate goes.
- **The reserve-model unification** (`liquidity_pool_snapshots` joining into
  `pool_state_changes`, ADR 0058 §3) is a separate merge about reserves, not
  about leg identity. It can happen in either order relative to this task.

## Acceptance Criteria

- [x] `legs` populated for every classic row, spot-verified against the pair
      columns before they are dropped (52 974 checked, 0 mismatched, 2026-09-16)
- [ ] No production reader references `asset_a_*` / `asset_b_*`
- [ ] `asset_codes_predicate` matches through `legs`, with no `pool_kind`
      guard, and returns the same classic results as today plus soroban pools
      matched by their real leg codes
- [ ] The `XLM` and `XLM/XLM` filters return no false positives for either
      kind (the 497 measured in #438 stay gone, and none appear for classic)
- [ ] Columns dropped from the live table, `init.sql`, the row struct and the
      column-order guard in one change
- [ ] **Docs updated** — `docs/architecture/database-schema/**` describes
      `legs` as the only leg source
