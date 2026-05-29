---
id: '0233'
title: 'DOCS: merge-parallel-backfills runbook — Phase 3 FREEZE + rsync + Phase 4 ATTACH PART on Hetzner CH'
type: DOCS
status: canceled
related_adr: ['0040', '0044', '0045']
related_tasks: ['0216', '0225', '0227', '0228']
tags:
  [
    priority-high,
    effort-medium,
    layer-docs,
    runbook,
    clickhouse,
    hetzner,
    merge,
    multi-machine,
  ]
milestone: 1
links:
  - docs/runbooks/backfill_soroban_2of5_fresh_machine.md
  - lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md
history:
  - date: '2026-05-27'
    status: canceled
    who: stkrolikiewicz
    reason: obsolete
    note: >
      Closed as obsolete. Task 0228 archived GREEN (Phase 6 validated) without
      this runbook — the parallel-backfill merge happened in prod using the
      operator script `scripts/merge-freeze-worker.sh` (delivered under this
      task's ID in commit ef530362) plus inline operator notes. Empirical
      timings from laptop1's FREEZE were not captured in runbook form and the
      moment for co-development has passed. No future parallel-backfill is
      planned (Hetzner CH on prod via ADR 0047 = single primary store).
      Dangling link in docs/runbooks/0228_phase6_validation.md:543 removed
      in the same change. If a re-sync scenario emerges later, re-spawn from
      this task as historical reference.
  - date: '2026-05-19'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from task 0228 acceptance criterion: "docs/runbooks/
      merge-parallel-backfills.md authored, extending the 2/5
      fresh-machine runbook + citing ADR 0045 + task 0216". Carved
      out as a dedicated task because (a) it has its own acceptance
      gates (operator-runnable end-to-end), (b) the runbook content
      is empirical — needs co-development with the first laptop 1
      FREEZE so real timings, disk-pressure observations, and edge
      cases land in the doc rather than being theorised, and (c) it
      blocks 0228 completion regardless of Phase 5 code being ready.

      Inline FREEZE instructions drafted in the planning chat
      (2026-05-19) are the seed; this task hardens them into a
      reviewable runbook with the full Phase 3 + Phase 4 sequence,
      worker-ordering, rollback, and observability sections.
---

# DOCS: `merge-parallel-backfills` runbook — Phase 3 FREEZE + rsync + Phase 4 ATTACH PART on Hetzner CH

## Summary

Author `docs/runbooks/merge-parallel-backfills.md`, the operator
runbook covering Phase 3 (FREEZE + rsync export) and Phase 4
(ATTACH PART import on Hetzner) of the parallel-backfill merge in
[task 0228](../active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md).
Extends the existing 2/5 fresh-machine runbook,
[`docs/runbooks/backfill_soroban_2of5_fresh_machine.md`](../../../docs/runbooks/backfill_soroban_2of5_fresh_machine.md),
which covers Phase 1 (local backfill) but stops short of the merge
sequence.

## Status: backlog

Can start any time. Best executed alongside laptop 1's first real
FREEZE so timings and observations are captured live rather than
theorised.

## Context

Per [ADR 0045](../../2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md),
the merge transport is `ALTER … FREEZE` (hard-link snapshot) +
`rsync` (incremental network copy) + `ALTER … ATTACH PART` (atomic
per-part import) — no S3 intermediary, no extra-disk-overhead on
workers. The mechanism is well-defined but operationally tricky
across three workers + one production Hetzner CH:

- **Worker ordering**: Phase 4 ATTACH happens in worker-order
  (m1 → m2 → m3) per ADR 0045, so the Tier-1 repair pass downstream
  has deterministic input state. Phase 3 FREEZE + rsync can be
  parallelised across workers.
- **Per-partition straddles**: laptop 1's range starts mid-partition
  100 and ends mid-partition 110; machine 2 covers partitions
  110–120; laptop 3 covers 120 onward. CH partitions 110 and 120
  are straddles — every worker writes a partial slice and they merge
  on Hetzner.
  ([Task 0228 README §Context](../active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md))
- **Hetzner readiness gate**: [task 0216](../active/0216_RESEARCH_hetzner-clickhouse-deploy/README.md)
  - [task 0227](../active/0227_FEATURE_infra-hetzner-ansible-playbook.md)
    must land before Phase 3 to Hetzner is possible — staging dirs,
    mTLS CA, dict_reader user, `config.d/memory.xml`, BX21 Storage Box,
    Borg repo. Worker-side FREEZE is local and can run independently.
- **Phase 5 code on Hetzner**: the post-merge repair pass
  (`backfill-runner repair-tier1 + asset-aggregates + nft-reclassify`)
  must be available on the Hetzner binary before Phase 5 runs but
  is not needed for Phase 3 or 4. Tracked in 0228 Stage 1.

## Implementation Plan

Single deliverable: `docs/runbooks/merge-parallel-backfills.md`. The
runbook is structured as 8 sections + rollback annex, each
operator-runnable on its own.

### Section 1 — Preconditions checklist

Before any Phase 3 step starts:

- [ ] Hetzner CH operational + reachable via mTLS from each worker
- [ ] Staging dirs provisioned on Hetzner:
      `/var/lib/clickhouse/detached_inbox/{m1,m2,m3}/`
- [ ] SSH key auth from each worker to Hetzner (`rsync` over SSH
      tested)
- [ ] Per-worker `verify-local` passes (continuity gap = 0,
      fact-parity verified)
- [ ] Schema/parser SHA logged in `backfill_runs` audit table per
      worker
- [ ] Disk free on Hetzner ≥ 1.5 TB (transient staging during rsync + post-attach data)
- [ ] No in-flight merges / mutations on any worker
      (`SELECT count() FROM system.merges` = 0)
- [ ] No leftover `/shadow/<name>/` from prior freeze attempts
      (UNFREEZE first if found)

### Section 2 — Phase 3.A: FREEZE on workers

Per worker. Independent of other workers.

- **Snapshot naming convention**:
  `phase3_{worker}_{YYYYMMDD}` (e.g. `phase3_m1_20260519`)
- **Partitioned fact tables** (10 tables × N partitions):
  per-partition `ALTER TABLE … FREEZE PARTITION 'N' WITH NAME …`
- **State tables** (9 tables, no partition):
  whole-table `ALTER TABLE … FREEZE WITH NAME …`
- **Verification**: file count + size in `/shadow/<name>/store/`
  matches `system.parts` aggregate for the worker's data
- **Audit artifact**: per-worker JSON dump of
  `system.parts` snapshot to
  `docs/runbooks/artifacts/{worker}_freeze_{name}.json`

Drafted bash skeleton (seed for the runbook section) lives in the
2026-05-19 planning chat — extract verbatim with light formatting.

### Section 3 — Phase 3.B: rsync workers → Hetzner

Per worker, after FREEZE complete + verified.

- **Command shape**:
  `rsync -av --partial --progress
/var/lib/clickhouse/shadow/<name>/
hetzner-ch:/var/lib/clickhouse/detached_inbox/<worker>/<name>/`
- **Resumability**: `--partial` survives network interrupts;
  re-running rsync only transfers missing parts.
- **mTLS / SSH**: documented per task 0216's auth model. Verify
  with a dry-run small-file transfer first.
- **Verification**: file count + total bytes on Hetzner staging
  matches worker `/shadow/` exactly. `md5sum -c` against
  generated checksum manifest for paranoia.
- **Disk budget**: rsync target dir grows to ≈ worker's local CH
  size. Monitor `df -h /var/lib/clickhouse` on Hetzner during
  transfer.

### Section 4 — Phase 4: ATTACH PART on Hetzner

Worker-order m1 → m2 → m3. Per worker:

- **Move parts from staging into table's `detached/`**:
  `mv /var/lib/clickhouse/detached_inbox/<worker>/<name>/<uuid>/<part>
/var/lib/clickhouse/data/default/<table>/detached/`
- **ATTACH PART**:
  `ALTER TABLE <table> ATTACH PART '<part>'`
- **Per-partition `OPTIMIZE FINAL PARTITION`** after each
  partition's parts attach (handles RMT collapse + straddle-cell
  merge). Pass `optimize_throw_if_noop = 0` and a generous
  `max_execution_time` (default 60s too tight for multi-GB
  partitions on first OPTIMIZE).
