---
id: '0260'
title: 'OPS: CH Snapshot B + rsync to M2 — pre-0241-deploy baseline'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0228', '0252', '0241', '0256']
tags: [priority-high, effort-medium, ops, hetzner, backup]
milestone: 1
links:
  - lore/1-tasks/active/0252_VALIDATION_clickhouse-endpoint-parity-against-stellar-apis.md
  - lore/1-tasks/active/0241_FEATURE_indexer-hard-swap-pg-to-ch-and-cutover-runbook.md
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

- Snapshot A path has a quoting artefact in its name
  (`pre_repair_20260521_1502]/`) — trailing `]` from the original
  BACKUP command; harmless, captured in [[hetzner-ch-artifacts]].
- Hetzner CH server profile cap is 6 GB by default
  (`max_memory_usage` in `users.d/timeouts.xml`). BACKUP queries
  may need a temporary bump; revert after.
- M2 owner is stkrolikiewicz; the local backfill copy was used
  during 0228 Phase 5 cross-machine cross-checks and has not been
  consulted since.
