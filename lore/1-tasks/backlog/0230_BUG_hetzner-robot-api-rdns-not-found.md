---
id: '0230'
title: 'BUG: community.hrobot Robot API returns "IP not found" for auction server'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0227']
tags:
  [
    priority-medium,
    effort-small,
    layer-infrastructure,
    ansible,
    hetzner-robot,
    bug,
  ]
links: []
history:
  - date: '2026-05-19'
    status: backlog
    who: fmazur
    note: 'Spawned from 0227 first-deploy debugging — Robot API rejects rDNS / firewall updates for auction server #<auction-id> (<box-ipv4>) with "IP not found" despite the IP being clearly owned and rDNS already set by Hetzner default. Initial deploy worked around by `--skip-tags hetzner`.'
---

# BUG: community.hrobot Robot API returns "IP not found" for auction server

## Summary

The `community.hrobot.reverse_dns` (and likely `community.hrobot.firewall`)
module call against server `#<auction-id>` / IPv4 `<box-ipv4>` fails
with HTTP-level "The IP address was not found" error during
`ansible-playbook` runs. The IP is unambiguously owned by the
account, visible in Robot UI, and already has a default rDNS
entry (`static.<reverse-ipv4>.clients.your-server.de`). Initial
deploy of task 0227 worked around the failure by invoking
`--skip-tags hetzner`, leaving the Hetzner Robot side without
IaC ownership.

## Context

Task 0227 delivered the `hetzner` Ansible role that uses
`community.hrobot.*` modules to declaratively configure:

- rDNS pointer for the box's IPv4
- Display label in Robot UI (optional, requires
  `HETZNER_SERVER_NUMBER` env)
- Stateless switch-level firewall rules

All three operations target the auction-purchased dedicated
server `Server Auction #<auction-id>` in FSN1-DC15.

During first deploy validation the `reverse_dns` task failed
with `Module failed: The IP address was not found`. Manual
inspection in Robot UI confirms:

- The IP is listed under the server
- Default rDNS is set (`static.<reverse-ipv4>.clients.your-server.de`)
- UI shows the rDNS field as editable

The webservice user (`#ws+...`) authenticates successfully (the
preceding preflight assertions pass), so credentials are OK.

## Hypotheses to investigate

1. **Webservice user permissions** — Robot UI may have per-feature
   permission toggles (e.g. "Reverse DNS management", "Firewall
   management", "Server management") that need to be explicitly
   enabled for the webservice user. Check Robot UI → Settings →
   Webservice and app settings → access tab.

2. **Auction-server API quirk** — Hetzner Server Auction servers
   sometimes require an initial UI-side configuration step
   before the Robot API recognises them as "managed" in the
   rDNS / firewall namespace. Try manually changing the rDNS
   value once via UI (any custom value), saving, then re-running
   the playbook.

3. **Module URL/method mismatch** — Possible (less likely)
   that `community.hrobot` 1.9.x has a bug for certain server
   types where it queries an endpoint that returns 404 instead
   of constructing a POST-create flow. Test by `curl`ing the
   raw Robot API endpoint directly:

   ```bash
   curl -u "$HCLOUD_ROBOT_USER:$HCLOUD_ROBOT_PASSWORD" \
        https://robot-ws.your-server.de/rdns/<box-ipv4>
   ```

   Compare behaviour to module's GET/POST flow.

4. **`community.hrobot` version pin** — currently pinned to
   `>=1.9.0,<2.0.0` in `requirements.yml`. Newer 2.x may handle
   auction servers / 404 cases differently. Possible upgrade path.

## Scope

- Reproduce the error against the live setup.
- Diagnose root cause from the four hypotheses above.
- Apply the fix:
  - If webservice permissions: document the required toggles
    in `infra-hetzner/README.md` operating model section.
  - If API quirk: document the manual one-time UI step in the
    runbook prerequisites.
  - If module bug: file upstream issue + workaround locally
    (e.g. shell out to `curl` against Robot API directly).
- Remove the `--skip-tags hetzner` workaround from the
  validation deploy procedure.

## Acceptance Criteria

- [ ] Root cause identified and documented in
      `infra-hetzner/README.md` or this task's notes
- [ ] `ansible-playbook -i inventory.ini site.yml` (without
      `--skip-tags hetzner`) completes successfully
- [ ] rDNS entry `ch-prod-01` IP → `ch-prod.sorobanscan.rumblefish.dev`
      (once task 0229 lands real DNS; until then any test value)
- [ ] Switch-level firewall rules visible in Robot UI matching
      `hetzner_firewall_rules` in `group_vars/all.yml`
- [ ] **API types regenerated** — N/A — this task does not touch
      `crates/api/**`, `Cargo.{toml,lock}`, or `libs/api-types/**`
- [ ] **Docs updated** — `infra-hetzner/README.md` prerequisites
      include any newly-required UI-side configuration step

## Out of Scope

- Migrating away from `community.hrobot` to a different
  automation approach
- Auto-recovery / retry logic in the playbook for transient
  Robot API failures (the API is reliable enough; one-time
  setup ought to suffice)

## Workaround currently in place

Initial deploy of 0227 used:

```bash
ansible-playbook -i inventory.ini site.yml \
    -e force_dirty=true \
    --skip-tags hetzner
```

This leaves the Hetzner Robot side without IaC ownership:

- rDNS unset (stays at Hetzner default — cosmetic)
- Server name unchanged (already `soroban-explorer-ch-prod`,
  fine)
- **Switch-level firewall not applied** — ufw on host remains
  the only firewall layer

The host-level ufw rules (allow 22/80/443) are sufficient for
operation. Defence-in-depth (switch + host) is the long-term
goal that this task delivers.
