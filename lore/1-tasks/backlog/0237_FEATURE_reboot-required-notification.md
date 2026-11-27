---
id: '0237'
title: 'FEATURE: reboot-required alert for ch-prod-01'
type: FEATURE
status: backlog
related_adr: ['0062']
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
  - date: '2026-11-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Redesigned under ADR 0062 (proposed, PR #762): the reboot flag becomes a check in x402's
      host probe and a rule beside `x402-disk-root` on the monitoring VM, instead of a cron with
      its own Slack webhook or mail relay and a cooldown file. Title changed from "notification
      (Slack / email)". The context from 0227 is unchanged. (The first draft of #763 put it on
      an explorer textfile exporter, task 0683, dropped with the ADR's slimming.)
---

# FEATURE: reboot-required alert for ch-prod-01

## Summary

Alert when `/var/run/reboot-required` exists on ch-prod-01: the box has security updates
installed that need a kernel, libc or systemd restart. This closes the visibility gap created by
disabling `unattended-upgrades`' automatic reboot in 0227's `security` role. Under ADR 0062 the
flag is a fact about the shared host, so x402's host probe reads it and x402's rules alarm on it,
beside `x402-disk-root` and `x402-memory`.

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

Two repositories: this one keeps what writes the flag, stellar-x402 reads it and alarms.

- **The writer (here).** The `security` role installs `update-notifier-common` (a no-op where the
  image has it): no role does today, and the touch drill can't show that anything writes the flag.
- **The fact (stellar-x402, `deploy/libexec/x402-host-probe`).** A check that adds
  `reboot_required` (1 when `/var/run/reboot-required` exists, 0 otherwise) to the probe's
  textfile, with the probe's test and the metrics catalog updated. The package names stay on the
  box (`/var/run/reboot-required.pkgs`), not in a label.
- **The rule (stellar-x402, `deploy/metrics/rules/x402-production.yml`).** `x402-reboot-required`:
  - expression `max(reboot_required{project="x402"}) == 1`, labels `check="reboot-required"`,
    `severity="warning"` and `shape="level"`;
  - the description says the box is shared with the explorer and ClickHouse, names
    `cat /var/run/reboot-required.pkgs` and the planned-reboot procedure;
  - re-pinned in soroban-infrastructure (`x402_rules_commit`, `x402_rules_sha256`).
- **Why no cooldown.** The shared route repeats a standing `warning` every 24 hours, which is the
  once-a-day reminder this task wanted, with no sentinel file. The alert resolves after the reboot
  removes the file.
- **Silence.** If the probe stops, `x402-textfile-stale` fires, so a stale 0 can't read as "no
  reboot needed".

## Acceptance Criteria

- [ ] Touching `/var/run/reboot-required` on the box fires `x402-reboot-required` into
      `#stellar-x402-bot` within minutes, and removing it resolves the alert (drilled once in a
      quiet window; ADR 0062, rule 5).
- [ ] x402's host probe test covers both values of `reboot_required`.
- [ ] `update-notifier-common` is installed by the `security` role.
- [ ] Re-running the playbook with no change is a no-op.
- [ ] **API types regenerated** — N/A — this task does not touch `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`.
- [ ] **Docs updated** — `infra-hetzner/README.md` operating-model section: what the alert means
      and how to acknowledge it (a planned reboot); stellar-x402 `docs/monitoring.md` gets the
      rule's section.

## Out of Scope

- Automatically performing the reboot from the alert path: the whole point of 0227's
  manual-reboot stance is an operator in the loop.
- A webhook, mail relay or cron of its own: ADR 0062 replaces them with the probe check and the
  rule.
