---
id: '0398'
title: 'Data-model hygiene: contract-surrogate redundancy + assets.contract_id/soroban_contracts.contract_id naming collision'
type: REFACTOR
status: backlog
related_adr: ['0032']
related_tasks: ['0364', '0331', '0359']
tags:
  ['phase-future', 'effort-small', 'priority-low', 'data-model', 'clickhouse']
links:
  - 'commit db521206 — perf(lore-0364): drop redundant subquery from contract arm A'
history:
  - date: '2026-07-16'
    status: backlog
    who: karolkow
    note: 'Spawned from 0364 future work — investigation surfaced while simplifying fetch_by_contract_id.'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Investigation done 2026-07-22 — verdict: document, do not rename; fold the
      rename into 0418 if it ever happens.**
      The three-place storage is deliberate, not redundant: `ids.rs` defines
      `account_id`, `contract_id` and `address_id` with byte-identical bodies and
      documents them as one shared surrogate space. Keep them as named aliases —
      they carry intent at the call site and are `#[inline]`, so the distinction
      costs nothing.
      The naming collision is real and sharper than the summary said: `contract_id`
      is `Int64` (surrogate) in 11 tables but `String` (the `C…` StrKey) in
      `soroban_contracts` and `soroban_contract_metadata`. The trap that bit 0364
      is therefore that `assets.contract_id` joins `soroban_contracts.id`, **not**
      `soroban_contracts.contract_id`.
      Rename was costed and rejected for now: the `ALTER` is metadata-only and
      trivial (148,440 + 3,850 rows), but the call sites are not — **85 hits in
      `stage.rs`, 21 in `crates/api`**, plus `init.sql`. A wide mechanical diff
      through the hottest ingest file, zero behaviour change, and a direct
      collision with 0414 (split `stage.rs`) and 0418 (asset-vocabulary
      consolidation), which are already queued against the same file. 0418 is the
      right home.
      Remaining criterion is the "just document" deliverable: schema comments in
      `init.sql`. Left undone deliberately — it edits a file both queued refactors
      touch, so it should land with them rather than ahead of them.
---

# Data-model hygiene: contract-surrogate redundancy + naming collision

## Summary

