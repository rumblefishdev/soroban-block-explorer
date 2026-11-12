---
id: '0668'
title: 'Give the Robot firewall rules an ip_version'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0665']
tags: [infra-hetzner, ansible, priority-medium, effort-small]
links: []
history:
  - date: '2026-11-06'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0665. The hetzner play failed on the firewall task and
      stopped the playbook before app.
---

# Give the Robot firewall rules an ip_version

## Summary

`infra-hetzner/ansible/roles/hetzner/tasks/main.yml` → "Apply Robot stateless
firewall rules" fails with "missing parameter(s) required by 'protocol':
ip_version found in rules -> input". `community.hrobot.firewall` now requires
`ip_version` on every rule that sets `protocol`, and none of the five rules in
`hetzner_firewall_rules` (`group_vars/all.yml`) has one.

## Context

- 2026-11-06 (0665): `--tags app,hetzner` set reverse DNS, then failed here.
  The play's only host failed, so Ansible aborted before the ch-prod-01 play.
- The module fails its parameter check before calling the Robot API, so the
  firewall on the server is unchanged.
- Any full run and any `--tags hetzner` run hits it.

## Implementation

- Read the live rules first (Robot UI → Firewall, or
  `community.hrobot.firewall_info`) and note whether they are IPv4 only.
- Add `ip_version: 'ipv4'` to each rule in `hetzner_firewall_rules`, or
  `ipv6` rules as well if the server needs them, so the result matches what
  is live.
- `ansible-playbook … --tags hetzner --check --diff`, then the real run.

## Acceptance Criteria

- [ ] `--tags hetzner` finishes, and the firewall matches the rules it had
      before.
- [ ] **Docs updated** — `infra-hetzner/README.md` if it lists the rules.
- [ ] **API types regenerated** — N/A.
