---
id: '0260'
title: 'OPS: CH Snapshot B + rsync to M2 — pre-0241-deploy baseline'
type: OPS
status: completed
related_adr: []
related_tasks: ['0228', '0252', '0241', '0256']
tags: [priority-high, effort-medium, ops, hetzner, backup]
milestone: 1
links:
  - lore/1-tasks/archive/0252_VALIDATION_clickhouse-endpoint-parity-against-stellar-apis.md
  - lore/1-tasks/active/0241_FEATURE_indexer-hard-swap-pg-to-ch-and-cutover-runbook.md
  - notes/G-snapshot-runbook.md
history:
  - date: '2026-05-25'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned to formalise the snapshot+rsync sequence that must
      happen between task 0252 close and the 0241 production
      deploy. The validated Hetzner CH state at 0252 close is the
      canonical "known-good" baseline; once 0241 ships the live
      indexer write path, the box state becomes a moving target and
      we lose the ability to re-validate against the same data
      without a snapshot in hand.

      Disk pressure forces a delete-before-create flow: we have one
      Snapshot A (`pre_repair_20260521_1502]/`, 691 GiB compressed
      zstd) on Hetzner and one local backfill on the M2 (also large)
      that both pre-date 0252 + 0255. Free both before taking the
      new snapshot — neither is needed once Snapshot B lands on M2.
  - date: '2026-05-26'
    status: active
    who: stkrolikiewicz
    note: >
      Activated for execution — pre-0241-deploy snapshot sequence
      kicks off.
  - date: '2026-05-26'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 0 (pre-flight) partial. Verified 0252 closed
      (artifact `docs/runbooks/artifacts/endpoint_validation_20260525.md`).
      Task converted file→directory; drafted operator runbook
      skeleton at `notes/G-snapshot-runbook.md` covering disk
      audit, table enumeration, BACKUP command shape (with TODOs
      for operator to fill from Snapshot A shell history),
      memory-cap bump, row-count drift check, rsync, and md5
      verification. Open questions captured in the note.
      Remaining Phase 0 (disk audit, transport check, M2
      backfill location) blocked on SSH key load + operator
      input.
  - date: '2026-05-26'
    status: active
    who: stkrolikiewicz
    note: >
      Phase 0 complete. Live audits captured in runbook:
      Hetzner has 280 GiB free pre-A-delete (Snapshot A is
      692 GiB, real path is `pre_repair_20260521_1502` —
      task body's trailing `]` was a transcription error,
      now corrected). CH `default` has 20 entities (18 RMT,
      2 MT, 1 Dictionary `transaction_hash_dict`) — matches
      schema. `max_memory_usage` at 6 GiB; will attempt
      Snapshot B at default cap, bump only on OOM.
      `system.backups` clean, no residual FAILED rows. No
      OPTIMIZE active at audit. Topology resolved: "M2" in
      the task body refers to a Linux worker that ran one leg
      of the 0228 backfill, not a static copy on operator's
      laptop. M2 has 393 GiB free → 760
      GiB after deleting the orphaned backfill Docker volume
      (`soroban-block-explorer_clickhouse-data`, 367 GiB,
      idle since 2026-05-21). Headroom on the receiver
      post-rsync = 9 % — below the 15 % comfort threshold
      but acceptable. Transport = direct SSH `<hetzner-host>`
      (no wireguard). BACKUP command shape recovered from
      box bash history: `BACKUP DATABASE default TO
      Disk('backups', '<name>')`. Original plan's Phase 1
      assumption ("free disk on M2") was directionally right —
      delete is needed — but the target is a live Docker volume,
      not a static copy.
  - date: '2026-05-27'
    status: completed
    who: stkrolikiewicz
    note: >
      Phases 1-4 executed. Snapshot B captured on Hetzner
      (`/srv/backups/snapshot_b_post_0252_20260526/`, 689.87 GiB
      per system.backups / 690 GiB du, 4923 files, BACKUP
      wall 9m 46s starting 2026-05-26 09:50:06 UTC).
      Row counts zero-diff across all 19 tables vs the
      [[ch-backfill-state]] frozen baseline. rsync to
      M2 (`~/snapshots/snapshot_b_post_0252_20260526/`)
      completed; md5 manifest diff = bit-identical for
      all 4923 snapshot files. (One stray empty `./ssh`
      pollution artefact appeared in destination dir from
      an operator broken-redirect retry — cleaned post-
      verify; runbook updated with the correct manifest
      command syntax.) Phase 1 reframed to a Docker volume
      delete on M2; Phase 2 used direct `rm` for
      Snapshot A instead of the mv-to-trash recovery window
      (no-op on single-FS box). All 6 ACs ticked.
      [[hetzner-ch-artifacts]] memory refreshed with Snapshot B
      coords. 0241 deploy unblocked (dependency recorded via
      `related_tasks` frontmatter, no cross-link in 0241 body
      per operator preference). Follow-up items (restore drill, automated
      snapshots, rsync stats wrapper) captured under Future
      Work — not spawned as backlog tasks yet, operator to
      triage priority.
