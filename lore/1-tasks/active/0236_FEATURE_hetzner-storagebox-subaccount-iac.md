---
id: '0236'
title: 'FEATURE: Declarative Hetzner Storage Box subaccount + SSH key via API'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0227', '0235']
tags:
  [
    priority-medium,
    effort-small,
    layer-infrastructure,
    ansible,
    hetzner-robot,
    hetzner-cloud,
  ]
links: []
history:
  - date: '2026-05-19'
    status: backlog
    who: fmazur
    note: 'Spawned from 0227 — Storage Box subaccount `<storagebox-sub>` was created manually in Hetzner Cloud Console (chroot `/borg-ch-prod-01-repo`, SSH on, password set, **no pubkey wired yet**). Wiring the Borg pubkey and the first end-to-end backup roundtrip are deferred here because Cloud Console UI does not expose SSH key management for subaccounts and the operator preferred to unblock backfill rather than route the pubkey via SFTP-with-password. This task brings the subaccount + pubkey + first-run validation under IaC.'
  - date: '2026-05-25'
    status: active
    who: fmazur
    note: 'Promoted from backlog to active.'
  - date: '2026-05-25'
    status: active
    who: fmazur
    note: 'Approach revised after API verification — the Hetzner Robot Web Service Storage Box endpoints (spec''s recommended "path 1") were REMOVED on 30 Jul 2025; Storage Boxes now live exclusively in the Hetzner Cloud API. Pivoted to the `hetzner.hcloud` collection (Cloud API, new `HCLOUD_TOKEN`). Also confirmed NO Ansible module (neither `community.hrobot.storagebox_subaccount` ≥2.4.0 nor `hetzner.hcloud.storage_box_subaccount`) manages SSH *public keys* — both only toggle SSH access — so pubkey registration is done by writing the subaccount `authorized_keys` over SFTP. See "Revised approach" below.'
  - date: '2026-05-26'
    status: completed
    who: fmazur
    note: >
      Implementation complete: new `hetzner_storagebox` role (reconcile.yml +
      authorize.yml + main.yml), 6 supporting files changed (group_vars,
      site.yml two storagebox plays, requirements.yml +hetzner.hcloud 6.9.x,
      backup role + ch-backup.sh.j2, README). Statically validated:
      `ansible-playbook --syntax-check` green against the real collection,
      YAML parse OK, key Jinja/idempotency logic unit-tested on localhost,
      programmatic no_log + credential audit clean. Three code-review rounds
      (5 finder angles each) — all surfaced bugs fixed; remaining items are
      accepted/inherent or pre-existing (logged under Issues). Marked
      completed at operator request. NOTE: runtime acceptance criteria
      (AC1–AC5) require a live `ansible-playbook` against BX21 and remain
      UNCHECKED pending the operator's first deploy — see Acceptance Criteria.
  - date: '2026-06-17'
    status: active
    who: fmazur
    note: >
      Reopened for the first operator deploy. Before deploying, the backup
      cadence/retention is being revised to fit the BX21 space budget and the
      re-derivable-from-S3 data model: switch the Borg cron from daily to
      WEEKLY (Sunday), retention to keep 4 (keep-weekly=4, keep-daily=0,
      keep-monthly=0), and fix the `borg compact` Sunday-only gate so reclaimed
      space is actually freed under the new cadence. See "Reopen scope
      (2026-06-17): weekly cadence + retention + compact fix" below. No change
      to the hetzner_storagebox role; AC1–AC5 still verified on this deploy.
  - date: '2026-06-18'
    status: active
    who: fmazur
    note: >
      First operator deploy (--tags backup,storagebox, validation off):
      subaccount CREATED on BX21 (no 0227 subaccount actually existed) and the
      Borg pubkey authorised via the bootstrap path — the hetzner_storagebox
      role works. BUT the first backup exposed a fatal design flaw: ch-backup's
      `BACKUP DATABASE TO Disk` writes a FULL local copy (~737 GiB) before borg
      pushes it, and on this box dataset ≈ disk (737 GiB on 1.8 TB) → ENOSPC.
      The full disk wedged ClickHouse's logging channel; with NO docker log
      rotation the container json log ballooned to ~209 GiB, sustaining a
      disk-full spiral (full prod incident). Recovered by truncating docker
      json logs + restarting CH; weekly cron DISARMED so it cannot re-trigger
      the broken backup. Scope EXPANDED in this task: redesign ch-backup to
      FREEZE+borg (no full local copy; ledgers-first freeze order), add docker
      log rotation, re-arm cron after. See "Backup mechanism redesign
      (2026-06-18)" below.
