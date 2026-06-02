---
id: '0237'
title: 'FEATURE: reboot-required notification (Slack / email) for ch-prod-01'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0227']
tags:
  [
    priority-medium,
    effort-small,
    layer-infrastructure,
    ansible,
    monitoring,
    uptime,
  ]
links: []
history:
  - date: '2026-05-20'
    status: backlog
    who: fmazur
    note: 'Spawned from 0227 — `roles/security` was changed to `Unattended-Upgrade::Automatic-Reboot "false"` after operator preference for planned maintenance windows over surprise 03:00 UTC reboots. Without an automatic reboot, the box now silently accumulates `/var/run/reboot-required` markers between operator visits. This task adds a notification cron so the operator learns about a pending reboot the same day, not at next SSH login.'
---

# FEATURE: reboot-required notification (Slack / email) for ch-prod-01

## Summary

Add a periodic check on `/var/run/reboot-required` that fires a
notification (Slack webhook or email) when the box has security
updates installed that need a kernel/libc/systemd restart. Closes
the visibility gap created by disabling `unattended-upgrades`'
automatic reboot in 0227's `security` role.

## Context

Task 0227 wired `unattended-upgrades` to install security patches
automatically but **not** reboot the box (`Automatic-Reboot "false"`
— operator preference for planned maintenance windows). Apt still
creates `/var/run/reboot-required` (and a `.pkgs` companion listing
which packages drove the requirement) whenever an installed update
needs a restart to take effect.

Without notification:

- Pending kernel / libc / openssl fixes sit unapplied between
  operator SSH sessions.
- The MOTD banner (`*** System restart required ***`) only surfaces
  at interactive login, easy to miss in remote-only workflows.
- Real-time security patches lose half their value if the live
  process keeps running the old binary for weeks.

## Scope

### Implementation

- New Ansible task in `roles/security/tasks/main.yml`:

  - Template a small shell script at
    `/usr/local/bin/check-reboot-required` that:

    1. Checks `[[ -f /var/run/reboot-required ]]`.
    2. If yes, posts a message to the operator's chosen channel:
       package list (`cat /var/run/reboot-required.pkgs`), uptime,
       last-update timestamp.
    3. Includes a 24h cooldown (touch a sentinel file) so the
       notification fires at most once per day until the operator
       resolves it by rebooting.

  - Install a `/etc/cron.d/check-reboot-required` cron entry that
    runs the script daily at a low-traffic hour (`07:00 UTC` —
    after the apt nightly job finishes, before EU working hours
    so the operator sees it at start of day).

  - Logrotate config for `/var/log/check-reboot-required.log`
    (mirror of the `ch-backup.log` pattern from `roles/backup`).

### Notification channel choice (decide in implementation)

Two viable backends, decide based on team's existing tooling:

1. **Slack incoming webhook** — `REBOOT_NOTIFY_SLACK_WEBHOOK` env
   var, posted JSON to `https://hooks.slack.com/services/...`.
   No SMTP setup, no Postfix on the box.
2. **Email via `msmtp` + SES** — set up a relay on the box, post
   to an `infra-alerts@<domain>` alias. Reuses any existing
   on-call mailing list.

Slack is the lighter setup; email is more universal and survives
team chat-tool migrations. Either way, the webhook URL / SMTP
password lives in `~/.config/soroban-prod.env` and is delivered to
the box via the existing template-rendering pattern (see
`roles/app/templates/env.j2`).

### Pre-flight assertion

Add `REBOOT_NOTIFY_*` to `site.yml`'s `Required environment values
must be set` assertion so the playbook fails fast if the operator
forgot to source the new variable.

## Acceptance Criteria

- [ ] Touch `/var/run/reboot-required` manually on the box and the
      cron script fires the notification within 24h (or wait one
      apt cycle).
- [ ] Re-running the playbook with no env change is a no-op
      (script + cron + env file unchanged).
- [ ] Removing `/var/run/reboot-required` resets the cooldown
      sentinel so the next required-reboot detection fires again.
- [ ] Notification includes: hostname, list of packages requiring
      restart (from `.pkgs`), uptime, last apt history entry.
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** — `infra-hetzner/README.md` operating-model
      section: document the notification expectation + how to
      acknowledge (manual reboot).

## Out of Scope

- Automatically performing the reboot from the notification path
  (the whole point of 0227's manual-reboot stance is operator-
  in-the-loop decisions).
- Generic monitoring stack (Prometheus AlertManager, etc.) — this
  is the smallest viable notifier specifically for the reboot
  flag. Broader observability is its own task.
- Multi-channel notification (Slack AND email AND PagerDuty) —
  pick one for this task; layering is straightforward but adds
  setup surface.
