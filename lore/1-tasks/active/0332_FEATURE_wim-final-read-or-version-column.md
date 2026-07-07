---
id: '0332'
title: 'FEATURE: make contract-detail reads of wasm_interface_metadata merge-correct (FINAL or version col)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0326', '0327', '0320']
tags:
  [
    clickhouse,
    api,
    contract-detail,
    replacingmergetree,
    correctness,
    priority-medium,
    effort-small,
  ]
links: []
history:
  - date: 2026-06-27
    status: backlog
    who: karolkow
    note: >
      Spawned from 0326 (upgradeable-backfill prod run). 0326's OPTIMIZE step is
      sufficient for steady state; this is the fundamental hardening so the read
      path is correct regardless of merge timing. Created on the 0326 branch (not
      develop) because develop had unrelated uncommitted WIP in the same API files.
  - date: '2026-07-01'
    status: active
    who: karolkow
    note: >
      Promoted to active. Bundled with 0299 onto a single branch
      (feat/0299_0332_routes-consolidation-and-wim-read).
  - date: '2026-07-07'
    status: active
    who: karolkow
    note: >
      Un-bundled from 0299 — 0299 shipped on its own branch as PR #320 and was
      archived. 0332 stays active for a fresh session. Branch
      feat/0332_wim-final-read-or-version-column already created off develop.
      Spec enriched with verified findings + exact locations + two scope
      decisions (delete dead PG contract-read; include the backfill hash-guard)
      — see "Execution plan (verified 2026-07-07)" below. No code written yet.
---

# FEATURE: contract-detail wim read must be merge-correct

## Summary

`wasm_interface_metadata` is `ReplacingMergeTree` with **no version column**. The
API contract-detail path reads it via `LEFT JOIN wasm_interface_metadata wim`
**without `FINAL`** (3 sites), relying on the old invariant "content is immutable
per `wasm_hash`, so any duplicate is byte-identical". The 0327 `upgradeable-backfill`
(run under 0326) breaks that invariant transiently: it re-INSERTs an existing
`wasm_hash` with a _different_ `metadata` (now carrying `upgradeable`), so old + new
rows coexist until a background merge. A non-`FINAL` read in that window can pick the
keyless row → chip flickers to Unknown. Not data loss; a timing-dependent read.

0326 closes the window with a one-shot `OPTIMIZE TABLE wasm_interface_metadata FINAL`.
This task removes the latent landmine permanently.

## Context

- Prod engine confirmed `ReplacingMergeTree` (no version) — table is tiny (3,720 rows).
- Code comment at `crates/api/src/contracts/queries_ch.rs:283` is **factually wrong**:
  claims "plain `MergeTree`, so it must NOT carry `FINAL`". Prod is RMT → `FINAL` is
  legal and correct. The comment dates from commit `d258c93b fix(lore-0327): unbreak
CH contract detail endpoint`, which removed a `FINAL` that hit `ILLEGAL_FINAL` —
  evidence the table (or some env) was plain `MergeTree` at that time. So: verify the
  engine across ALL deploy targets BEFORE re-adding `FINAL`.
- Read sites without `FINAL`: **2 live ClickHouse sites** —
  `queries_ch.rs:310` (`fetch_contract`) and `queries_ch.rs:559`
  (`fetch_wasm_interface`). The task originally listed a 3rd (`queries.rs:252`)
  but that is the **dead Postgres** path (`PgPool`/`sqlx`) — `FINAL` is
  ClickHouse-only, so it is not a FINAL site. See the Execution plan below.
  (Line numbers verified 2026-07-07; may drift.)

## Implementation

1. Verify `wasm_interface_metadata` engine is `ReplacingMergeTree` in every deploy
   target (prod ✓ 2026-06-27; check local/CI init.sql ✓; any other env).
2. Add `FINAL` to the `wim` join at the 3 read sites (tiny table → cheap).
3. Fix the stale "plain `MergeTree`" comment to say RMT-no-version + why `FINAL`.
4. (Alternative, heavier — only if `FINAL` proves too costly anywhere) add a version
   column + `ReplacingMergeTree(version)` via migration. Default to option 2.

## Acceptance Criteria

- [ ] Engine verified RMT in all envs; `FINAL` added to the 3 wim reads.
- [ ] Stale "plain MergeTree" comment corrected.
- [ ] Contract-detail chip is correct immediately after an `upgradeable-backfill`
      re-insert, with NO `OPTIMIZE` needed (test: insert divergent metadata for an
      existing hash, read without merge, assert keyed row wins).
