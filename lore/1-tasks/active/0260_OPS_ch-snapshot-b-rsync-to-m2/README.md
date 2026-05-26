---
id: '0260'
title: 'OPS: CH Snapshot B + rsync to M2 — pre-0241-deploy baseline'
type: OPS
status: active
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
      the task body = fishuser-HERO (Linux worker that ran
      0228 backfill leg). fishuser-HERO has 393 GiB free → 760
      GiB after deleting the orphaned backfill Docker volume
      (`soroban-block-explorer_clickhouse-data`, 367 GiB,
      idle since 2026-05-21). Headroom on the receiver
      post-rsync = 9 % — below the 15 % comfort threshold
      but acceptable. Transport = direct SSH `sorban-prod`
      (no wireguard). BACKUP command shape recovered from
      box bash history: `BACKUP DATABASE default TO
      Disk('backups', '<name>')`. Original plan's Phase 1
      assumption ("free disk on M2") was directionally right —
      delete is needed — but the target is a live Docker volume,
      not a static copy.
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

- [ ] Snapshot A removed from Hetzner; disk free ≥ Snapshot B
      expected size + headroom.
- [ ] Local M2 backfill removed; disk free ≥ snapshot transfer
      size + headroom.
- [ ] Snapshot B captured on Hetzner under
      `/srv/backups/snapshot_b_post_0252_<YYYYMMDD>/`; row counts
      per table match what `0252` validation reported.
- [ ] Snapshot B rsynced to M2; md5sum end-to-end matches.
- [ ] Snapshot path + size + transfer wall captured in
      [[hetzner-ch-artifacts]] memory.
- [ ] Task 0241 deploy unblocked (cross-link in 0241 history).

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
- "M2" in this task body = the **fishuser-HERO** Linux worker
  host (which ran one leg of the 0228 parallel backfill).
  Confirmed 2026-05-26 — see Phase 0 history entry. The
  backfill copy is the live Docker volume
  `soroban-block-explorer_clickhouse-data` on fishuser-HERO
  (367 GiB, idle since 2026-05-21).
