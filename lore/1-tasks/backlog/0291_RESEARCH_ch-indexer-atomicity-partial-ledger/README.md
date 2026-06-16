---
id: '0291'
title: 'Indexer atomicity audit: partial-ledger crash recovery + backfill re-run idempotency on ClickHouse'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: []
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
    note: 'Task created. Audit transactional integrity of CH indexer: crash mid-ledger, partial rows, backfill re-run idempotency.'
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

- [ ] Crash window documented: exact failure points in `commit()` that leave
      orphan rows, with file:line.
- [ ] Both resume paths verified to re-process a marker-less ledger.
- [ ] Each RMT table class verified idempotent under re-run, OR flagged as
      unsafe with reasoning.
- [ ] **Current-state tables confirmed absolute-state (not delta-accumulated)**,
      or flagged as a double-apply bug.
- [ ] Orphan-that-never-dies scenario evaluated (possible? blast radius?).
- [ ] "Dedup only on merge" read-side exposure assessed for count/sum queries.
- [ ] Repair options weighed (keep-as-is, pre-insert cleanup, experimental CH
      transactions, staging+MOVE PARTITION, read-side `FINAL`) with a clear
      recommendation + rationale.
- [ ] Follow-up backlog tasks spawned for any required fix.
- [ ] **Docs updated** — `N/A unless` audit changes the documented ingestion
      pipeline shape; if a cleanup/guard is added, update
      `docs/architecture/**` ingestion docs in the implementing PR.
- [ ] **API types regenerated** — N/A — research task, no `crates/api/**` change.

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