- [ ] No regression on the contract-detail hot path (latency sanity).

## Devil's-advocate follow-ups (2026-06-29, from the 0326 prod run)

Two concrete hardenings surfaced by the adversarial review of the 0326 run:

1. **Priority bump: low → medium.** The non-`FINAL` read is a latent landmine, not
   cosmetic. The 0326 `OPTIMIZE` is one-time; the live indexer keeps INSERTing `wim`
   rows, and the moment any write produces a _divergent_ `metadata` for an existing
   `wasm_hash` (a metadata-schema change, a re-parse), the stale-read window reopens
   with no recurring `OPTIMIZE`. Fix the read, don't rely on the merge.

2. **Content-address verify in `upgradeable-backfill` (and the live parser path).**
   `crates/backfill-runner/src/upgradeable_backfill.rs` runs `wasm_imports_upgrade_fn`
   on whatever bytecode RPC returns and writes the derived `upgradeable` flag to prod
   **without checking `sha256(cce.code) == requested wasm_hash`**. A corrupt / truncated
   / tampered public-RPC response would write a WRONG flag with no detection. Add the
   hash-equality guard before trusting the import scan (a few lines); apply the same
   guard wherever the live path derives `upgradeable` from fetched bytecode.

Acceptance for these:

- [ ] `sha256(fetched code) == wasm_hash` asserted before the import scan; mismatch →
      skip + warn (mirrors the 0326 `malformed_metadata` posture), never write a flag.
- [x] tags bumped to `priority-medium` (already `priority-medium` in frontmatter).

---

## Execution plan (verified 2026-07-07 — fresh-session ready)

> A prior session investigated the whole surface and made two scope decisions
> (see below) but wrote **no code**. Everything needed to execute is here; line
> numbers were verified on develop 2026-07-07 and may drift — re-grep before
> editing.

### Engine — VERIFIED `ReplacingMergeTree` everywhere (AC#1 satisfied)

- CH schema `crates/db-clickhouse/schema/init.sql:117-122`:
  `ENGINE = ReplacingMergeTree ORDER BY (wasm_hash)`, no version column →
  `FINAL` is legal and correct.
- Prod: confirmed RMT (2026-06-27, this task's Context).
- `crates/db/migrations/0002_identity_and_ledgers.sql:41` is the **Postgres**
  schema — DEAD (PG retired, CH-only). Ignore it.
- ⇒ No blocker: `FINAL` is legal in every live target.

### The read fix — 2 LIVE ClickHouse sites (not 3)

1. `crates/api/src/contracts/queries_ch.rs:310` — `fetch_contract` (drives the
   upgradeable chip). Add `FINAL`.
2. `crates/api/src/contracts/queries_ch.rs:559` — `fetch_wasm_interface`
   (interface metadata). Add `FINAL`.

**Exact syntax** — the canonical query in
`docs/architecture/database-schema/endpoint-queries-clickhouse/11_get_contracts_by_id.sql:70`
already specifies it (docs are AHEAD of code):

```
LEFT JOIN wasm_interface_metadata wim FINAL ON wim.wasm_hash = sc.wasm_hash
```

i.e. append ` FINAL` after the `wim` alias, before `ON` — mirrors the existing
`FROM soroban_contracts sc FINAL`. Because docs already document `wim FINAL`,
**no docs/architecture change is needed** (ADR 0032 already satisfied).

### Comments to correct (both factually stale)

- `crates/api/src/contracts/queries_ch.rs:287-291` — pitfall **#1** falsely
  claims "`wasm_interface_metadata` is a plain `MergeTree`, so it must NOT
  carry `FINAL`". Prod + init.sql are RMT. Replace with: RMT-no-version →
  `FINAL` is legal AND needed (0327 backfill re-INSERTs a divergent `metadata`
  per `wasm_hash`, leaving 2 parts until merge). **KEEP pitfall #2** (the
  `sc.id AS id` aliasing note) unchanged.
- `crates/db-clickhouse/schema/init.sql:113-116` — "reads stay FINAL-free
  (lore-0293)" is now stale: 0327 broke the byte-identical-duplicate
  invariant. Update to note contract-detail reads use `FINAL` because a
  backfill can leave 2 divergent parts per `wasm_hash` transiently.

### DECISION 2026-07-07 — delete the dead PG contract-read path (do it here)

