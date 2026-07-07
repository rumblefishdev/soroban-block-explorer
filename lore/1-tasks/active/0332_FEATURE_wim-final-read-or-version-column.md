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
      Implemented option A (FINAL) + sha256 content-address guard. Deep dive +
      devil's-advocate confirmed FINAL is the correct fix and rejected the version
      column (project `version = observed ledger` convention is the wrong axis for
      wim, whose divergence axis is the parser). Corrected the task's "3 read
      sites" error (only 2 are CH; the 3rd is dead PG). Prod re-verified RMT/no
      version via chq; mechanism proven read-only. Not committed; not yet moved to
      archive (divergent-value CI test + latency sanity pending).
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
- Read sites without `FINAL`: originally listed as 3, but **only 2 are
  ClickHouse** — `queries_ch.rs` (contract header / chip) and `queries_ch.rs`
  (wasm interface). The third, `queries.rs`, is the **dead Postgres** path
  (`sqlx`, `encode()`, jsonb `?`, `$1`); `FINAL` is a ClickHouse-only keyword and
  does not exist in PG, so that site is left untouched (PG retired). Line numbers
  in the original list were stale (code shifted since the task was written); the
  grep-authoritative sites at implementation time were `queries_ch.rs:310` and
  `queries_ch.rs:559`. The old comment's claim of a third "events stats query
  that joins `wim` FINAL-free" was itself stale — no such third join exists.

## Implementation

1. Verify `wasm_interface_metadata` engine is `ReplacingMergeTree` in every deploy
   target (prod ✓ 2026-06-27; check local/CI init.sql ✓; any other env).
2. Add `FINAL` to the `wim` join at the 3 read sites (tiny table → cheap).
3. Fix the stale "plain `MergeTree`" comment to say RMT-no-version + why `FINAL`.
4. (Alternative, heavier — only if `FINAL` proves too costly anywhere) add a version
   column + `ReplacingMergeTree(version)` via migration. Default to option 2.

## Acceptance Criteria

- [x] Engine verified RMT in all envs; `FINAL` added to the wim reads. Prod
      re-verified 2026-07-07 via `chq`: `ReplacingMergeTree` no version arg, 4004
      rows. init.sql = RMT. `FINAL` added to the **2 CH sites** (not 3 — see
      Context; the 3rd is dead PG).
- [x] Stale "plain MergeTree" comment corrected (`queries_ch.rs` + `init.sql`).
- [x] Contract-detail chip is correct immediately after an `upgradeable-backfill`
      re-insert, with NO `OPTIMIZE` needed. Proven on a local CH (docker, CH 26.3):
      two divergent parts for one hash, no merge, `wim FINAL` alone (sc NOT final)
      resolves `upgradeable=true`; with no FINAL anywhere the join fans out to 2.
      **A dedicated e2e test was written, run green, then dropped** (moved to
      `.trash/wim_final_e2e.rs`) — see the analyzer finding below: it cannot run in
      CI (no CH service) and cannot demonstrate fixed-vs-broken on the prod
      analyzer, so it was not worth keeping. Mechanism also proven on prod
      read-only.
- [ ] No regression on the contract-detail hot path (latency sanity). `cargo
    check` green; updated query runs on prod. Latency benchmark not yet run.

## Design Decisions

### From Plan

1. **Option A (`FINAL`) over option 4 (version column).** Default per the task.
   Confirmed correct after a deep dive: `FINAL` is the idiomatic RMT read and is
   necessary regardless (a version column does NOT remove the need for `FINAL` —
   RMT still returns un-merged parts to a non-FINAL query). Tiny table (4004
   rows) → `FINAL` is cheap.

### Emerged

2. **Version column deliberately rejected, not just deferred — with a sharper
   reason than "YAGNI".** The project convention (init.sql:69-76) is `state table
→ RMT(version = observed ledger)`, read `argMax(_, version)` (cf.
   `soroban_contract_metadata`, `*_enrichment`). That convention's version axis is
   **ledger time**. `wim`'s divergence axis is the **parser version** (content is
   a pure function of immutable WASM bytes + parser logic), so a `version =
observed ledger` column would be schema-consistent but semantically wrong and
   would NOT fix the only case `FINAL` misses (a stale-parser write landing after
   a newer one). The only column that fixes that is `parser_version`, which itself
   breaks the project's version convention. Net: consistency and the fundamental
   fix pull in opposite directions, and the residual "stale-write-lands-last" case
   is triply-improbable (needs a future re-derivation + deploy skew + a rare
   re-emit of an already-backfilled hash) and is mitigated operationally (deploy
   indexer before any re-derivation backfill / one-shot `OPTIMIZE` à la 0326).
   Recorded in the `queries_ch.rs` + `init.sql` comments for the future.

