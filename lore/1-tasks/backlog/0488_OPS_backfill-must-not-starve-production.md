---
id: '0488'
title: 'OPS/REFACTOR: a backfill must not be able to starve production — scratch isolation, self-throttling, derivable resume'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0279', '0359', '0367', '0266']
tags: [backfill, ops, clickhouse, reliability, priority-high, effort-medium]
links: []
history:
  - date: '2026-08-14'
    status: backlog
    who: stkrolikiewicz
    note: 'Spawned from the 0279 backfill incident: scratch filled /, ClickHouse lost write space, live ingestion fell 10h behind.'
---

# OPS/REFACTOR: a backfill must not be able to starve production

## Summary

The 0279 backfill filled the production box's only filesystem, ClickHouse
started refusing writes (`Cannot reserve 1.00 MiB, NOT_ENOUGH_SPACE`), the
prices ingest failed and the live indexer fell **10 hours** behind. Nothing
in the system prevented a batch job from taking the database's disk, and
nothing alerted a human. This task makes that structurally impossible rather
than a matter of operator vigilance.

## Context — what actually happened (2026-08-13/14)

Eight `backfill-runner --lp-amounts-only` workers ran against prod. Each holds
downloaded ledger files for the partition it is parsing plus the one it is
prefetching — ~13.6 GB per partition in recent eras. Scratch lived in
`~/.bf` on `/`, the same ext4 filesystem as ClickHouse's data.

Timeline:

| time   | event                                                                                                  |
| ------ | ------------------------------------------------------------------------------------------------------ |
| ~13:00 | 8 workers start, 358 GB free                                                                           |
| 18:30  | free crosses 60 GB — watchdog logs its first line **to a file nobody was reading**                     |
| 20:20  | free = 0; workers die, CH cannot reserve space, `prices.price_ohlcv_1m` inserts fail, merges cancelled |
| 20:45  | last ledger the live indexer manages to commit                                                         |
| 07:15  | operator finds it; 148 alert lines in `~/.bf/disk.log`, all unread                                     |

### The second consumer — found a day later, and it was the larger one

The first post-mortem blamed the scratch alone. It was wrong. On 2026-08-14,
with the box again at 51 GB free and the arithmetic refusing to close, a plain
`du -sh /*` found **`/home/deploy/tmux-server-3607624.log` at 351 GB, growing
4.5 MB/s — 385 GB per day.**

The tmux server had been started as `tmux -v` (verbose server logging) at some
point months earlier, and every session on the box fed it. The backfill
workers ran with `-v` too, so thousands of `parsing ledger` lines per second
went through tmux and into that file, on the same filesystem as ClickHouse.

Reconciling the 358 GB the outage consumed: ~233 GB was scratch, and the rest
was this log. **Isolating the scratch was necessary and would not have been
sufficient** — an unbounded writer sat outside every boundary we drew.

Truncating it in place (`truncate -s 0`, the file being held open) returned
335 GB instantly. Killing the server and restarting it without `-v` stopped
the source; 14 of the 15 sessions were month-old idle shells whose scrollback
was archived to `~/tmux-archive/` first.

Two lessons beyond the disk:

- **Diagnosis order.** Both times the box filled, the reflex was to inspect
  the suspect we already knew about. `du -sh /*` costs seconds and would have
  found this a day earlier.
- **Verbose logging on a long batch run is not free.** Dropping `-v` from the
  workers and the tmux server raised throughput from ~840 to ~1,980
  ledgers/min per worker — 4.5 MB/s of log traffic was competing with
  ClickHouse for the same RAID.

Three independent failures compounded:

1. **No isolation.** Batch scratch and the production database competed for
   the same free space, and the batch job won.
2. **No actuator.** The watchdog detected the condition correctly for 110
   minutes before the disk hit zero, but it could only append to a log file.
   A detector without an actuator documents the accident.
3. **A wrong capacity model, held only in a human's head.** Peak scratch was
   sized at `8 workers × 2 partitions × 12.6 GB ≈ 202 GB` against 358 GB free.
   Reality consumed all 358 GB — partitions in recent eras run 13.0–13.6 GB
   and workers transiently hold a third one, because the sync races ahead of
   the parse.

A fourth, separate finding from the same run: the backfill starved the live
indexer of ClickHouse write capacity. **94% of per-ledger backfill time is
the CH write** (parse 4.7 ms, persist 71.8 ms), so 8 parallel writers left
the live path queueing. Throughput did not improve with more workers —
6 workers gave 8,547 ledgers/min, 12 gave 9,118 — which is the same fact seen
from the other side.

## Implementation Plan

### Step 1: Isolate scratch (done manually 2026-08-14 — make it standard)

`/srv/bf-scratch` is a loopback ext4 image capped at 89 GB, mounted
`loop,noatime,discard`, owned by `deploy`. Verified: writing past the cap
returns ENOSPC inside the scratch while `/` is untouched, and `discard`
returns freed blocks to `/` as the runner deletes partitions.

```bash
sudo fallocate -l 90G /srv/bf-scratch.img
sudo mkfs.ext4 -m 0 -q -L bf-scratch /srv/bf-scratch.img
sudo mount -o loop,noatime,discard /srv/bf-scratch.img /srv/bf-scratch
sudo chown deploy:deploy /srv/bf-scratch
```

To make it survive a reboot (the box has a pending restart):

```
/srv/bf-scratch.img /srv/bf-scratch ext4 loop,noatime,discard,nofail 0 0
```

Codify in `docs/backfills.md`: **a backfill never writes scratch to a
filesystem the database can allocate from.** 89 GB bounds the box to ~3
concurrent workers, which is a real constraint, not a tuning preference —
the box is at 1.5 TB of 1.7 TB.