- **Dictionary refresh**: after `transaction_hash_index` parts
  attach, `SYSTEM RELOAD DICTIONARY transaction_hash_dict`.
- **Verification**: per-table `SELECT count() FROM <tbl>` matches
  `worker_count + previous_attached_count`.

### Section 5 — Verification + observability per phase boundary

- **After FREEZE** (worker-side): `/shadow/` part count matches
  `system.parts` aggregate; disk free unchanged (hard-links).
- **After rsync** (Hetzner-side): file count + checksum match
  worker snapshot.
- **After ATTACH PART** (Hetzner-side): no orphan parts in
  `detached/`, per-table row counts increase by the expected
  worker delta.
- **After per-partition OPTIMIZE FINAL** (Hetzner-side): part
  count per partition reduces (RMT collapse occurred); query plan
  for sample API endpoint scans expected number of granules.

### Section 6 — Rollback / UNFREEZE annex

Before rsync starts: `UNFREEZE` is the recovery. After ATTACH PART
starts on Hetzner: `DETACH PART` is the recovery (parts return to
`detached/`, can be re-attached or removed). After OPTIMIZE FINAL:
the part-merge is destructive; recovery requires re-rsync from
worker `/shadow/` (which is why workers keep `/shadow/` until
post-attach OPTIMIZE confirmed).

