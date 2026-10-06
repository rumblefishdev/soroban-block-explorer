---
id: '0625'
title: 'backfill-runner: fill a partition up to its last published file'
type: FEATURE
status: backlog
related_adr: ['0052']
related_tasks: ['0553', '0225']
tags: ['backfill', 'testnet', 'effort-small', 'priority-low']
links: []
history:
  - date: 2026-10-06
    status: backlog
    who: karolkow
    note: 'Spawned from 0553 launch: the top-up to the tip wrote nothing.'
---

# backfill-runner: fill a partition up to its last published file

## Summary

`backfill-runner run` skips any 64k partition whose files are not all on S3
yet ("S3 archive lag … skipping", task 0225) and exits 0. So the partition
holding the network tip can never be backfilled; the live indexer has to walk
it ledger by ledger (~0.55 s each). On the 0553 launch that left ~57k ledgers,
~10 h of catch-up with the stall alarm up. After every testnet reset it will
be the same.

## Stan teraz

- Done: nothing; the behaviour is documented in 0553's launch note.
- Next: decide the shape (below).
- In force: the lag check protects against writing a partition while SDF is
  still uploading it — keep that for every partition except an explicitly
  requested open one.

## Implementation Plan

### Step 1: Shape

An explicit opt-in (e.g. `--to-tip`) that, for the last partition of the
range only, processes the contiguous run of files present from `--start` and
stops at the first missing one, reporting where it stopped. Every other
partition keeps the lag check.

### Step 2: Runbook

`docs/runbooks/testnet-reset.md` step 6 uses it, so the indexer resumes a few
ledgers behind the tip.

## Acceptance Criteria

- [ ] A run ending past the tip writes every published ledger up to the first
      missing file, contiguously, and says where it stopped.
- [ ] Without the opt-in, behaviour is unchanged (existing lag tests green).
- [ ] **Docs updated** — `docs/backfills.md` and the reset runbook.
- [ ] **API types regenerated** — N/A, no API change.