---

# FEATURE: Declarative Hetzner Storage Box subaccount + SSH key via API

## Summary

Add a task to `infra-hetzner/ansible/roles/hetzner` that creates (or
idempotently reconciles) a Hetzner Storage Box subaccount dedicated
to the Borg backup cron, including the SSH public key registration.
Removes the only remaining manual click step from the 0227 first-
deploy procedure.

## Context

Task 0227 shipped the Hetzner production deployment with a single
manual step preserved: creating the BX21 subaccount + uploading
the cron's `ed25519` pubkey via Hetzner Cloud Console. The decision
to defer was a scope call — 0227 spec already mandated "everything
declarative" but adding subaccount automation risked expanding the
task while we still had unverified Robot API behaviour (see 0235).

Two API surfaces are candidates:

1. **Hetzner Robot REST API**
   (`POST /storagebox/{id}/subaccount`,
   `PUT /storagebox/{id}/subaccount/{username}/key`):

   - Pro: same webservice user we already use for rDNS / firewall.
   - Con: behaviour for our account is unverified; 0235 documents a
     possible per-feature permission gap on the webservice user
     (`reverse_dns` returns "IP not found").
   - Ansible module: `community.hrobot.storagebox_subaccount` (1.9+).

2. **Hetzner Cloud API**
   (`POST /v1/storage_boxes/{id}/sub_accounts` with `ssh_keys` in
   the body, one round trip):
   - Pro: single endpoint covers subaccount + SSH keys; modern
     surface; Console-equivalent.
   - Con: requires a separate Hetzner Cloud API token (different
     from Robot user/pass); not yet exposed via Ansible community
     module — likely needs `ansible.builtin.uri`.
   - REST docs:
     https://docs.hetzner.cloud/#storage-box-sub-accounts

Both are viable. Recommendation: prototype path 1 first because it
re-uses the credentials already wired through `group_vars/all.yml`;
fall back to path 2 if Robot API webservice permissions are
locked down.

## Revised approach (2026-05-25)

The framing above is **superseded**. API verification during
implementation found:

1. **Path 1 (Robot REST API) is dead.** Hetzner removed the Robot
   Web Service Storage Box endpoints on **30 Jul 2025** and moved
   Storage Box administration into the Hetzner Console / Cloud API
   (status incident `f06ffe20`). The `/storagebox/{id}/subaccount`
   routes on `robot-ws.your-server.de` no longer exist. The spec's
   claim that `community.hrobot.storagebox_subaccount` exists "(1.9+)"
   is wrong — the module landed in `community.hrobot` **2.4.0**
   (token auth in 2.5.0); the pinned `1.9.5` does not ship it.

2. **No Ansible module manages SSH _public keys_ for a subaccount.**
   Both `hetzner.hcloud.storage_box_subaccount` (collection 6.9.0)
   and `community.hrobot.storagebox_subaccount` (≥2.4.0) only expose
   an `ssh_enabled` / `ssh` boolean to toggle the access _method_ —
   neither uploads or revokes a key. SSH keys on a Storage Box
   subaccount are authorised the same way as on the master account:
   by writing the subaccount's `~/.ssh/authorized_keys` over
   SSH/SFTP (port 23 = OpenSSH format).

**Chosen implementation:**

- **Subaccount lifecycle** via the official **`hetzner.hcloud`**
  collection (`storage_box_subaccount` + `storage_box_subaccount_info`),
  Cloud API, authenticated with a new **`HCLOUD_TOKEN`**. Isolated
  new collection — zero blast radius on the existing `community.hrobot`
  `hetzner` role (which keeps its Robot user/pass auth).
- **Pubkey registration** by rendering the subaccount
  `authorized_keys` over SFTP (overwrite, not append → gives the
  delta-reconciliation / revocation the acceptance criteria want).
  Run from the box (inside Hetzner) so `reachable_externally` can
  stay `false`; the subaccount password (new secret) is used only
  for that transient login, `no_log`.