### Step 2: The runner owns its budget (`crates/backfill-runner`)

- `--max-scratch-gb` with a **pre-flight check**: partitions in flight ×
  observed partition size × workers, against `statvfs` on the temp dir; refuse
  to start when it does not fit with margin. This is the arithmetic that was
  done by hand and got wrong.
- **Delete each ledger file immediately after it is indexed**, instead of
  keeping the whole 64,000-file partition until the partition completes.
  Turns the per-worker footprint from "two full partitions" (~27 GB) into
  "one partition plus a shrinking remainder" and makes the peak predictable.
- Re-check free space before each partition sync; abort cleanly with a
  distinct exit code rather than dying mid-write.

### Step 3: The backfill yields to live ingestion

Poll `SELECT now() - max(closed_at) FROM ledgers` every N ledgers and pause
when the lag exceeds a threshold (10 minutes is a reasonable default), resume
when it recovers. Self-throttling, so nobody has to watch.

Complement server-side: give the backfill its own ClickHouse user on a
profile with lower `priority` and a write quota, so the live indexer wins
contention for the write path by construction. Note the constraint from
task 0314/0477: these users are XML-defined (`users_xml`), so SQL `ALTER
USER` does not work — the change is a `users.d/*.xml` edit plus a compose
recreate (a plain restart keeps the stale inode).

### Step 4: Alerts reach a human through the channel that exists

Two CloudWatch alarms on the path proven by task 0367 (galexie-lag →
Slack):

- box disk free below a threshold,
- ingestion lag (`now() - max(closed_at)`) above ~15 minutes.

A file on the box is not an alarm. This is the single change that would have
turned an 11-hour outage into a 10-minute one — and, unlike the scratch cap,
it catches consumers nobody thought to bound, which is exactly how the 351 GB
tmux log went unnoticed for months.

### Step 4b: Nothing operational writes an unbounded log to the DB volume

The tmux finding generalises. Audit what writes to `/` outside ClickHouse and
either bound it or move it: `tmux -v` (now off — do not restart the server
with it), worker logs (`--verbose` off by default for long runs; when on,
rotate or cap), and any `nohup`/`tee` pattern in the runbooks. A recurring
`du -sh /*` check belongs in the disk alarm's runbook entry, because the
alarm says "space is going" and the operator needs "here is where".

### Step 5: Resume state derivable from the database

Add a `gaps` subcommand that reconstructs what is missing by comparing
partition coverage against a reference table, and prints ranges ready to feed
straight back into `run`:

```sql
-- a partition counts as done when it carries ≥99% of the ledgers the
-- reference table has for the same range
WITH expected AS (SELECT intDiv(ledger_sequence,64000) AS p, uniqExact(ledger_sequence) AS n
                  FROM operation_pools WHERE ledger_sequence BETWEEN ? AND ? GROUP BY p),
     present  AS (SELECT intDiv(ledger_sequence,64000) AS p, uniqExact(ledger_sequence) AS n
                  FROM lp_operation_amounts WHERE ledger_sequence BETWEEN ? AND ? GROUP BY p)
SELECT e.p FROM expected e LEFT JOIN present g ON e.p = g.p
WHERE coalesce(g.n, 0) < e.n * 0.99
```

This query is what recovered the 0279 run after the crash: three successive
worker layouts (6 → 12 → 8) had written overlapping bands, and the logs died
with the disk, so the only trustworthy source was the data itself. Make it a
command instead of tribal knowledge.

### Step 6: A backfill is a change with a plan

Preconditions checklist in `docs/backfills.md`, to be filled before starting:
free scratch ≥ computed need, live lag < threshold, alarms armed, declared
worker count and expected wall time, and an explicit stop rule.

## Acceptance Criteria

- [ ] Scratch isolation documented in `docs/backfills.md` as a precondition,
      with the loopback recipe and the reboot caveat
- [ ] `--max-scratch-gb` + pre-flight capacity check; run refuses to start
      when the budget does not fit
- [x] Per-ledger file deletion — landed with this task's own PR
      (`ingest.rs::drop_indexed_file`, on both the indexed and the
      already-in-DB path, so a resumed partition shrinks too)
- [ ] Peak footprint per worker measured on a real run and recorded here —
      the number that says whether the bound above is worth what it claims
- [ ] Ingestion-lag self-throttle with a configurable threshold
- [ ] Backfill runs under a dedicated CH user with lower priority + quota
- [ ] Two CloudWatch → Slack alarms live (disk free, ingestion lag), test-fired
- [ ] Audit of unbounded writers on the DB volume; tmux `-v` confirmed off and
      worker logs bounded
- [ ] `gaps` subcommand emits ranges that `run` accepts unchanged
- [ ] Preconditions checklist in `docs/backfills.md`
- [ ] **Docs updated** — `docs/backfills.md` (preconditions, isolation,
      resume-from-gaps); no `docs/architecture/**` change, this does not alter
      the shape of the system
- [ ] **API types regenerated** — N/A, nothing under `crates/api/**`

## Notes

The immediate 0279 backfill can resume before this task lands: scratch
isolation (Step 1) is already in place and bounds the blast radius to the
89 GB image. Steps 2–6 are what stop the next backfill — and there will be
one, `operation_pools` retirement and any future derived table both imply a
full-history re-parse — from rediscovering the same hole.

Worth revisiting alongside this: the write path itself. At 71.8 ms per ledger
the backfill is bound by many small inserts. A bulk pattern (parse to Native
files, then a few large `INSERT`s) would attack the actual bottleneck and
reduce the pressure that made the live indexer fall behind in the first place.