Investigate (don't necessarily change) the contract-surrogate data model.
Surfaced while simplifying `fetch_by_contract_id` in task
[0364](../active/0364_PERF_astlist-astdetail-assets-final-refactor.md)
(commit `db521206`). The **same value** — `cityhash64` of a contract's `C…`
StrKey — is stored under **three column names of two different types**, and
one of those names collides with an unrelated `String` column. This is a
readability trap (it was a bug-magnet in the 0364 subquery), not a correctness
bug. Deliverable is a **findings note + recommendation** (consolidate / rename
/ just document), NOT necessarily code.

## Context

The 0364 simplification replaced a correlated subquery

```sql
WHERE a.contract_id = (SELECT id FROM soroban_contracts WHERE contract_id = ?)
```

with the Rust-computed surrogate `ids::contract_id(strkey)` — because
**`assets.contract_id` (Int64) and `soroban_contracts.id` (Int64) always hold
the identical value**: both are `cityhash64(C-StrKey)`. Verified on prod: for a
given StrKey, `soroban_contracts.id == assets.contract_id`. The schema header
already documents this shared space ([`init.sql:11`] — `soroban_contracts.id ←
cityhash64(contract_id StrKey)`), and [`ids.rs:100-107`] frames it as one
deliberate surrogate space ("resolve back to a StrKey via `accounts` (G) /
`soroban_contracts` (C)").

## Investigation Scope

### 1. Same surrogate in ≥3 places — deliberate space or droppable storage?

`cityhash64(C-StrKey)` is stored as:

- `assets.contract_id` `Int64` ([`init.sql:268`])
- `soroban_contracts.id` `Int64` ([`init.sql`] ~L195)
- `asset_sac.sac_contract_id` `SimpleAggregateFunction(max, Int64)`
  ([`init.sql:313`])

Question: is any of this redundant _storage_ that could be dropped, or is it
the intended shared surrogate space (deterministic, so any table can recompute
the FK without a lookup)? The `ids.rs` doc treats it as one space on purpose —
this is almost certainly **document, don't drop**. Confirm and write the intent
down where a reader hits it (schema comment on each of the three columns).

### 2. Identical `ids::` functions — collapse or keep for clarity?

[`ids.rs`] `contract_id()`, `account_id()`, `address_id()` are **byte-identical
bodies** — all `hash64(strkey.as_bytes())`. They intentionally share one
surrogate space (a balance holder is a G-account OR a C-contract; task
[0331](../archive/)). Decide: three named fns for call-site clarity, or collapse
to one `strkey_id()` (+ thin type-clarity wrappers if kept)?

- **Verify no caller relies on them being distinct** — they can't, the bytes are
  equal, but confirm nothing type-checks on the fn identity.
- Note the golden test [`ids.rs:198`] already pins `address_id(g) == account_id(g)`,
  so the shared-space contract is test-locked either way.
- Likely outcome: keep the three names as intent-documenting aliases (call-site
  reads `contract_id(c)` not `strkey_id(c)`), maybe collapse bodies to one
  private helper they all call (already do via `hash64`). Cheap; low value.

### 3. Naming collision — `assets.contract_id` vs `soroban_contracts.contract_id`

**Same column name, different type and meaning:**

- `assets.contract_id` — `Int64` **surrogate** ([`init.sql:268`])
- `soroban_contracts.contract_id` — `String` **`C…` StrKey** ([`init.sql:196`])

This is the genuine readability trap — the 0364 subquery bug-magnet came partly
from mentally swapping the two. A rename (e.g. `assets.contract_surrogate` or
`soroban_contracts.strkey`) would fix it, **but** it's a schema + ingest + every
query touching those columns change, and re-backfill / careful migration on a
`ReplacingMergeTree`. Weigh cost vs clarity; recommend the cheapest option that
kills the trap (may just be a loud schema comment + a doc note, not a rename).
Per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md): if
any column IS renamed, update `docs/architecture/database-schema/**` in the same
PR.

## Acceptance Criteria

- [x] Findings note written — **deliberate, not redundant.** `ids.rs:84-107`
      defines `account_id`, `contract_id` and `address_id` with byte-identical
      bodies (`hash64(strkey.as_bytes())`), and the doc comment states the intent:
      "one shared surrogate space; resolve back to a StrKey via `accounts` (G) /
      `soroban_contracts` (C)". Confirmed against the live schema: the same
      `cityhash64` value is stored as `contract_id Int64` in 11 tables, plus
      `sac_contract_id SimpleAggregateFunction(max, Int64)` (`asset_sac`) and
      `caller_contract_id Nullable(Int64)`. Nothing to drop.
- [x] Recommendation on the `ids::` fn trio — **keep as named aliases.**
      Collapsing to one function would erase intent at the call site
      (`account_id(x)` and `contract_id(x)` say different things about the
      argument) and buys nothing at runtime: all three are `#[inline]` and
      compile to the same call. The distinctness is documentation, not dispatch.
- [x] Recommendation on the naming collision — **document, do not rename.**
      The collision is real and worse than the summary states: `contract_id` is
      `Int64` (the surrogate) in 11 tables but `String` (the actual `C…` StrKey)
      in `soroban_contracts` and `soroban_contract_metadata`. So
      `assets.contract_id` joins `soroban_contracts.`**`id`**, never
      `soroban_contracts.contract_id`, despite the identical name.
      Cost of the rename path, measured: `RENAME COLUMN` itself is cheap
      (metadata-only on MergeTree; 148,440 and 3,850 rows) — the cost is in the
      call sites: **85 occurrences in `stage.rs`, 21 in `crates/api`**, plus
      `init.sql`. That is a wide, purely-mechanical diff across the hottest
      ingest file, carrying real review risk for zero behaviour change, and it
      would collide with 0414 (splitting `stage.rs`) and 0418 (asset-vocabulary
      consolidation) — both of which touch the same file and are already queued.
      Better sequencing: fold the rename into 0418, which exists to consolidate
      exactly this kind of vocabulary, rather than doing it standalone here.
- [ ] If the recommendation is "just document": land the schema comments / doc
      note in this task. If "rename" or "collapse": spawn a follow-up impl task
      (this task stays investigation-only).
- [ ] **Docs updated** — N/A unless a column is renamed → then update
      `docs/architecture/database-schema/**` per ADR 0032.
- [ ] **API types regenerated** — N/A (internal surrogate; no API surface).

## Notes

- Pure data-model hygiene spun off a perf task. Zero correctness impact today —
  the values are provably equal, which is exactly why 0364 could drop the
  subquery. The risk is future readers, not current rows.
- Bias toward the lazy outcome: the shared surrogate space is intentional and
  test-pinned; the highest-value cheap fix is probably schema comments on the
  three columns + renaming ONLY if the collision keeps biting.
