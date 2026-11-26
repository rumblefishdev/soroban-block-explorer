---
id: '0237'
title: 'FEATURE: reboot-required alert for ch-prod-01'
type: FEATURE
status: backlog
related_adr: ['0062']
related_tasks: ['0227', '0681', '0683']
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
  - date: '2026-11-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Redesigned under ADR 0062 (proposed, PR #762): the reboot flag becomes a textfile that
      0683's host timer writes and a rule on the monitoring VM, instead of a cron with its own
      Slack webhook or mail relay and a cooldown file. Title changed from "notification (Slack /
      email)". The context from 0227 is unchanged.
---

# FEATURE: reboot-required alert for ch-prod-01

## Summary

Alert when `/var/run/reboot-required` exists on ch-prod-01: the box has security updates
installed that need a kernel, libc or systemd restart. This closes the visibility gap created by
disabling `unattended-upgrades`' automatic reboot in 0227's `security` role. Under ADR 0062 the
flag is a host fact, a textfile served by 0683's exporter and alarmed on the monitoring VM.

## Context

Task 0227 wired `unattended-upgrades` to install security patches automatically but **not**
reboot the box (`Automatic-Reboot "false"`, the operator's preference for planned maintenance
windows). update-notifier-common's hook (`/usr/share/update-notifier/notify-reboot-required`,
run from package postinsts and `/etc/kernel/postinst.d/update-notifier`) still creates
`/var/run/reboot-required`, and appends to its `.pkgs` companion, whenever an installed update
needs a restart to take effect.

Without an alert:

- Pending kernel, libc and openssl fixes sit unapplied between operator SSH sessions.
- The MOTD banner (`*** System restart required ***`) only surfaces at interactive login, easy to
  miss in remote-only workflows.
- Security patches lose half their value if the live process keeps running the old binary for
  weeks.

## Implementation

- **The writer.** The `security` role installs `update-notifier-common` (a no-op where the image
  has it): no role does today, and the touch drill can't show that anything writes the flag.
- **The fact.** 0683's host timer writes `reboot_required` (1 when the file exists, 0 otherwise)
  and `reboot_required_packages`, the line count of `/var/run/reboot-required.pkgs`. The package
  names stay on the box, not in a label.
- **The rule.** `explorer-reboot-required` in the explorer's rules file (0681):
  - expression `reboot_required{project="explorer"} == 1`, labels `severity="warning"` and `shape="level"`;
  - the description names `cat /var/run/reboot-required.pkgs` and the planned-reboot procedure.
- **Why no cooldown.** The shared route repeats a standing `warning` every 24 hours (accepted for
  the explorer in 0681, Step 1), which is the once-a-day reminder this task wanted, with no
  sentinel file. The alert resolves after the
  reboot removes the file.
- **Silence.** If the timer stops, 0683's `explorer-textfile-stale` fires, so a stale 0 can't
  read as "no reboot needed".

## Acceptance Criteria

- [ ] Touching `/var/run/reboot-required` on the box fires `explorer-reboot-required` into the
      channel within minutes, and removing it resolves the alert (drilled once in a quiet window).
- [ ] `update-notifier-common` is installed by the `security` role.
- [ ] Re-running the playbook with no change is a no-op.
- [ ] **API types regenerated** — N/A — this task does not touch `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** — `infra-hetzner/README.md` operating-model section: what the alert means
      and how to acknowledge it (a planned reboot).

## Out of Scope

- Automatically performing the reboot from the alert path: the whole point of 0227's
  manual-reboot stance is an operator in the loop.
- A webhook, mail relay or cron of its own: ADR 0062 replaces them with the textfile and the rule.