- **Dynamic discovery** of `STORAGEBOX_SSH_USER` / `STORAGEBOX_SSH_HOST`:
  the Cloud API response carries `username` (e.g. `u514605-sub1`) and
  `server` (e.g. `u514605-sub1.your-storagebox.de`), so we
  `set_fact` them onto the box host before the `backup` role runs
  (spec option (a)). This is _required_ for the fresh-box case — the
  operator cannot know the Hetzner-assigned subaccount username in
  advance.

## Scope

- Add `STORAGEBOX_ID` + `HCLOUD_TOKEN` env vars to
  `group_vars/all.yml` (required for the Cloud API) plus a new
  subaccount password secret; add preflight assertions. Demote
  `STORAGEBOX_SSH_USER` / `STORAGEBOX_SSH_HOST` from required to
  optional (now discovered, env only pins/overrides).
- New role `roles/hetzner_storagebox/` with its own `storagebox`
  tag so it runs independently of the existing `hetzner` tag.
- Tasks:
  - Reconcile subaccount (`borg-backup-ch-prod-01`, homedir
    `borg-ch-prod-01-repo`, SSH on, SMB/WebDAV/Read-Only off)
    via `hetzner.hcloud.storage_box_subaccount`. Idempotent match
    by subaccount `name` (+ labels); adopt the 0227 manually-created
    one via `storage_box_subaccount_info` lookup on `home_directory`.
  - Discover `username` / `server` from the API and `set_fact`
    them onto the box host for the downstream `backup` role.
  - Render the subaccount `authorized_keys` from the box's Borg
    pubkey (`/root/.ssh/borg_ed25519.pub`, generated by the
    `backup` role) over SFTP, overwriting for delta reconciliation.
- **First-run validation** — after the pubkey is wired, execute
  `/usr/local/bin/ch-backup` once via Ansible and assert the Borg
  repo on the BX21 lists at least one `ch-*` archive. Carry-over
  from 0227's "Borg backup runs successfully" criterion. Gated
  behind a var (default on) since it runs a full backup.
