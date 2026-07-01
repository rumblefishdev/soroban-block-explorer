---
id: '0332'
title: 'FEATURE: make contract-detail reads of wasm_interface_metadata merge-correct (FINAL or version col)'
type: FEATURE
status: completed
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
  - date: '2026-07-01'
    status: completed
    who: karolkow
    note: >
      Added FINAL to the 2 CH wim reads (queries_ch.rs:307,549), fixed the stale
      "plain MergeTree" comment, aligned canonical doc 12. Engine verified RMT
      (prod chq + init.sql); FINAL query prod-validated. PG site N/A (retired).
      Emerged: descoped the sha256 content-address guard (implemented then
      reverted) — upgradeable-backfill is a spent one-shot and the live path reads
      trusted ledger bytes, so the guard was dead code (YAGNI). No automated CH
      regression test (no CH harness in tests_integration) — validated via prod.
      PR #300.
---

# FEATURE: contract-detail wim read must be merge-correct

## Summary

`wasm_interface_metadata` is `ReplacingMergeTree` with **no version column**. The
API contract-detail path reads it via `LEFT JOIN wasm_interface_metadata wim`
**without `FINAL`** (2 CH sites — PG retired, see below), relying on the old invariant "content is immutable
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
- Read sites without `FINAL`: `queries_ch.rs:307`, `queries_ch.rs:549` (ClickHouse).
- The third historical site, `queries.rs:252`, is the **Postgres** path
  (`sqlx::PgPool`, JSONB `?` operator). Postgres is **retired** — `FINAL` is a
  ClickHouse-only concept and the RMT merge race cannot occur in PG. So the PG
  site is **N/A**; do not touch it here. If it obstructs, delete the PG detail
  path outright rather than maintaining it.

## Implementation

1. Verify `wasm_interface_metadata` engine is `ReplacingMergeTree` in every deploy
   target (prod ✓ 2026-06-27; check local/CI init.sql ✓; any other env).
2. Add `FINAL` to the `wim` join at the 2 CH read sites (tiny table → cheap).
   PG site `queries.rs:252` is N/A (retired engine).
3. Fix the stale "plain `MergeTree`" comment to say RMT-no-version + why `FINAL`.
4. (Alternative, heavier — only if `FINAL` proves too costly anywhere) add a version
   column + `ReplacingMergeTree(version)` via migration. Default to option 2.

## Acceptance Criteria

- [x] Engine verified RMT in all CH envs (prod `chq` + `db-clickhouse/schema/init.sql`);
      `FINAL` added to the 2 CH wim reads (`queries_ch.rs:307`, `:549`). PG site N/A — retired.
- [x] Stale "plain MergeTree" comment corrected (`queries_ch.rs`); canonical doc
      `12_get_contracts_interface.sql` brought in line with doc 11 (`wim FINAL`).
- [~] Contract-detail chip correct immediately after re-insert, no `OPTIMIZE`.
  Validated against prod CH (`FINAL` accepted, correct `upgradeable` row); NO
  automated regression test — `tests_integration.rs` has no CH harness (PG-only),
  and FINAL-dedup is a ClickHouse guarantee. A CH test harness is its own task.
- [x] No regression on hot path — `FINAL` over a ~3.9k-row table is negligible.

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

- [~] **Content-address guard — descoped (2026-07-01).** `upgradeable-backfill` is a
  one-shot backfill of pre-0327 WASMs; it was already run in prod and won't
  recur (the live parser writes `upgradeable` going forward). The only
  tamperable surface (public-RPC fetch) lives in this spent script — the live
  path derives `upgradeable` from the **authenticated ledger stream**, which is
  already content-addressed by the protocol (no untrusted-fetch surface). So a
  `sha256(code) == wasm_hash` guard would be dead code on a script that never
  re-runs; YAGNI. Was implemented + tested, then reverted. If the backfill is
  ever re-purposed for a re-parse over public RPC, add the guard then.
- [x] tags bumped to `priority-medium` (already set in frontmatter).
