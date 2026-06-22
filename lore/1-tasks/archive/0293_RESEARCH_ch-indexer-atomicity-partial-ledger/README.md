---
id: '0293'
title: 'Indexer atomicity audit: partial-ledger crash recovery + backfill re-run idempotency on ClickHouse'
type: RESEARCH
status: completed
related_adr: []
related_tasks: ['0298', '0310', '0232']
tags:
  [
    'phase-research',
    'area-indexer',
    'area-clickhouse',
    'priority-medium',
    'effort-medium',
  ]
links: []
history:
  - date: '2026-06-16'
    status: backlog
    who: karolkow
    note: 'Task created (renumbered 0291→0293, collision on develop). Audit transactional integrity of CH indexer: crash mid-ledger, partial rows, backfill re-run idempotency.'
  - date: '2026-06-17'
    status: active
    who: karolkow
    note: 'Promoted to active via /promote-task. Starting atomicity audit.'
  - date: '2026-06-17'
    status: active
    who: karolkow
    note: >
      Audit complete. Verdict: commit-marker + RMT design is sound, NO
      double-apply bug (current-state tables store absolute XDR post-image, not
      deltas). Residual gaps LOW: orphan-that-never-dies only via code-change
      mid-crash; transient read-side dup on non-FINAL transactions queries.
      Recommend keep-as-is + narrow hardening. Converted to directory, added
      notes/S-atomicity-audit-findings.md. Spawned 0298. Left active pending PR.
  - date: '2026-06-22'
    status: completed
    who: karolkow
    note: >
      Completed. The audit (LOW-severity verdict, keep-as-is + 0298) plus the
      emergent assets-aggregate clobber fix found during it. Fix evolved
      single-query CTE -> two-step -> AggregatingMergeTree -> final design B:
      pre-computed per-asset `asset_aggregates` table maintained by a refreshable
      MV (REFRESH EVERY 2 MINUTE), read via a trivial 1:1 LEFT JOIN. develop
      merged in (128 commits; 4 conflicts resolved; kept develop's asset_enrichment
      join). cargo --workspace --tests green; init_sql 24 statements; prod CH 26.3
      verified to support refreshable MVs. Spawned 0310 (drop dead columns +
      ledgers/wasm engine swap). Deploy runbook in this README.
---

# Indexer atomicity audit: partial-ledger crash recovery + backfill re-run idempotency on ClickHouse

## Summary

ClickHouse has **no real transactions**. The indexer does not write a ledger
atomically — it writes ~18 entity tables one after another (transactions,
operations, events, balances, …) and only at the very end writes a row to the
`ledgers` table as a "done" marker. If the indexer crashes mid-ledger, the
entity rows exist but the `ledgers` marker does not.

This task audits what actually happens in that crash window, and answers three
questions the team raised:

1. When a ledger is half-written (rows present, no `ledgers` marker), do we
   **delete** the orphan rows or **leave** them?
2. If we re-run backfill on that "missed" ledger, does the **already-present
   partial data cause problems**?
3. Does the re-run **replace** the partial rows with fresh ones, or duplicate
   them?

The pipeline is _designed_ to handle this (commit-marker pattern +
`ReplacingMergeTree` dedup), but that design has never been adversarially
verified. This task confirms the guarantee holds and surfaces the edge cases
where it does **not**.

## Context

Current behaviour found in the code (grounding for this audit):

- **Write order per ledger** — `crates/db-clickhouse/src/persist/writer.rs:130`
  (`write_ledger`): rows for 18 tables are buffered, then `commit()`
  (`writer.rs:279`) ends each table's INSERT in FK order, and **last** opens +
  closes the `ledgers` INSERT (`writer.rs:309`). So `ledgers.sequence` is the
  sole "fully indexed" marker — there is no separate status table.
- **No ACID transaction** — just sequential HTTP INSERTs + a commit marker.
  Failure between the first entity INSERT and the `ledgers` INSERT leaves orphan
  rows with no marker.
- **Resume logic relies on the marker:**
  - Backfill: `crates/backfill-runner/src/resume.rs:19` →
    `SELECT sequence FROM ledgers WHERE sequence BETWEEN $1 AND $2`; per-ledger
    skip at `crates/backfill-runner/src/ingest.rs:170`.
  - Live tail: `crates/indexer/src/handler/mod.rs:206` →
    `SELECT max(sequence) FROM ledgers`, re-does from `max+1`.
- **Dedup engine** — 17 of 19 tables are `ReplacingMergeTree`
  (`crates/db-clickhouse/schema/init.sql`); `ledgers` + `wasm_interface_metadata`
  are plain `MergeTree`. RMT dedups by ORDER BY key (some with a version column
  like `last_updated_ledger`) — **but only on background merge, not at insert
  time**.

So the _intended_ answer to the team's three questions is:

1. **Leave** orphan rows — don't delete.
2. Re-run is safe — backfill skips ledgers already in `ledgers`; the missed one
   is re-processed.
3. Re-insert produces the same deterministic surrogate keys → RMT collapses
   old + new rows to one on merge.

This task's job is to **prove that's actually true** and find where it breaks.

## Implementation Plan

### Step 1: Confirm the crash window + orphan semantics

- Trace `commit()` step-by-step. Confirm there is genuinely no marker until all
  entity tables ack, and identify the exact failure points that leave orphans.
- Confirm both resume paths (backfill + live tail) re-process a ledger whose
  marker is missing, regardless of orphan rows.

### Step 2: Verify re-insert idempotency per table class

Classify the 17 RMT tables and check each class survives a re-run:

- **Event-log tables keyed by `(ledger, tx, …)`** (transactions,
  operations_appearances, soroban_events, …): re-run produces identical keys?
  Confirm surrogate keys are deterministic (no time/random input) so old+new
  collapse. **No version column** on several — verify duplicates are byte-identical
  so RMT's arbitrary winner is harmless.
- **Current-state tables keyed by entity** (account_balances_current,
  accounts, lp_positions, nfts, …, version = `last_updated_ledger`): these are
  **collapsed state, not append-only**. Verify the row value for ledger N is
  computed as an **absolute** final state from XDR, NOT a delta accumulated onto
  the previous row. If it's delta-on-previous, a re-run double-applies → wrong
  balance. **This is the highest-risk case.**

### Step 3: Hunt the orphan-that-never-dies case

RMT only replaces a key the re-run **also produces**. Find any scenario where
attempt 1 writes a row for key K but attempt 2 does **not** (non-deterministic
parse, code change between attempts, partial-tx write): that orphan persists
forever and is never overwritten. Determine whether this is possible and what
the blast radius is.

### Step 4: Quantify the "dedup only on merge" exposure

Between a re-run insert and the next background merge, duplicate rows coexist.
Audit which read queries use `FINAL` / dedup-correct aggregation vs which can
return doubled rows in that window (e.g. counts, balance sums). Confirm the
`account_balances_current` canonical query (related to task 0198) is not exposed.

### Step 5: Evaluate repair options

If the audit finds a real gap (double-apply, orphan-that-never-dies, read-side
dup exposure), weigh the fix options — don't just pick the obvious one:

- **Keep current design** (leave-orphans + RMT) — accept the gap if blast radius
  is negligible. Cheapest. Document why it's acceptable.
- **Explicit pre-insert cleanup** — before re-inserting ledger N, delete its
  rows: `ALTER TABLE … DELETE WHERE ledger_sequence = N` or
  `DROP PARTITION` where the partition maps to one ledger range. Mutations are
  async + heavy on CH — assess cost at backfill scale.
- **Experimental ClickHouse transactions** — CH has experimental
  multi-statement transactions (`SET experimental_transactions = 1` /
  `BEGIN TRANSACTION … COMMIT`, currently limited to a single-node /
  non-replicated setup). Evaluate whether wrapping the per-ledger writes gives
  true atomicity, and whether the experimental status + replication constraints
  rule it out for prod. Note CH version + Hetzner deploy topology (tasks 0216, 0266) when judging viability.
- **Two-phase / staging table** — write ledger N to a staging area, then an
  atomic `MOVE PARTITION` / swap into the live table once complete. Heavier
  but engine-supported and non-experimental.
- **Read-side `FINAL` / dedup-correct queries** — if the only real exposure is
  transient duplicates before merge, fixing reads may be cheaper than fixing
  writes.

Decide and document the recommendation. Also consider:

- A guard in backfill to detect "rows exist but no marker" mismatch and act
  on it (re-do or flag).

Spawn backlog tasks for any fix that emerges; do not leave as prose.

## Acceptance Criteria

- [x] Crash window documented: exact failure points in `commit()` that leave
      orphan rows, with file:line. (`writer.rs:284-307` entity `end()`s,
      `:312/314/316` ledgers, any panic/SIGKILL; marker is last at `:309-317`.)
- [x] Both resume paths verified to re-process a marker-less ledger. (Backfill
      `resume.rs:19` + `ingest.rs:171`; live tail `handler/mod.rs:204` `max+1`.
      No orphan-detection guard exists.)
- [x] Each RMT table class verified idempotent under re-run, OR flagged as
      unsafe with reasoning. (Event-log A: byte-identical re-insert. State B:
      absolute. `assets`: no version col but recomputed absolutely.)
- [x] **Current-state tables confirmed absolute-state (not delta-accumulated)** —
      ABSOLUTE, no double-apply bug. Values are XDR post-images
      (`stage.rs:1113`, `state.rs:703/712`, nft owner from event `to`); no
      read-modify-write anywhere.