- Remove the manual Phase 5 / Robot-UI key-add instructions from
  `infra-hetzner/README.md` (also in the DR runbook + the `backup`
  role's "add this key via Robot UI" debug task). Replace with a
  one-liner referencing `STORAGEBOX_ID` / `HCLOUD_TOKEN`.

## Reopen scope (2026-06-17): weekly cadence + retention + compact fix

Added on reopen, ahead of the first operator deploy. Motivation: the
Borg repo on the BX21 grows with the ClickHouse corpus (~700 GB per
full snapshot and rising). Borg deduplicates, so the repo is NOT
`N × full` — but the operator wants a tighter, predictable window. The
ClickHouse data is **re-derivable from the `stellar-ledger-data` S3 XDR
archives** (the indexer is resumable), so a 7-day RPO is acceptable: a
worst-case box loss means re-ingesting up to a week, not permanent loss.
The backup itself stays **online** — ClickHouse `BACKUP DATABASE`
snapshots the immutable MergeTree parts present at start; the indexer
keeps writing (new parts land in the next run), so **no maintenance
window / writer stop is required**.

Changes (all in the `backup` role + group_vars; the `hetzner_storagebox`
role is untouched):

1. **Cadence: daily → weekly (Sunday).** Add `borg_cron_weekday`
   (env `BORG_CRON_WEEKDAY`, default `0` = Sunday) and pass `weekday:`
   to the `ch-backup` cron task. Sunday is chosen so the existing
   compact path also fires (see 3); a true "every 7 days" is not a clean
   cron expression, so a pinned weekday is the right mechanism. Hour /
   minute (03:30 UTC) + jitter unchanged.
2. **Retention: keep 4.** group_vars defaults `borg_keep_daily 7 → 0`,
   `borg_keep_weekly 4` (unchanged), `borg_keep_monthly 6 → 0`. Yields a
   4-archive (~4-week) window. `keep-weekly=4` protects the single first
   archive on a fresh repo (last-of-week), so prune never deletes it.
   Still env-overridable.
3. **Fix the `borg compact` gate.** `ch-backup.sh.j2` only compacts on
   Sundays (`date +%w == 0`). With a weekly backup on a non-Sunday that
   would mean compact NEVER runs → `prune` marks segments dead but space
   is never reclaimed → repo bloats (the opposite of the goal). Run
   `compact` on every backup run instead (weekly cadence removes the
   original daily-lock-contention rationale), making it independent of
   the chosen weekday.
4. **Docs:** update `infra-hetzner/README.md` (env-override block defaults
   - the DR line "until the next _daily_ run lands" → weekly). Architecture
     docs state Borg backups but not the cadence → N/A.

## Backup mechanism redesign (2026-06-18): `BACKUP TO Disk` → FREEZE + borg

**Why (supersedes the `BACKUP DATABASE` mechanism above).** The first deploy
proved `ch-backup` is unworkable on ch-prod-01. `BACKUP DATABASE ... TO
Disk('backups')` writes a **full copy** of the dataset to `/srv/backups` on
the same `/dev/md1` before borg pushes it. With dataset ≈ disk (737 GiB on
1.8 TB), a full local copy needs ~2× → ENOSPC. It filled the disk, wedged
ClickHouse's logging channel, and — with **no docker log rotation** — the
container json log grew to ~209 GiB → self-reinforcing disk-full spiral / prod
incident. Borg dedup only bounds the **remote** repo; it does NOT shrink the
**local staging** (CH writes a full copy every run), so the local-staging
model can never fit here.

**New mechanism — FREEZE + borg → BX21 (no full local copy):**
Replace `BACKUP TO Disk` with `ALTER TABLE ... FREEZE` (hardlinks immutable
MergeTree parts into `shadow/`, **~0 extra disk**), then `borg create` reads
the frozen `shadow/` tree and pushes to the BX21 subaccount, then UNFREEZE.

- **Local disk ~0** (hardlinks) + small borg chunk-index cache → removes the
  ENOSPC root cause.
- **Encryption (required):** borg `repokey-blake2` = authenticated client-side
  encryption. The indexer's _derived_ data must not leak — Hetzner only ever
  sees ciphertext. (Rules out plain SSE object storage where the provider
  holds keys; rules out `BACKUP→mounted-StorageBox`.)
- **Online (verified):** FREEZE does not block INSERT/SELECT — measured 238
  inserts + 238 selects, 0 errors, during 243 concurrent freeze cycles
  (CH 26.3). Snapshot isolation preserves point-in-time values even for a
  ReplacingMergeTree row updated + merged after the freeze (live returns the
  latest; restored-from-freeze returns the frozen version).
- Borg is Hetzner's recommended Storage Box tool (chunked, resumable, dedup,
  encrypted over SSH) → reuses the 0236 subaccount/key/repo. (Direct CH
  `BACKUP` to a mounted Storage Box rejected: small-file SSHFS/CIFS is slow +
  fragile, no resume.)

### ⚠️ Freeze ORDER is critical — `ledgers` FIRST (marker-first)

Our indexer writes the **`ledgers` row LAST** as a commit marker: _if a row
exists in `ledgers`, all other data from that ledger's XDR is already in the
DB._ Therefore the FREEZE loop MUST freeze **`ledgers` first**, then every
other table (any order among the rest):

```
1. FREEZE ledgers            ← last-written commit marker → frozen FIRST
2. FREEZE <all other tables>
```

Freezing `ledgers` at T1 then the detail tables at T2 > T1 guarantees every
ledger present in the frozen `ledgers` had its children written before
T1 < T2 → the children snapshot ⊇ what `ledgers` claims → **no orphans; the
frozen `ledgers` is a conservative, trustworthy high-water mark.** Detail
tables may hold a little extra (a partially-ingested ledger beyond the marker)
— harmless, reconciled on restore. This yields a referentially-consistent
backup **without pausing the indexer**.

Freezing `ledgers` LAST would be **WRONG** — it could then claim ledger N
whose children were not captured → orphan rows / a lying marker. The freeze
order MUST be an **explicit, configurable list with `ledgers` pinned first** —
never "whatever `system.tables` returns".

### Restore procedure

