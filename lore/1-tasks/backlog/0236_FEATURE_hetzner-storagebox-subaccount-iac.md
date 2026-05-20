---
id: '0236'
title: 'FEATURE: Declarative Hetzner Storage Box subaccount + SSH key via API'
type: FEATURE
status: backlog
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

## Scope

- Add `STORAGEBOX_ID` env var to `group_vars/all.yml` (required
  for Robot API endpoints) plus preflight assertion.
- New role/section `roles/hetzner_storagebox/` with its own tag
  so it can be run independently of the existing `hetzner` tag
  (which is currently broken on the auction server per 0235).
- Tasks:
  - Reconcile subaccount (`borg-backup-ch-prod-01`, homedir
    `/borg-ch-prod-01-repo`, SSH on, SMB/WebDAV/Read-Only off).
    Idempotent match by `comment` field.
  - Read the Borg pubkey from the box via
    `ansible.builtin.slurp` (`/root/.ssh/borg_ed25519.pub` —
    generated earlier by the `backup` role).
  - Register the pubkey on the subaccount. The Cloud Console UI
    does not expose this — exercise it via Hetzner Robot REST
    API (`POST /storagebox/{id}/subaccount/{username}/key`)
    using the same `HCLOUD_ROBOT_USER` / `HCLOUD_ROBOT_PASSWORD`
    that the existing hetzner role uses. Fallback to Hetzner
    Cloud API (`POST /v1/storage_boxes/{id}/sub_accounts` with
    `ssh_keys` array) if Robot returns 403/not-found like 0235.
- Update `STORAGEBOX_SSH_USER` / `STORAGEBOX_SSH_HOST` flow.
  Subaccount in Cloud Console gets its own hostname
  (`<sub>.your-storagebox.de`) distinct from master; the env
  must reflect that. Either: (a) discover dynamically from the
  Robot API response and `set_fact` for downstream `backup`
  role, or (b) document the operator update step in README.
- **First-run validation** — after the pubkey is wired,
  execute `/usr/local/bin/ch-backup` once via Ansible and assert
  the Borg repo on the BX21 lists at least one archive. This is
  the carry-over from 0227's "Borg backup runs successfully"
  acceptance criterion (deferred to this task).
- Remove the manual Phase 5 instructions from
  `infra-hetzner/README.md`. Replace with a one-liner referencing
  `STORAGEBOX_ID`.

## Acceptance Criteria

- [ ] `ansible-playbook` against a fresh BX21 (no subaccount yet)
      creates the subaccount and registers the pubkey in a single
      run, without UI interaction.
- [ ] `ansible-playbook` against the BX21 we hand off from 0227
      (subaccount already present, **pubkey missing**) is also a
      single-run flow: detect the existing subaccount via the
      `comment` match, attach the pubkey, validate.
- [ ] Re-running the playbook is idempotent (no-op when the
      subaccount + pubkey already match the declared state).
- [ ] Removing the pubkey from `group_vars` and re-running revokes
      the key on Hetzner side (delta reconciliation, not just
      "create on missing").
- [ ] First Borg backup roundtrip succeeds: `ch-backup` runs from
      the box, the repo on BX21 lists at least one archive named
      `ch-prod-01-...`, and re-running the script the next day
      adds a second archive (proves cron pathway).
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** — `infra-hetzner/README.md` Phase 5 section
      simplified to a single env-var step.

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
