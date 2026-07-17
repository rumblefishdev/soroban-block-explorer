# ClickHouse Backup & Restore

How the production ClickHouse box is backed up, how to take one by hand, and
what to do when data goes bad.

The **restore** procedure is not duplicated here — it lives in
[`infra-hetzner/README.md` § Disaster recovery](../infra-hetzner/README.md#disaster-recovery).
This file explains the backup side, the levers around it, and the step people
miss after a restore.

---

## TL;DR

| I want to…                                      | Do                                                                             |
| ----------------------------------------------- | ------------------------------------------------------------------------------ |
| understand the weekly backup                    | [§ How the weekly backup works](#how-the-weekly-backup-works)                  |
| take an off-box backup **right now**            | `sudo CH_BACKUP_NO_JITTER=1 /usr/local/bin/ch-backup`                          |
| checkpoint before a destructive op              | [§ Pre-op checkpoint](#pre-op-checkpoint--two-levers)                          |
| keep a snapshot the weekly prune won't eat      | [§ Pre-op checkpoint, lever B](#pre-op-checkpoint--two-levers)                 |
| back up **without** the Storage Box             | [§ Without a Storage Box](#backup-without-a-storage-box)                       |
| restore a broken DB                             | [`infra-hetzner/README.md` § DR](../infra-hetzner/README.md#disaster-recovery) |
| **re-ingest after a restore** (the missed step) | [§ After a restore](#after-a-restore--re-ingest-the-gap)                       |

---

## How the weekly backup works

**Mechanism** (task 0236 redesign): ClickHouse `ALTER TABLE … FREEZE` hardlinks
the immutable MergeTree parts into the data dir's `shadow/` (**~0 extra disk — no
full local copy**), Borg pushes that frozen tree to the Hetzner BX21 Storage Box
(client-side encrypted + deduplicated), then `UNFREEZE` releases the hardlinks.

This **replaced** the original `BACKUP DATABASE … TO Disk` mechanism, which wrote
a full local copy (~737 GiB) that could not fit on this box — dataset ≈ disk size
→ **ENOSPC → prod incident**. That history is why the weekly path is FREEZE-based
and why a routine local backup is not an option.

**Consistency — without pausing the indexer.** Tables are frozen with the
**marker table first**. The indexer writes that marker row **last**, as a commit
marker (a marker row implies all its children are already in the DB). Freezing it
first makes the backup's marker a **conservative high-water mark** — every marker
row in the archive has its child rows present, no orphans — with **no indexer
pause**. The script fails loudly rather than ship an orphan-prone backup if the
marker is missing, renamed, or not frozen first.

**Schedule & safety:**

- Cron: `/etc/cron.d/ch-backup`, **weekly** (Sunday 03:30 UTC by default), running
  `/usr/local/bin/ch-backup` as root.
- **Jitter:** random 0–30 min delay. Skip it with `CH_BACKUP_NO_JITTER=1` (the
  cron never sets it; manual/Ansible validation runs do).
- **flock** — a second invocation while one is running just exits 0.
- **Cleanup trap** — any non-success exit releases the frozen hardlinks, so a
  failed run never pins disk. Orphaned shadow dirs >24 h old are swept.

**Retention:** `borg prune --keep-daily 0 --keep-weekly 4 --keep-monthly 0
--glob-archives 'ch-*'`, then `borg compact` on **every** run so pruned segments
are actually reclaimed. Archives are named `ch-<stamp>`.

**The archive is self-describing.** It carries:

- `_schema.sql` — the **live** `CREATE` of every frozen table + dictionaries.
- `_table_uuids.tsv` — the uuid↔name map (Atomic DBs store parts under opaque
  UUIDs, which differ on a fresh box).

Restore recreates the schema from **`_schema.sql`, not `init.sql`** — `init.sql`
drifts from prod via online ALTERs, and a mismatched column would make parts fail
to ATTACH.

**Exit codes:** borg exit **1 = warnings, archive is usable** (e.g. a part
vanished mid-create due to a concurrent merge); **≥2 = real error**.

---

## Ad-hoc backup — same mechanism, right now

```bash
ssh sorban-prod
sudo CH_BACKUP_NO_JITTER=1 /usr/local/bin/ch-backup
tail -f /var/log/ch-backup.log
```

Same FREEZE→Borg→UNFREEZE path as the cron, minus the jitter. Costs ~0 local
disk. Safe to run alongside a live indexer.

---

## Pre-op checkpoint — two levers

Before something destructive (a backfill, a redrain, an engine swap) you want a
snapshot that **sits there until you say otherwise**, independent of the weekly
cron. Two ways, with very different costs:

|            | **A. `BACKUP … TO Disk`**                                  | **B. Pinned Borg archive**                     |
| ---------- | ---------------------------------------------------------- | ---------------------------------------------- |
| Where      | **local**, `/srv/backups` on the box                       | **off-box**, the Storage Box                   |
| Cost       | **full local copy** (~760 GiB) — needs free disk ≥ dataset | deduplicated ≈ free; **0 local disk**          |
| Cron-proof | the cron doesn't know it exists                            | name outside `ch-*` → `prune` never matches it |
| Status     | **what we actually use**                                   | **not yet used in prod**                       |

### A. `BACKUP … TO Disk` — the one we actually use

```bash
# BOX — ASYNC dodges the 300 s client receive_timeout
docker exec app-clickhouse-1 clickhouse-client -q \
  "BACKUP DATABASE default TO Disk('backups', 'snapshot_pre_<task>_$(date +%Y%m%d)') ASYNC"

# poll it
docker exec app-clickhouse-1 clickhouse-client -q \
  "SELECT name, status, error, formatReadableSize(total_size) FROM system.backups ORDER BY start_time DESC LIMIT 1"
```

- The `backups` disk is pinned in `crates/db-clickhouse/config.d/backups.xml` →
  container `/backups/` → host **`/srv/backups`** (mounted only in
  `docker-compose.prod.yml`). It is deliberately **not** under
  `/var/lib/clickhouse` — a backup disk nested in the data path fails CH startup.
  It was previously configured by hand on the box, which a container recreate
  silently dropped; pinning it in config makes it survive deploys.
- **It writes a full local copy — only use it when the host has the room.** Real
  sizes: `snapshot_pre_0281_20260615` was **760.61 GiB**; the root disk is 1.8 TB.
  Check `df -h` first.
- Nothing prunes it. **Drop it only after post-op validation passes.**
- Used before: `snapshot_pre_0281_20260615` (0281), and
  `snapshot_pre_0359_backfill_20260713` before the 0359/0379 re-parse.

> Because the drop is gated on validation, a snapshot from a run whose validation
> is still pending is **still on disk**. Worth a `df -h` if a backfill is in
> flight.

### B. Pinned Borg archive — off-box, cheap — **not yet used in prod**

The weekly prune only considers archives matching `--glob-archives 'ch-*'`. An
archive named **outside** that glob is therefore **never pruned** — it just sits
in the repo, deduplicated against the weeklies, costing almost nothing and **zero
local disk**:

```bash
ssh sorban-prod
sudo CH_BACKUP_NO_JITTER=1 /usr/local/bin/ch-backup          # creates ch-<stamp>
# then rename it out of the prune glob so the cron can never reap it:
sudo -E BORG_PASSCOMMAND="cat /etc/soroban-backup/borg.passphrase" \
        BORG_RSH="ssh -i /root/.ssh/borg_ed25519" \
        borg rename "<BORG_REPO>::ch-<stamp>" "pre-backfill-<task>-$(date +%Y%m%d)"
```

> ⚠️ **Untested.** The mechanism follows directly from the prune glob, but this
> has not been exercised on prod. Verify with `borg list` before trusting it as
> your only checkpoint.

This is the better lever when disk is tight — which on this box it usually is.

---

## Backup without a Storage Box

**A Borg repo is just an SSH target.** Nothing about the Storage Box is special:

```bash
BORG_REPO="ssh://user@some-host:22/./path/to/repo"
```

Point `borg_repo_url` (or `BORG_REPO`) at any machine with disk, authorise the
box's `/root/.ssh/borg_ed25519` pubkey there, and the same `ch-backup` script
works unchanged.

The other way to get data onto a machine is the **ADR 0045 export direction** —
`FREEZE` + `rsync` the shadow tree to any host. That is how the first
multi-machine backfill moved data; see
[`docs/backfills.md` § Path B2](backfills.md#b2--freeze--rsync--attach-part-adr-0045).

> **Do not** use the box's own local disk as a routine backup target. That is
> exactly the 0236 ENOSPC incident. `BACKUP … TO Disk` is a _deliberate, checked_
> pre-op exception, not a backup strategy.

---

## Restore

➡️ **[`infra-hetzner/README.md` § Disaster recovery](../infra-hetzner/README.md#disaster-recovery)**
— the full procedure: `borg list` / `borg extract`, recreate the exact schema from
`_schema.sql`, `ATTACH PART` per table via `_table_uuids.tsv`, `chown 101:101`,
`SYSTEM RELOAD DICTIONARIES`, completeness check, resume point.

Two things to carry in your head:

- **There is no SQL `RESTORE`.** You recreate the schema and re-attach parts.
- Use **`_schema.sql` from the archive**, never `init.sql`.
- A `NO PARTS` / `ATTACH FAILED` / `PARTIAL` line in that procedure is a **red
  flag** — the table restores empty or short. Do not declare success.

The restore has been drill-tested locally end-to-end, but a real BX21 restore is
still the operator's **first live exercise** — rehearse on a throwaway box before
you need it.

---

## After a restore — re-ingest the gap

**This is the step that gets missed.** Restoring rolls the database back. It does
**not** roll back the ingestion pipeline: the Lambdas already consumed the S3/SQS
events for the ledgers after the backup point, and **they will not re-deliver
them**. Left alone, the restored DB simply stays short.

1. **Find the resume point.** The marker table is a conservative high-water mark,
   so its max is the last fully-committed ledger:

   ```sql
   SELECT max(sequence) + 1 AS resume_from FROM default.ledgers;
   ```

2. **Re-ingest the gap yourself:**

   ```bash
   backfill-runner run --start <resume_from> --end <tip>
   ```

   `run` is a **gap-filler** — it reads `ledgers` and skips what is already there,
   so it fills exactly the missing range. This is the correct tool after a
   restore.

3. **If the data is bad _in place_** rather than missing (the range is still in
   `ledgers`), `run` will **no-op silently**. You need `--reindex` to bypass the
   resume-skip — see [`docs/backfills.md`](backfills.md).

4. **Then `repair-tier1`** (with the indexer stopped) and validate. See
   [`docs/backfills.md` § After any backfill](backfills.md#after-any-backfill).

> ⚠️ **Cross-build hazard.** If the gap was originally ingested by a _different_
> parser build than the one you re-parse with, the re-parse is **unsafe on
> version-less RMT tables** (`liquidity_pool_snapshots`, `assets`, `transactions`,
> the 9 event-log tables) — at equal version RMT may keep the **stale** row. See
> [`docs/backfills.md` rule 4](backfills.md#4-re-parsing-with-a-different-parser-build-is-unsafe-on-version-less-rmt).

---

## Where things live

| Thing                   | Path                                                                                                                            |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Backup script           | `/usr/local/bin/ch-backup` (rendered from `infra-hetzner/ansible/roles/backup/templates/ch-backup.sh.j2` — do not edit by hand) |
| Cron                    | `/etc/cron.d/ch-backup`                                                                                                         |
| Log                     | `/var/log/ch-backup.log` (rotated weekly, 26 weeks)                                                                             |
| Borg passphrase         | `/etc/soroban-backup/borg.passphrase`                                                                                           |
| Borg SSH key            | `/root/.ssh/borg_ed25519` (root has no default identity — `BORG_RSH` must point at it)                                          |
| Local pre-op snapshots  | host `/srv/backups` (container `/backups/`)                                                                                     |
| Disk config             | `crates/db-clickhouse/config.d/backups.xml`                                                                                     |
| Storage Box / retention | `infra-hetzner/ansible/group_vars/all.yml` + the `soroban-prod / ansible-env` password-manager entry                            |

Passphrase rotation (and the leak-response variant, which `borg key
change-passphrase` does **not** cover) is documented in
[`infra-hetzner/README.md` § Borg passphrase rotation](../infra-hetzner/README.md#borg-passphrase-rotation).