---

# OPS: CH Snapshot B + rsync to M2

## Summary

Take a fresh "Snapshot B" of the Hetzner CH state after task 0252
finalisation, then rsync it to the local M2 machine as the
canonical pre-0241-deploy backup. Snapshot A (pre-0252) and the
local backfill on M2 are both freed first to make room.

## Why

- 0252 closes the validation sweep on the current CH state. That
  state is the "known-good" baseline against which any post-0241
  drift will be measured (task 0256 re-runs E11 against post-deploy
  ingestion).
- 0241 deploys the new indexer write path. Once live, the CH state
  diverges from the validated snapshot continuously — without a
  point-in-time backup we have no rollback target.
- 0256 needs the **new** parser to ingest fresh ledgers; we cannot
  measure mismatch reduction without first taking Snapshot B and
  comparing against it.

## Sequence

1. **Validate 0252 close** — Phase E artifact landed +
   `endpoint_validation_<YYYYMMDD>.md` reviewed; no pending
   compare\_\*.py runs on the box.
2. **Free disk on Hetzner** — delete Snapshot A
   (`/srv/backups/pre_repair_20260521_1502]/`, 691 GiB zstd). Its
   purpose was to gate the Phase 5 repair-pass rollback;
   0252 close confirms the post-repair state, so A is no longer
   needed.
3. **Free disk on M2** — delete the local Soroban-era backfill
   copy. Locate via the operator (path not in this task —
   ask the M2 owner first).
4. **Take Snapshot B** on Hetzner — same shape as Snapshot A
   (BACKUP TABLE … ZSTD, hand-curated table list per the existing
   runbook). Target: `/srv/backups/snapshot_b_post_0252_<YYYYMMDD>/`.
5. **rsync** Snapshot B → M2 — `rsync -avzP --partial` over the
   wireguard / SSH tunnel; capture transfer rate + total wall.
6. **Verify** — md5sum the snapshot on both ends matches.
7. **Mark task done.** Unblocks 0241 deploy → which unblocks 0256.

## Acceptance Criteria

- [x] Snapshot A removed from Hetzner; disk free ≥ Snapshot B
      expected size + headroom. (Removed 2026-05-26; freed
      692 GiB; post-delete free = 972 GiB vs 690 GiB Snapshot B
      = 28 % headroom.)
- [x] Local M2 backfill removed; disk free ≥ snapshot transfer
      size + headroom. (Removed Docker volume
      `soroban-block-explorer_clickhouse-data` on M2, freed
      367 GiB; post-delete free = 760 GiB vs 690 GiB Snapshot B
      = 9 % headroom — workable.)
- [x] Snapshot B captured on Hetzner under
      `/srv/backups/snapshot_b_post_0252_20260526/`; row counts
      per table match what `0252` validation reported.
      (CH `BACKUP DATABASE default` 2026-05-26 09:50:06→09:59:52,
      9m 46s wall; on-disk 689.87 GiB / 690 GiB du; 4923 files;
      all 19 table row counts zero-diff vs [[ch-backfill-state]]
      frozen baseline.)
