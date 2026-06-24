---
id: '0322'
title: 'CH atomicity hardening: orphan guard, insert-dedup token, ledgers/wasm RMT, read-side dedup'
type: FEATURE
status: backlog
related_adr: ['0044']
related_tasks: ['0293']
tags:
  [
    'phase-future',
    'area-indexer',
    'area-clickhouse',
    'priority-low',
    'effort-medium',
  ]
links: []
history:
  - date: '2026-06-17'
    status: backlog
    who: karolkow
    note: 'Spawned from 0293 future work. Two LOW-severity hardening items the atomicity audit surfaced; no correctness fix required today.'
  - date: '2026-06-23'
    status: backlog
    who: karolkow
    note: 'Renumbered 0311 → 0322 to resolve an id collision: the active, merged 0311 (FEATURE enrichment multi-provider RPC, PR #270) keeps the id; this not-yet-started backlog task takes the new id. No external references pointed here.'
---

# CH atomicity hardening: backfill orphan guard + read-side transactions dedup decision

## Summary

The task 0293 atomicity audit (+ a sourced deep dive on fundamental fixes)
confirmed the indexer's commit-marker + `ReplacingMergeTree` design is sound (no
double-apply bug) and that true multi-table transactional atomicity is not
available on ClickHouse — the marker+RMT approach is industry-standard. The deep
dive surfaced a ranked set of proportionate, non-experimental hardenings (A–G
below). None is a correctness fix required today; bundle them into an
indexer-robustness pass. (A), (C) are cheap wins; (B) meaningfully tightens
live-path consistency; (G) is an adjacent state-table check worth doing.

## Context

From `lore/1-tasks/active/0293_RESEARCH_ch-indexer-atomicity-partial-ledger/notes/S-atomicity-audit-findings.md`:

1. **Orphan-that-never-dies (Step 3):** RMT only collapses keys a re-run also
   emits. If the row-emission / surrogate-key logic changes (a deploy) **between**
   a crash and its backfill re-run, attempt-1 orphan rows for the crashed ledger
   are never overwritten. Blast radius = one ledger. Today there is **no
   orphan-detection guard** in the resume path (`backfill-runner/src/resume.rs`,
   `ingest.rs`).
2. **Read-side transient duplicates (Step 4):** the non-`FINAL` `transactions`
   list queries (`crates/api/src/transactions/queries_ch.rs` Stmt A/B/C) can
   return doubled rows in the window between a re-run insert and the next
   background merge. `FINAL` is deliberately dropped there per ADR 0044
   (multi-billion-row table). Statement B (contract filter) has no dedup at all.

## Implementation — ranked candidate improvements

Sourced from the 0293 deep dive (`notes/R-fundamental-fix-deep-dive.md`). True
multi-table transactional atomicity is NOT available on ClickHouse MergeTree
(experimental transactions rejected — durability-not-guaranteed + open commit
crashes + Keeper dep; no atomic 18-table swap exists). The marker+RMT design is
industry-standard (Goldsky/CryptoHouse). These are the proportionate hardenings:

- **(A) Orphan guard — DO.** In the backfill resume path, before re-processing a
  marker-less range, clear that range's rows
  (`ALTER TABLE … DELETE WHERE ledger_sequence BETWEEN lo AND hi`,
  `mutations_sync=1`) across the 10 partitioned fact tables. Mechanism already in
  repo (`bootstrap.rs:477`, `sink.rs:298`, `nft_reclassify.rs:54-60`). Fires only
  on the rare partial-resume path. Fully closes the Step-3 orphan. ~1-2 days.
- **(B) `insert_deduplication_token` on live-tail — STRONG CANDIDATE.** Token
  `ledger-N-<table>` → a re-run's already-written tables are skipped at insert
  time, so **no duplicate ever lands** (vs RMT which lets it land then merges).
  Eliminates the transient read-side dup window on the live path. Constraints:
  single-block-per-partition (true for 1 ledger), identical retry settings,
  `non_replicated_deduplication_window` sized > SQS retry lag; verify the crate
  actually transmits the token. NOT clean for the 64k backfill stream (many
  blocks) — use (A) there.
