---
id: '0395'
title: 'PERF: re-evaluate accounts_recent — native projection vs refreshable-MV table (0353 projection-rejection was a flippable default, not a hard limit)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0385', '0353', '0397']
tags: [perf, clickhouse, read-path, tech-debt, effort-small, priority-low]
milestone: 3
links: []
history:
  - date: 2026-07-14
    status: backlog
    who: karolkow
    note: >
      Spawned from 0387 deep-dive. Upstream research (CH docs, not our repo)
      showed the 0353 "projection refused on RMT (Code 344)" premise is a
      flippable default (deduplicate_merge_projection_mode), not a hard limit.
      accounts_recent (0385) may be a workaround for a non-limitation.
---

# PERF: accounts_recent — projection vs refreshable-MV table

## Summary

`accounts_recent` (0385) is a separate plain-MergeTree table + refreshable MV,
`ORDER BY (last_seen_ledger, id)`, powering the acclist `last_seen`-ordered
browse. It exists because a projection on the base `accounts` RMT was believed
refused (0353, "Code 344"). 0387's research proved that premise wrong: since CH
24.8, projections on RMT are allowed via
`SETTINGS deduplicate_merge_projection_mode = 'rebuild'`; error 344 is
`SUPPORT_IS_DISABLED` (flippable), not `NOT_IMPLEMENTED`. So `accounts_recent`
may be replaceable by a native projection `ORDER BY (last_seen_ledger, id)` on
`accounts` — less machinery, less staleness, no separate refresh.

## Context

Emerged during 0387 (account surrogate→StrKey read cost). Shares the same
projection spike (`scratchpad/spike_0387_projection.sql`). Tech-debt cleanup,
not urgent — acclist works today.

## Implementation (spike first)

- Reuse the 0387 projection spike to confirm `ADD PROJECTION` works on an
  `accounts`-shaped RMT in prod's CH version, and that the projection is used.
- **Blocker check:** projections are bypassed under `FINAL`. Verify acclist's
  read (`accounts::fetch_list`) does NOT use `FINAL` — if it does, a projection
  won't help and `accounts_recent` stays. (accounts_recent's whole point was to
  avoid the `accounts FINAL` sort.)
- Weigh `rebuild`-on-merge cost vs the current 2-min MV recompute + EXCHANGE.
  **Measured 2026-07-22 (task 0397), the number this task was missing:** the
  refresh costs **107.2 bn read_rows / 7 days** (5040 runs × 21.3M, 5 781 ms and
  895 MiB each) to serve **2 163 reads totalling 0.16 bn** — a **670:1**
  cost-to-use ratio, paid every 2 minutes whether or not anyone reads. The case
  for the projection is therefore cost, not just "less machinery": a projection
  is maintained per-part at write time, i.e. O(changed rows). If the projection
  turns out not to work, **relaxing the interval is the fallback with most of
  the win** — 2 min → 30 min is −93% for a staleness nobody browsing "recently
  active accounts" would notice. Note the distinction 0397 drew: an _ordering_
  projection over an RMT is safe (dedup still resolves at read after the seek),
  an _aggregate_ projection is not (raw duplicate versions get summed).
- If projection wins: migrate acclist read off `accounts_recent`, drop the MV +
  table (`mv` to `.trash/`), update docs/architecture per ADR 0032.

## Acceptance Criteria

- [ ] Spike verdict: does a native projection replace `accounts_recent` cleanly
      (created + used + `rebuild` cost acceptable + acclist non-`FINAL`)?
- [ ] If yes: acclist repointed, MV/table removed, docs updated, before/after
      read_rows on acclist unchanged or better.
- [ ] If no: document why (likely `FINAL` dependency) so this isn't re-litigated.
