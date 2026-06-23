---
id: '0321'
title: 'OPS: backfill native=0 tombstones for merged-account ghosts (DB-only, no S3 re-parse)'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0295']
tags: [ops, clickhouse, layer-data, accountmerge, priority-medium, effort-small]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned from 0295. The parser tombstone (bug-2) fixes new merges
      fix-forward; the ~522k already-merged-not-recreated accounts in prod CH
      need a one-shot backfill.
---

# OPS: backfill native=0 tombstones for merged ghosts

## Summary

0295's parser fix zeroes a merged account's native balance going forward, but the
~522k already-merged-not-recreated accounts in prod CH still carry a stale
positive native row (~12.4M phantom XLM, native aggregate inflated; verified vs
Horizon 404). Fix with a one-shot DB script — **no S3 / XDR re-parse needed**.

## Why DB-only (no re-parse)

The tombstone needs only `(account_id, merge_ledger)`, both already in
`operations_appearances` (type=8 AccountMerge → `source_id` + `ledger_sequence`).
INSERT a native `balance=0` row at the merge ledger for each merged account; under
`ReplacingMergeTree(last_updated_ledger)` the zero wins (and a recreated account's
higher-ledger row wins over the zero → safe).

## Plan

1. Derive the merged set from `operations_appearances WHERE type = 8` (max merge
   ledger per `source_id`). Optionally join `transactions` and filter
   `successful = 1` to drop failed-tx merges (operations_appearances has no success
   flag of its own).
2. INSERT `(account_id, asset_type = 0, balance = 0, last_updated_ledger =
last_merge)` for each.
3. Verify: deduped native sum drops by ~12.4M; sampled merged accounts read 0.

## Constraints

- Needs a **write-capable CH cert** — read-only `chq` / `dev_read` cannot INSERT
  (same blocker noted in 0315).
- Idempotent: re-inserting the same zero rows is harmless under RMT.

## Acceptance Criteria

- [ ] All merged-not-recreated accounts read native balance 0
- [ ] Native aggregate de-inflated (~12.4M XLM removed)
- [ ] No live / recreated account zeroed (RMT higher-ledger wins)