- **(C) `ledgers` + `wasm_interface_metadata` → ReplacingMergeTree — DO (cheap).**
  Both are plain `MergeTree` today → a duplicate marker row (overlapping ranges /
  manual re-run) is never self-healed and **doubles rows in `ledgers` JOINs**
  (`ledgers/queries_ch.rs:234`, `contracts/queries_ch.rs:180/302/487/627`). RMT
  keyed by their ORDER BY (no version) self-heals. `ledgers` ~11M rows → cheap
  rebuild; marker semantics unchanged (RMT dedups, never drops the row).
- **(D) Read-side decision for `transactions` Stmt A/B/C** — accept the transient
  dup per ADR 0044, or add `LIMIT 1 BY id`. Pick one and document.
- **(E) Ops-runbook lever (no code):** `OPTIMIZE TABLE <t> PARTITION <p> FINAL`
  to force a synchronous RMT collapse of one partition after a known crash.
- **(F) Deferred — version column on the 9 event-log tables + `assets`:**
  deterministic tie-break at the data-model level. Needs a multi-billion-row
  rebuild → do it at the next forced full re-backfill, not standalone.
- **(G) Adjacent — confirm `repair_tier1` coverage.** The unpartitioned STATE
  tables can "silently corrupt under cross-machine RMT collapse" in parallel
  backfill (`repair_tier1.rs`, `main.rs:148-157`). Verify the repair pass is
  wired into the K-parallel flow — possibly higher impact than the Step-3 orphan.
  If gaps found, spawn a separate task.

### Round-3 fundamental fixes (red/blue/backup; see `notes/S-redteam-blueteam-verdict.md`)

These were the owner's real targets ("fundamental fix, not guard-everywhere").
CH transactions were rejected (durability-not-guaranteed + WONTFIX
ClickHouse#104661 + Keeper + commit-crash bugs). The fundamental fixes that
actually work are NOT write-atomicity — they may each warrant a **separate task**
(infra / API), noted here so they aren't lost:

- **(H) Quiesce-then-backup — INFRA, likely own task. HIGH value.** The daily
  `BACKUP DATABASE default` (`infra-hetzner/ansible/roles/backup/templates/ch-backup.sh.j2:136`,
  03:30) runs WITHOUT pausing the live indexer → a daily archive can capture a
  torn ledger (entity rows present, marker absent). Fix: pause the SQS doorbell /
  set indexer Lambda reserved-concurrency to 0, run `BACKUP`, resume (~10 min,
  zero correctness cost via cursor-resume). One-line addition; makes every archive
  a true point-in-time snapshot. This is the root-cause fix for the backup worry.
- **(I) Restore drill — OPS. HIGH value.** Task 0260 never tested restore
  (`0260 README:312`). Verify the daily Borg archive actually restores into a
  scratch CH + that the indexer reconcile self-heals the tail. Do regardless.
- **(J) Centralize read dedup — API, likely own task.** The "FINAL in every
  endpoint" tax is RMT-current-state-structural, NOT a crash/atomicity problem;
  no write fix removes it. Centralize via dedup-correct VIEWS per current-state
  table (wrap FINAL/argMax once; endpoints read the view). Do NOT use a blanket
  `<final>1</final>` profile flag — it forces FINAL on the 3.6B-row `transactions`
  table and worsens the `read_rows` quota blowups (tasks 0290/0198).

## Acceptance Criteria

- [ ] (A) Resume path guards/clears marker-less orphan rows; cost assessed.
- [ ] (B) `insert_deduplication_token` on live-tail evaluated/implemented, or
      explicitly declined with reason.
- [ ] (C) `ledgers` + `wasm_interface_metadata` RMT conversion done or declined.
- [ ] (D) `transactions` Stmt A/B/C read-side dup decision made and documented.
- [ ] (E) `OPTIMIZE … PARTITION … FINAL` manual-repair step in the ops runbook.
- [ ] (G) `repair_tier1` parallel-backfill coverage confirmed (or gap task spawned).
- [ ] Docs updated if ingestion/read path shape changes; else `N/A`.
- [ ] API types regenerated if `crates/api/**` changes; else `N/A`.