Place frozen parts into each table's `detached/`, then `ALTER TABLE ...
ATTACH`. High-water mark = `max(ledger)` from the restored `ledgers`; resume
ingestion from `max(ledger)+1` (data re-derivable from S3). Optionally trim
detail rows for ledgers `> max(ledger)` before resuming (only needed if the
indexer is not idempotent on partial re-ingest).

### Optional pause (default OFF)

Marker-first already gives a referentially-consistent snapshot. For absolute
cross-table atomicity, pause the indexer **only for the FREEZE loop** (seconds
at 379 parts / 19 tables), NOT for the borg upload (which reads immutable
frozen parts while the indexer runs).

### Companion fixes (in this task)

- **Docker log rotation** — add `logging: {driver: json-file, options:
{max-size: "100m", max-file: "5"}}` to the `clickhouse` service in the
  compose prod overlay + recreate, and mirror it in the `app`/`docker` role
  (and ideally `daemon.json`). Missing rotation is what turned a failed
  backup into the incident.
- **Re-arm the cron** — the FREEZE-based `ch-backup` redeploy restores
  `/etc/cron.d/ch-backup` (weekly Sunday 03:30 UTC). Until then the box has
  **NO scheduled backup** (cron DISARMED 2026-06-18 →
  `/root/ch-backup.cron.disabled`).
- **`snapshot_pre_0281` (~761 GiB)** — delete once the 0281 rollback is no
  longer needed → frees ~half the disk (operational, not code).
- **Capacity** — ch-prod-01 is undersized (737 GiB data on 1.8 TB, snapshot
  consuming the rest); track under 0216.

### Empirically validated (2026-06-18)

The redesign's assumptions were proven on a local CH 26.3.12 sandbox
(== prod major) before implementation:

1. **Online** — INSERT + SELECT ran uninterrupted during FREEZE: 238
   inserts + 238 selects, **0 errors**, across 243 concurrent
   FREEZE/UNFREEZE cycles.
2. **Snapshot isolation + encrypted Borg roundtrip** — 1000-row table;
   after FREEZE it was heavily mutated (→V2, +500 rows, OPTIMIZE) AND
   again post-archive (all→V3); a full FREEZE → `borg create` (repokey) →
   `borg extract` → ATTACH roundtrip restored **exactly** the freeze-time
   fingerprint, **≠** live. Bit-for-bit.
3. **Literal single row** — freeze X=`ORIGINAL`; ledger changes it to
   `CHANGED` (RMT) + merge: live query returns `CHANGED`,
   restored-from-borg returns `ORIGINAL`.
4. **`ledgers`-first ordering** — multi-table test: marker-first → **0
   orphans**; marker-last → **1 orphan** (a ledger whose children weren't
   captured). Confirms the freeze order is load-bearing.
5. **Partitioned multi-table restore (drill)** — full FREEZE → `borg create`
   (repokey) → `borg extract` → per-table `cp *_*` + `ATTACH PART` restore of
   a mixed schema (2 PARTITION-BY tables with parts `0_*…4_*`, 1 plain
   MergeTree, 1 RMT) reconstructed **every** table bit-for-bit (row counts +
   cityHash sums == backup-time; live mutated + reverted in between).
   Validates the partition-aware restore (review finding #2) end-to-end.

A max-effort code review (10 finder angles) ran on the redesign; it found
and fixed 2 critical bugs (a `docker compose exec -T` stdin-steal that froze
only the marker table; the `all_*`/`ATTACH PARTITION ID 'all'` restore that
left partitioned tables empty) plus hardening (borg exit-1 handling, signal
trap exit, `flock`, marker-first guard, TOCTOU single-query, dead-`/srv/backups`
cleanup). Credentials scan: clean.

(`FREEZE` ≈ 0 disk is by construction — hardlinks.) Restore gotchas the
tests surfaced are baked into the DR runbook: `chown 101:101` the copied
parts, `data_paths[1]` (no scalar `path`), `ATTACH PARTITION ID 'all'`,
`SYSTEM UNFREEZE` disabled → per-table `ALTER ... UNFREEZE WITH NAME`,
and a `_table_uuids.tsv` map written into the shadow tree so a restore
can map Atomic-DB `store/<uuid>/` dirs back to table names.

## Acceptance Criteria

> AC1–AC5 are **runtime** criteria: they require a live
> `ansible-playbook` against the BX21 and are **not yet checked** —
> verification is the operator's first deploy. The implementation is
> complete and statically validated (syntax-check, unit-tested Jinja,
> no_log/credential audit, three review rounds). Tick these after the
> deploy confirms them.

- [ ] `ansible-playbook` against a fresh BX21 (no subaccount yet)
      creates the subaccount and registers the pubkey in a single
      run, without UI interaction.
- [ ] `ansible-playbook` against the BX21 we hand off from 0227
      (subaccount already present, **pubkey missing**) is also a
      single-run flow: detect the existing subaccount via the
      `name` / `home_directory` match, attach the pubkey, validate.
- [ ] Re-running the playbook is idempotent (no-op when the
      subaccount + pubkey already match the declared state).
- [ ] Revocation / delta: the subaccount `authorized_keys` is
      rendered (overwrite) from the declared key set, so a rotated
      or removed pubkey replaces/clears the old one on the next run
      (delta reconciliation, not just "create on missing").
- [ ] First Borg backup roundtrip succeeds: `ch-backup` runs from
      the box, the repo on BX21 lists at least one `ch-*` archive
      (script names archives `ch-<UTC-stamp>`), and re-running the
      script the next day adds a second archive (proves cron
      pathway). The day-2 check is inherently temporal — done by
      the operator after the first cron fire.
- [ ] **(reopen) Weekly cron** — the installed `/etc/cron.d/ch-backup`
      fires only on the configured weekday (Sunday by default), not daily.
- [ ] **(reopen) Retention keep-4** — `borg prune` runs with
      `--keep-weekly=4 --keep-daily=0 --keep-monthly=0`; the first/only
      archive on a fresh repo survives the prune.
- [ ] **(reopen) Compact reclaims space** — `borg compact` runs on every
      backup run (not gated to Sunday), so pruned segments are freed.
- [ ] **(redesign) FREEZE, not `BACKUP TO Disk`** — `ch-backup` uses
      `ALTER TABLE ... FREEZE` + `borg create` from `shadow/`; a backup run
      consumes ~0 extra local disk (no full local copy → no ENOSPC).
- [ ] **(redesign) `ledgers`-first freeze order** — the freeze order is an
      explicit list with `ledgers` pinned FIRST, then all other tables
      (marker-first → consistent high-water mark without pausing the indexer).
- [ ] **(redesign) encrypted borg → BX21 + restore documented** — frozen
      snapshot pushed to the subaccount, client-side encrypted; restore via
      `max(ledger)` high-water mark + ATTACH-from-detached is documented.
- [ ] **(redesign) docker log rotation** — `json-file` `max-size`/`max-file`
      on the CH container and in the role (no unbounded container logs that
      can refill the disk).
- [ ] **(redesign) cron re-armed by the FREEZE deploy** — `/etc/cron.d/ch-backup`
      restored (weekly Sunday 03:30 UTC). Currently DISARMED
      (`/root/ch-backup.cron.disabled`) so the broken backup cannot fire.
- [x] **(redesign) restore drill-tested** — full multi-table
      FREEZE → borg → extract → ATTACH restore exercised end-to-end on a
      local CH 26.3.12 sandbox with a mixed PARTITION-BY/plain/RMT schema;
      every table (incl. partitioned `0_*…4_*`) reconstructed bit-for-bit.
      (Local mechanism proven; a real BX21 restore is still the operator's
      first live DR exercise.)
- [x] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [x] **Docs updated** — `infra-hetzner/README.md` updated: `ansible-env`
      block + prerequisites (Cloud Console token + `STORAGEBOX_ID`),
      section 4 env list, routine-deploy `--tags storagebox`, and the DR
      runbook (manual Robot-UI key steps → Cloud-API automation). The
      `backup` role's manual Robot-UI debug task was likewise removed.
      `docs/architecture/**` = N/A — topology (Borg → BX21) unchanged;
      the subaccount/token mechanism lives in `infra-hetzner/README.md`.

## Out of Scope

- Snapshot plan automation (separate Storage Box endpoint —
  spawn its own task if needed).
- Master account `<storagebox-master>` SSH key management (we deliberately
  keep the master account password-only and route every automated
  access through subaccounts — separation-of-concerns).
- Migrating away from `community.hrobot` to the Cloud API
  across the whole hetzner role (different scope; only matters
  if 0235 root-causes to webservice permissions that newer Cloud
  API tokens sidestep).

## Notes

The Borg pubkey wired in the first 0227 deploy will be revoked
and reissued by this task on the first IaC run. The current
pubkey value lives on the box at `/root/.ssh/borg_ed25519.pub`
and is not duplicated into the repo (`ansible.builtin.slurp` in
the new role reads it directly).

## Implementation Notes

- New role `roles/hetzner_storagebox/` — `reconcile.yml` (Cloud-API
  subaccount create/adopt + discover `username`/`server`, set_fact onto
  the box host), `authorize.yml` (delegated-to-box: probe key →
  bootstrap-or-steady authorized_keys push → mode enforce → drift
  baseline → first-run validation), `main.yml` (wholesale post-bootstrap
  entry).
- `site.yml`: two `storagebox`-tagged plays bracket the box play
  (reconcile BEFORE so discovery reaches the `backup` role; authorize
  AFTER so the Borg pubkey exists). Preflight now requires
  `HCLOUD_TOKEN` / `STORAGEBOX_ID` / `STORAGEBOX_SUBACCOUNT_PASSWORD`
  and no longer requires `STORAGEBOX_SSH_USER/HOST` (discovered).
- `group_vars/all.yml`: +3 required secrets, subaccount declarative
  state vars, `STORAGEBOX_SSH_USER/HOST` demoted to optional.
- `requirements.yml`: +`hetzner.hcloud >=6.9.0,<7.0.0` (needs the
  `hcloud` Python SDK on the controller).
- `backup` role: hard drift-fail → conditional warn/fail keyed on
  `ansible_run_tags`/`ansible_skip_tags`; fp recording moved out (to
  authorize); manual Robot-UI debug removed. `ch-backup.sh.j2`:
  `CH_BACKUP_NO_JITTER` guard for the validation run.
- Validated: `ansible-playbook --syntax-check` green (real collection
  6.9.0), YAML parse, unit-tested Jinja (adoption match, probe
  classification, drift run/skip-tags complements, `sb_validate_now`,
  authorized_keys render), programmatic `no_log`/credential audit clean.

## Design Decisions

### From Plan

1. **`hetzner.hcloud` (Cloud API) for the subaccount lifecycle** —
   chosen by the operator over `community.hrobot` 2.x and raw `uri`
   (isolated collection, zero blast radius on the existing `hetzner`
   role). New `HCLOUD_TOKEN`.

### Emerged

2. **New secret `STORAGEBOX_SUBACCOUNT_PASSWORD`** — the Cloud API
   requires a password at create, and it is also the only way to do the
   first SFTP login that installs `authorized_keys` (no API manages SSH
   keys). Used only transiently, `no_log`.
3. **Key install runs FROM the box (delegated), not the controller** —
   keeps `reachable_externally: false` (no laptop exposure) and the
   password off the operator laptop.
4. **Bootstrap vs steady auth path** — probe the key; `reset_password` +
   password-SFTP only when the server actively DENIES the key; a
   connectivity/host-key failure fails fast instead of resetting the
   password (avoids spurious non-idempotent mutation / MITM password
   hand-off).
5. **Revocation lever = `ssh_enabled:false` / delete via Cloud API**,
   not emptying `storagebox_authorized_keys` (the box key is always
   authorised — it is the backup identity). Documented in group_vars +
   README DR.
6. **First-run validation gated to first run only** (`sb_validate_now`,
   keyed on "no `ch-*` archive yet") so steady re-runs are a true no-op;
   drift baseline (`borg.fp`) recorded last, after validation.
7. Review-hardening: `check_mode:false` on read-only probes + validation
   skipped under `--check`; `scp -p` to preserve 0600; `regex_escape` on
   the adoption home match; `default()` chains so `--check` on a fresh
   box degrades instead of aborting.

## Issues Encountered

- **Robot Storage Box API removed 2025-07-30** — the spec's primary
  "path 1" was dead; pivoted to the Cloud API (see Revised approach).
- **Pinned `community.hrobot` 1.9.5 lacks `storagebox_subaccount`** (the
  spec's "(1.9+)" claim is wrong; module landed in 2.4.0). Moot after the
  pivot.
- **No Ansible/Cloud-API surface manages a subaccount's SSH public
  keys** — forced the SFTP `authorized_keys` mechanism.

## Future Work

Pre-existing / out-of-scope items surfaced during review (left as prose
per operator preference — not spawned as backlog tasks):

- `ch-backup.sh.j2` uses `borg prune --glob-archives` (renamed
  `--match-archives` in Borg 2.x; fine on Ubuntu 24.04's Borg 1.2).
- README Borg-passphrase-rotation runbook uses `echo <new-passphrase>`
  (shell-history exposure on the box; root-only, manual op).
- `clickhouse-client.xml` mode comment in `ch-backup.sh.j2` vs the `app`
  role's actual `0400 owner=101` — doc drift.
- **Runtime validation (AC1–AC5)** against BX21 — operator's first deploy.