- [x] Orphan-that-never-dies scenario evaluated — possible **only** via
      cross-attempt nondeterminism (code/parser change mid-crash); blast radius =
      one crashed ledger. LOW.
- [x] "Dedup only on merge" read-side exposure assessed — only the non-`FINAL`
      `transactions` queries (Stmt A/B/C, `transactions/queries_ch.rs`) are
      transiently exposed; balances canonical query (0198) + all other reads use
      `FINAL`/`argMax` and are safe.
- [x] Repair options weighed (keep-as-is, pre-insert cleanup, experimental CH
      transactions, staging+MOVE PARTITION, read-side `FINAL`) with a clear
      recommendation + rationale. **Recommend keep-as-is + narrow hardening
      (0298).** See `notes/S-atomicity-audit-findings.md` Step 5.
- [x] Follow-up backlog tasks spawned for any required fix. (Task 0298 —
      atomicity hardening; created on develop.)
- [x] **Docs updated** — N/A. Audit found the documented ingestion shape is
      accurate; no cleanup/guard landed in this task (deferred to 0298), so no
      `docs/architecture/**` change required here.
- [x] **API types regenerated** — N/A — research task, no `crates/api/**` change.

## Notes

- Two non-RMT tables: `ledgers`, `wasm_interface_metadata` (plain `MergeTree`).
  `ledgers` re-insert on a re-run could in principle create a **duplicate
  marker row** (MergeTree never dedups) — check whether resume/`max(sequence)`
  logic cares, and whether `transaction_count` mismatches between attempts
  matter.
- Related: task 0198 (canonical balances), task 0217 (NFT pending quarantine
  tables) — both touch the same current-state RMT tables.
- Live-tail retry envelope: `crates/indexer/src/handler/mod.rs:113` (50/200/800ms
  backoff) — relevant to how often the crash window is actually hit in prod.

## Findings

Full audit with file:line evidence:
[notes/S-atomicity-audit-findings.md](notes/S-atomicity-audit-findings.md).

**Verdict:** the commit-marker + `ReplacingMergeTree` design is **sound**; **no
double-apply / data-corruption bug**. Team's three questions: orphan rows are
**left**; re-run is **safe** (backfill skips marker-present ledgers,
re-processes marker-less ones); re-run **collapses** via deterministic keys
(RMT), not durable duplication. Residual gaps are LOW severity (see Steps 3–4).

`ledgers` duplicate-marker concern from Notes: **not** produced by the normal
crash→resume path (resume only re-runs when the marker is absent); only from
overlapping backfill/live ranges or manual re-run. Resume membership /
`max(sequence)` tolerate it; sole cosmetic effect is network-TPS
`sum(transaction_count)` over-count in its 200-ledger window.

## Implementation Notes

- Pure read-only audit — **no production code changed**. Deliverable is the
  synthesis note + this README.
- Evidence gathered via 4 parallel code-reading agents (write path, schema
  classification, current-state computation, resume + read-side); the
  highest-risk claims (commit ordering, absolute-state balances, `assets`
  aggregate recompute) were re-verified by direct reads of
  `writer.rs:279-334`, `stage.rs:1100-1199`, `asset_aggregates.rs`.
- Schema confirmed: 19 tables, 17 RMT + 2 `MergeTree`. `assets` is the only RMT
  table with no version column (safe — identity re-insert is byte-identical and
  aggregates are recomputed absolutely).

## Design Decisions

### From Plan

1. **Followed the 5-step audit plan** (crash window → per-class idempotency →
   orphan-that-never-dies → read-side exposure → repair options).

### Emerged

2. **Recommend keep-as-is, not a write-side fix.** The absolute-state guarantee
   makes re-run idempotent; the only durable gap (Step 3 orphan) needs a deploy
   boundary to land inside a crash window. A heavyweight fix (experimental CH
   txns / per-ledger staging) is unjustified vs the blast radius.
2a. **`DROP PARTITION` ruled out for single-ledger cleanup** — event-log tables
   `PARTITION BY intDiv(ledger_sequence, 500000)`, so a partition spans 500 k
   ledgers; only `ALTER … DELETE WHERE ledger_sequence = N` could isolate one
   (async/heavy) — deferred to 0298.
3. **Bundled both hardening items into one follow-up (0298)** rather than two
   micro-tasks, per task-scope convention.