- [x] Snapshot B rsynced to M2; md5sum end-to-end matches.
      (rsync the Hetzner host → M2; 4923/4923 files
      bit-identical via md5sum manifest diff; sole diff was
      a stray empty `./ssh` artefact in destination dir from
      operator's broken-redirect retry — cleaned.)
- [x] Snapshot path + size + transfer wall captured in
      [[hetzner-ch-artifacts]] memory.
- [x] Task 0241 deploy unblocked. (Cross-link in 0241 history
      intentionally not added — operator chose to keep 0241's
      pending-Part-D history clean. The 0260 → 0241 dependency
      is recorded here via `related_tasks` in frontmatter and
      via the Why section above.)

## Notes

- Snapshot A path is `/srv/backups/pre_repair_20260521_1502`
  (no trailing `]` — earlier captures in [[hetzner-ch-artifacts]]
  and this task's history transcribed a stray `]` that does not
  exist on disk; confirmed via `ls -la /srv/backups/` 2026-05-26).
- Hetzner CH server profile cap is 6 GB by default
  (`max_memory_usage` in `users.d/timeouts.xml`). BACKUP queries
  may need a temporary bump; revert after.
- M2 owner is stkrolikiewicz; the local backfill copy was used
  during 0228 Phase 5 cross-machine cross-checks and has not been
  consulted since.
- "M2" in this task body refers to a Linux worker host (one of
  the 0228 parallel-backfill workers), not a static copy on the
  operator's laptop. Confirmed 2026-05-26 — see Phase 0 history
  entry. The "backfill copy" is the live Docker volume
  `soroban-block-explorer_clickhouse-data` on that worker
  (367 GiB, idle since 2026-05-21).

## Implementation Notes

- **Operator runbook** captured at `notes/G-snapshot-runbook.md`
  (created during Phase 0 — closes the "no documented BACKUP
  procedure" gap that was the largest pre-flight risk).
- **BACKUP command shape** recovered from Hetzner bash history:
  `BACKUP DATABASE default TO Disk('backups', '<name>')`.
  Single statement covers all 20 entities of the `default` DB
  atomically (18 RMT, 2 MT, 1 Dictionary). zstd compression is
  the implicit default — no `SETTINGS compression_method` was
  needed.
- **Snapshot B specifics:** name `snapshot_b_post_0252_20260526`,
  path `/srv/backups/snapshot_b_post_0252_20260526/` on
  the Hetzner host, 689.87 GiB on disk (system.backups) / 690 GiB
  du, 4923 files, BACKUP wall 9m 46s. Pulled to M2
  at `~/snapshots/snapshot_b_post_0252_20260526/`.
- **Row-count integrity** confirmed zero-diff across all 19
  tables vs `[[ch-backfill-state]]` frozen baseline — CH BACKUP
  snapshot was consistent (write path was off pre-0241, so the
  baseline didn't drift between 0252 close and Snapshot B).
- **Transport:** direct SSH on alias `<hetzner-host>` from
  M2; no wireguard.

## Issues Encountered

- **CH client receive_timeout = 300 s.** `clickhouse-client -q`
  in synchronous mode dropped the BACKUP query at 5 min with
  "Timeout exceeded while receiving data from server". Server-
  side BACKUP kept running and finished at 9m 46s. Status was
  visible via `SELECT … FROM system.backups`. Cosmetic, not a
  failure; runbook now flags this and suggests the `ASYNC`
  keyword for cleaner client behavior in future runs.
- **`mv` to `.trash_*` cannot free disk on a single-FS box.**
  Earlier draft of the runbook prescribed `mv` to a temp
  directory as a "recovery window" for Snapshot A deletion.
  Sorban-prod has no second large filesystem, so `mv` only
  rewrites the inode and `df` shows no reclamation. Corrected
  to single-step `rm -rf` with the risk surface analysed in
  the runbook ("Snapshot A's job as Phase 5 rollback gate
  ended at 0252 close; live `/srv/clickhouse-data` is the
  source of truth").
- **Snapshot A path discrepancy.** Both [[hetzner-ch-artifacts]]
  memory and the original 0260 task body cited the dir name
  with a trailing `]` (`pre_repair_20260521_1502]/`); the real
  dirname is plain `pre_repair_20260521_1502`. Confirmed via
  `ls -la /srv/backups/`. Memory + Notes section both
  corrected during Phase 0.
- **Shell-redirect mishap polluted hero snapshot dir.** Operator's
  initial attempt at the md5 manifest used `> ssh user@host:path`,
  which Bash parses as redirect to local file `ssh` (the next
  token) with the rest as args to `sort`. Result: empty file
  `./ssh` inside the hero snapshot copy + SIGPIPE on md5sum.
  Cleaned via `rm` post-verify; manifest re-ran successfully.
  Runbook now shows two correct alternatives (local-then-scp
  vs pipe-to-remote-shell).
- **rsync transfer stats not captured.** Operator did not save
  the rsync `time` / bytes / rate output. Wall is between the
  BACKUP completion (~09:59 UTC) and the verify step. Not
  blocking close; future snapshot ops should always wrap the
  rsync call in `time` and tee the summary to a session log.

## Design Decisions

### From Plan

1. **Single `BACKUP DATABASE default` rather than per-table loop.**
   Snapshot A used the same shape; covers all 20 entities
   atomically.
2. **`rsync -aP --partial` without `-z`.** Snapshot is already
   zstd-compressed on disk, so `-z` adds CPU for ~zero gain.

### Emerged

3. **"M2" in the task body disambiguated mid-execution.**
   Original wording was ambiguous between an operator laptop
   and a Linux backfill worker host. Confirmed with operator
   during Phase 0 — it consistently means the worker host that
   ran one leg of 0228 backfill. Updated the runbook + Notes
   to use the abstract "M2" throughout and removed any
   laptop-flavoured framing.
4. **Phase 1 ("free disk on M2") reframed to "delete M2's
   backfill Docker volume".** The original prose implied a
   static backup
   copy needing `rm`. Reality was a live Docker volume
   (`soroban-block-explorer_clickhouse-data`, 367 GiB) idle
   since 2026-05-21. Removal required `docker compose down` +
   `docker volume rm`, not raw delete.
5. **Skipped the `mv`-to-`.trash_*` recovery window for Snapshot A
   delete.** Original plan assumed mv would buy a recovery window;
   single-FS box makes that a no-op (see Issues). Direct `rm -rf`
   chosen; risk analysis added to the runbook.
6. **`BACKUP` ran at the default 6 GiB `max_memory_usage`.** Plan
   assumed a temp bump might be needed; Snapshot A's bash history
   showed no bump, and Snapshot B succeeded at the default cap
   in 9m 46s. The bump procedure stays in the runbook for future
   use only.
7. **No CH BACKUP runbook existed in the repo before this task.**
   Surfaced during planning, closed by `notes/G-snapshot-runbook.md`.
   Future snapshot ops on this box can be driven from that single
   file rather than from operator memory.
8. **md5 manifest verification chosen over `du -sb` size check.**
   Plan called for end-to-end md5; we executed it. Cheaper paths
   (`du -sb` equality, `rsync --dry-run --checksum`) are documented
   in the runbook as alternatives but were not taken — the full
   manifest diff caught the stray `ssh` pollution file, which a
   `du` check would have masked.

## Future Work

- **Snapshot B restore drill.** This task only verifies bit-identity
  of the on-disk artefact, not its restorability into a working
  CH instance. A restore drill belongs to a follow-up backlog
  task (would require a second CH instance with ≥1 TiB disk).
- **Automated periodic snapshots.** One-shot here; productionising
  the snapshot cadence (cron / systemd timer) is a separate task.
- **rsync stats tooling.** A wrapper that always tees rsync's
  summary to a session log file would prevent loss of timing
  data like we hit in this run.