3. **PG read site (`queries.rs`) excluded.** It is dead Postgres; `FINAL` is not a
   PG keyword. Not touched.

4. **Content-address guard scope + failure posture (devil's-advocate item 2).**
   `sha256(fetched code) == wasm_hash` asserted before the import scan in
   `upgradeable_backfill`. Mismatch → mark `seen`, count a new `hash_mismatch`
   stat, warn, skip (never write a flag) — mirrors the `malformed_metadata`
   posture. Went one step further than the acceptance line: `hash_mismatch > 0`
   now also drives a **non-zero process exit** (main.rs), same as
   `missing_on_rpc`/`malformed_metadata`, because corrupt/tampered RPC bytecode is
   an anomaly the operator must chase. Added `sha2` to `backfill-runner`'s deps.

5. **No unit test for the guard.** It is an `sha256` equality (stdlib), not a
   branch/parser with edge cases; a test would exercise the `sha2` crate, not our
   logic. YAGNI.

6. **The task's core premise is largely refuted for the current prod config —
   `wim FINAL` reframed from "bug fix" to "hardening" (option C).** Discovered by
   running the divergence case on a local CH (docker): CH 26.x's **new analyzer**
   (`enable_analyzer=1`, the prod default — confirmed via `chq`) **propagates
   `FINAL` from the main table to joined `ReplacingMergeTree` tables**. The
   existing query already carries `soroban_contracts sc FINAL` (for the 0320 stale
   `wasm_hash` reason), so `sc FINAL LEFT JOIN wim` ALREADY reads `wim` as FINAL —
   the fan-out / keyless-pick the task describes **cannot occur on prod as
   configured**. Matrix (local, both analyzers; prod re-confirmed):

   - new analyzer: `sc FINAL, wim plain` → 1 (masked); `sc FINAL, wim FINAL` → 1
   - old analyzer: `sc FINAL, wim plain` → 2 (bug); `sc FINAL, wim FINAL` → 2
     (FINAL on a join's right side is IGNORED by the old analyzer)

   Consequence: the explicit `wim FINAL` is functionally inert on both analyzers
   today (redundant on new via propagation, ignored on old). Its ONE real gain is
   **decoupling** — `wim plain` vs `wim FINAL` differ only when `sc` loses its own
   FINAL (new analyzer, `sc plain + wim FINAL` → 1 vs `sc plain + wim plain` → 2).
   So it guards a future refactor that drops `sc FINAL`, and documents intent.
   Kept as cheap explicit hardening; NOT shipped as a live-bug fix. The `sha256`
   guard and the corrected comments (queries_ch.rs + init.sql, which now document
   the analyzer propagation) carry the real value of this task.

7. **e2e test dropped (option C).** Written + run green on docker, then moved to
   `.trash/`. It does not run in CI (`ci.yml` runs `cargo test` with no CH
   service → the `CLICKHOUSE_URL` gate skips it) and, on the prod analyzer, cannot
   distinguish fixed-vs-broken (the bug is masked). Low regression value for the
   maintenance cost. The mechanism is captured in the code comments instead.

## Issues Encountered

- The task's original "3 read sites" + line numbers were stale/wrong; only 2 are
  ClickHouse. Corrected in Context.
- `queries_ch.rs` comment (from `d258c93b`) claimed `wim` was plain `MergeTree`
  (→ `ILLEGAL_FINAL`). Prod + init.sql are both `ReplacingMergeTree`, so `FINAL`
  is legal — the comment was factually wrong. Rewritten.

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

- [x] `sha256(fetched code) == wasm_hash` asserted before the import scan; mismatch →
      skip + warn (mirrors the 0326 `malformed_metadata` posture), never write a flag.
      Also drives a non-zero process exit (new `hash_mismatch` stat). The "live path
      that derives `upgradeable` from fetched bytecode" is `db-clickhouse` persist
      `stage.rs`, but it derives from the SAME ledger-entry bytecode the indexer
      already validated (not a separate public-RPC fetch), so no extra guard is
      needed there — the guard targets the RPC-fetch path (`upgradeable_backfill`).
- [x] tags bumped to `priority-medium` (already present in frontmatter `tags`).
