---
id: '0332'
title: 'FEATURE: make contract-detail reads of wasm_interface_metadata merge-correct (FINAL or version col)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0326', '0327', '0320']
tags:
  [
    clickhouse,
    api,
    contract-detail,
    replacingmergetree,
    correctness,
    priority-low,
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
- Read sites without `FINAL`: `queries_ch.rs:307`, `queries_ch.rs:549`,
  `queries.rs:252`.

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