4. **Emergent: fixed the assets-aggregate clobber found during the audit.**
   `assets.total_supply`/`holder_count` were served NULL for ~25% of classic
   assets (no-version RMT, the per-ledger indexer's `None` row beat the batch).
   Iterated the fix — single-query CTE → two-step `fill_aggregates` →
   `AggregatingMergeTree` → **final design B: a pre-computed per-asset
   `asset_aggregates` table maintained by a refreshable MV** (`REFRESH EVERY 2
   MINUTE`), read via a trivial 1:1 LEFT JOIN. Picked B over the incremental AMT
   (A) because the dominant constraint is the `api_reader` read quota (0290/0198):
   B's read is O(1) and off-quota, 115× smaller storage; the only cost is ≤2-min
   staleness (fine for a supply/holder display). `MergeTree` not RMT (refreshable
   MV replaces, never appends); `asset_type IN (1,2)` (native sum unreliable;
   `chq` showed ~104.8B > real ~50B XLM supply). Drop of the dead columns + engine
   swaps deferred to **0310**.
5. **Merged develop (128 commits behind) before completing**, per user
   direction — kept develop's `asset_enrichment` join, re-applied the aggregate
   fix on top. Archived now (not deferred to PR merge) on user instruction.

## Future Work

- **Task 0298** — CH atomicity hardening: (1) backfill resume orphan guard for
  the code-change-mid-crash case; (2) read-side decision for the exposed
  non-`FINAL` `transactions` queries (accept per ADR 0044, or add `LIMIT 1 BY
  id`). Priority low — no correctness fix required today.
- **Task 0310** — prod cleanup (destructive, deferred from this task): drop the
  dead `assets.total_supply` / `holder_count` columns; rebuild `ledgers` /
  `wasm_interface_metadata` as `ReplacingMergeTree`. Neither is applied by
  `CREATE TABLE IF NOT EXISTS`; gated on the rollout below being verified in prod.

## Deploy / Migration Runbook (assets-aggregate AMT fix)

> The **additive** rollout of this task's assets-aggregate fix. Safe and
> reversible — nothing here is destructive. The destructive cleanup (drop dead
> columns, engine swaps) is **0310**, gated on this being verified in prod.

**Why a runbook at all:** prod already has every table, so deploying the new
`init.sql` is a no-op for them (`CREATE TABLE IF NOT EXISTS`). Only the two **new**
objects — `asset_aggregates` (per-asset table) + `asset_aggregates_mv` (a
**refreshable** MV `TO` it) — get created. The table is empty until the MV's first
refresh runs.

0. **Prerequisite — CH supports refreshable MVs. ✅ verified.** Prod CH is
   `26.3.10.60` (checked 2026-06 via `chq`); `REFRESH EVERY` is GA since 24.10 and
   `allow_experimental_refreshable_materialized_view` is on by default (value=1).
   `system.view_refreshes` is present. No flag / version action needed.
1. **Apply additive schema** (`init.sql`) → creates `asset_aggregates` + the
   refreshable MV (rest no-op).
2. **Trigger the first refresh** — no manual backfill INSERT; the refresh computes
   the whole table from `account_balances_current`. **Required: the MV does NOT
   populate on create — it waits for the first interval, so the table is EMPTY
   until this runs. Skipping it = API serves NULL supply/holders for 100% of
   assets** (worse than the 25% bug being fixed):
   ```sql
   SYSTEM REFRESH VIEW asset_aggregates_mv;
   SYSTEM WAIT VIEW    asset_aggregates_mv;  -- block until it finishes
   ```
3. **Deploy the API** — reads aggregates via the trivial `asset_aggregates` 1:1
   LEFT JOIN in `ASSET_CH_SELECT` (`crates/api/src/assets/queries_ch.rs`). No flag
   flip yet.
4. **Read-rows smoke** on a mega-holder asset page (e.g. yUSDC / USDC) as
   `api_reader` (readonly): lower risk than an AMT — the read is a 1:1 join on a
   small per-asset table, no read-time `GROUP BY` over holders (that heavy work is
   in the refresh, an admin job off the quota). Capture `read_rows` from
   `system.query_log`.
5. **HARD GATE before flag flip — assert the table is populated.** Do NOT flip
   until this passes (≈315k classic assets in prod, 2026-06):
   ```sql
   SELECT count() FROM asset_aggregates;  -- MUST be > 300000, not 0
   ```
   Script this as a blocking check in the flip procedure, not a manual eyeball —
   an empty table is the single biggest exposure (100%-NULL regression).
6. **Flag flip** the assets module to `DataSource::Ch` (task 0243). Reversible —
   flip back to PG if the smoke or canary regresses; nothing destructive ran.

**Freshness:** figures lag by up to `REFRESH EVERY` (2 min as written) —
eventually consistent, not to-the-ledger. Tune the interval if the supply/holder
display needs to be tighter (pure schema change).

**Develop merge (done):** this branch forked before develop's `asset_enrichment`
join landed in `queries_ch.rs` (lore-0231, commit `b944c125`). develop has been
merged in; `ASSET_CH_SELECT` now keeps the `asset_enrichment` (`ae`) join for
icon/name **and** the `asset_aggregates` join for supply/holders, with the dead
`a.total_supply` / `a.holder_count` reads removed.
