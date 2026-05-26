---
id: '0236'
title: 'FEATURE: Declarative Hetzner Storage Box subaccount + SSH key via API'
type: FEATURE
status: completed
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