### Section 7 — Worker-ordering matrix

Table mapping `(worker, partition)` → `(rsync done, attach done,
optimize done)`. Lets the operator track progress across the
m1→m2→m3 ordering without losing state across sessions.

### Section 8 — Cross-references

- [ADR 0045](../../lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md) — transport mechanism rationale
- [Task 0216](../../lore/1-tasks/active/0216_RESEARCH_hetzner-clickhouse-deploy/README.md) — Hetzner readiness gate
- [Task 0227](../../lore/1-tasks/active/0227_FEATURE_infra-hetzner-ansible-playbook.md) — Ansible playbook + mTLS CA
- [Task 0225](../../lore/1-tasks/archive/) — pre-parse sync validation (prereq for m2/m3)
- [Task 0228 Phase 5](../../lore/1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md) — post-merge repair pass (downstream consumer)
- Parent: [`backfill_soroban_2of5_fresh_machine.md`](../../docs/runbooks/backfill_soroban_2of5_fresh_machine.md)

## Acceptance Criteria

- [ ] `docs/runbooks/merge-parallel-backfills.md` authored covering
      sections 1–8 above.
- [ ] Cross-referenced from
      [`docs/runbooks/backfill_soroban_2of5_fresh_machine.md`](../../docs/runbooks/backfill_soroban_2of5_fresh_machine.md)
      (`See also` link added).
- [ ] Cross-referenced from
      [`docs/architecture/infrastructure/infrastructure-overview.md`](../../docs/architecture/infrastructure/infrastructure-overview.md)
      §5.6 per the 0228 docs acceptance gate.
- [ ] Worker-ordering matrix template (section 7) provided as a
      fillable markdown table; operator copy-paste fills it during
      real run.
- [ ] Empirical timing notes captured during laptop 1's first real
      FREEZE — embedded in the runbook (per-table FREEZE duration,
      `/shadow/` build size, rsync throughput to Hetzner, ATTACH
      PART duration, per-partition OPTIMIZE FINAL duration).
- [ ] Rollback procedures (section 6) tested at least once on
      laptop 1 + a non-prod CH (any of the existing claude-test
      worktrees) — note in the runbook which testbed.
- [ ] **Docs updated** — this task IS the doc update. Self-consistent.
- [ ] **API types regenerated** — N/A: docs-only task, no
      `crates/api/**` / `Cargo.{toml,lock}` / `libs/api-types/**`
      changes.

## Notes

- The inline FREEZE steps drafted in the 2026-05-19 chat (worker
  Phase 3.A) are the most polished seed — port verbatim and expand
  with rsync + ATTACH + OPTIMIZE coverage.
- Co-development with laptop 1's first FREEZE is the recommended
  workflow: operator runs FREEZE following the draft, captures
  timings + edge cases, files updates back into the runbook in the
  same PR. This way the runbook ships with empirical grounding,
  not pure theory.
- This task blocks 0228 completion (per its `blocks` frontmatter
  field) — both the FREEZE/rsync/ATTACH operational evidence and
  the runbook artifact are 0228 acceptance criteria.
- If the operator chooses to start Phase 3 before this task lands,
  document the first run in this task's `notes/` directory as an
  R-note (research/observations) — convertible to runbook content
  on completion.