Prod `DataSource` is always `Ch`; the `DataSource::Pg` contract arms are dead
(`queries.rs:252` is the PG twin of the CH `fetch_wasm_interface`).

- PG module `crates/api/src/contracts/queries.rs` (sqlx/`PgPool`). Full surface
  to remove: `fetch_contract_list`, `fetch_contract`, `fetch_contract_stats`,
  `fetch_wasm_interface`, `fetch_invocation_appearances`,
  `fetch_event_appearances` + their `*Row` structs.
- Dispatch arms in `crates/api/src/contracts/handlers.rs`: `DataSource::Pg` at
  ~138, 515, 688, 704, 721, 739, 774 + the cursor-guard match arms ~617, 790,
  and the `use super::queries::{…}` import block ~30-34.
- **COMPILE NOTE:** `DataSource` is a 2-variant enum (`Pg|Ch`) SHARED across
  modules (ledgers, network, search, liquidity_pools still use `PgPool`).
  Removing only the `Pg` arms makes the contract matches non-exhaustive. So the
  **contracts module must go Ch-only**: drop the `DataSource::for_module`
  dispatch in contract handlers, call `queries_ch::*` directly, delete the PG
  functions. **The `DataSource` enum itself STAYS** (other modules branch on
  it). Scope the delete to the contracts module ONLY.
- Cursor variants `EventCursor::Pg` / `TxListCursor::Pg`: check whether only
  contracts constructs them before removing; if other modules use them, leave.
- Treat this as a SEPARATE commit from the FINAL fix (it is a refactor beyond
  the correctness fix).
- ⚠️ **COORDINATE WITH TASK 0244** (`refactor/0244_remove-postgres-sqlx-entirely`,
  active branch as of 2026-07-07): 0244 removes Postgres/sqlx wholesale, which
  SUBSUMES this contract-module PG delete. Before doing the PG deletion here,
  check 0244's status — if it is landing soon, either drop this sub-step
  (let 0244 own it) or rebase on 0244 to avoid a merge conflict in
  `contracts/handlers.rs` + `contracts/queries.rs`. The `FINAL` read fix and the
  backfill guard are independent of 0244 and can ship regardless.

### DECISION 2026-07-07 — include the backfill hash-guard (devil's-advocate #2)

`crates/backfill-runner/src/upgradeable_backfill.rs` — before running
`xdr_parser::contract::wasm_imports_upgrade_fn` on RPC-fetched bytecode (fetch

- parse described in the module doc ~lines 12-23), assert
  `sha256(code) == requested wasm_hash`. Mismatch → skip + `warn!` (mirror the
  0326 `malformed_metadata` posture), never write a flag. Grep
  `wasm_imports_upgrade_fn` and apply the same guard on any live parser path that
  derives `upgradeable` from fetched bytecode.

### Test (AC#3)

Integration test in `crates/api/src/tests_integration.rs` (CH-backed): INSERT
two divergent `metadata` rows for one `wasm_hash` into
`wasm_interface_metadata` (NO `OPTIMIZE`), read via `fetch_contract`, assert the
keyed/newer row wins (chip correct, not Unknown). Follow the existing CH
integration-test harness in that file. **Check first** how it provisions CH
(testcontainers vs external) — backend is Lambda-only, no local server.

### API-types codegen gate

`crates/api/**` is touched → CI `API types freshness` runs. The FINAL add +
PG deletion do NOT change DTOs/routes/openapi, so `nx run
@rumblefish/api-types:generate` should yield an EMPTY diff — but RUN it before
commit and stage any change (per repo CLAUDE.md), else the gate goes red.

### Suggested commit breakdown

1. `fix(lore-0332): wim contract-detail reads use FINAL + correct stale
MergeTree comments` (2 CH sites + `queries_ch.rs` comment + `init.sql`
   comment).
2. `refactor(lore-0332): drop dead PG contract-read path` (`queries.rs` +
   handler dispatch → Ch-only).
3. `fix(lore-0332): content-address guard in upgradeable-backfill`
   (`sha256(code) == wasm_hash`).
4. Test can fold into #1 or be its own commit.

### Remaining AC (unchecked above) map to

- FINAL added to the 2 CH reads → commit 1.
- Stale comment corrected → commit 1.
- Post-backfill correctness test (no OPTIMIZE) → commit 1/4.
- Latency sanity on the hot path → table is ~3.7k rows, `FINAL` cost is
  negligible; confirm with an EXPLAIN or a timed prod query if paranoid.
- `sha256` guard → commit 3.
