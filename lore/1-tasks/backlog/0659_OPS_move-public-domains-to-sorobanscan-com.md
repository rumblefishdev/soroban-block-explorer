---
id: '0659'
title: 'Move the public domains to sorobanscan.com (umbrella)'
type: OPS
status: backlog
related_adr: ['0048']
related_tasks: ['0660', '0661', '0662', '0663', '0664', '0665', '0631', '0658']
tags: [infra, cloudflare, domains, umbrella, priority-medium, effort-large]
links: []
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: >
      Created after the team agreed the names on Slack (conference feedback):
      every public host moves to sorobanscan.com, the product portals gather
      under /api/, old hosts keep answering with redirects, ClickHouse moves
      too. One step at a time, each its own task.
---

# Move the public domains to sorobanscan.com (umbrella)

## Summary

Every public host of the explorer and the Prices API moves from
`*.sorobanscan.rumblefish.dev` (Route 53) and `*-sorobanscan.rumblefishdev.com`
(Cloudflare) to `sorobanscan.com`, a Cloudflare zone owned by
soroban-infrastructure (`cloudflare/zone.tf`). The product portals gather under
`/api/`. Old hosts keep answering with redirects, so no shared link breaks.
This task holds the names and the order; each step is its own task.

## Stan teraz

- Done: names and order agreed (2026-11-03); 0631 and 0660 shipped
  (2026-11-04; Prices Compute for 0660 rides with stellar-prices-api 0242).
- Done (2026-11-05): 0661, 0662 and 0663. The explorer SPA and API serve on
  `sorobanscan.com`, `testnet.sorobanscan.com`, `api.sorobanscan.com` and
  `api-testnet.sorobanscan.com`, beside the old hosts. The Turnstile widget
  moved to soroban-infrastructure.
- Done (2026-11-05/06): 0664. Every old explorer host redirects (`301` SPA,
  `308` API), and the old API domains, certificates and edge rules are gone.
  Search Console change of address and GA4 are done.
- Done (2026-11-06): step 6 up to the switch, stellar-prices-api task 0340.
  The Prices API answers on `prices-api.sorobanscan.com` (proxied), and the
  portal runs on `sorobanscan.com/api/prices-api/` with Discord sign-in. The
  old Prices API host still serves API-key clients.
- Next: 0340 step 5 (Prices docs, skill and examples, then the old API host).
  Then 0665. Optional: delete the unused certificate
  `api.sorobanscan.rumblefishdev.com` (`67947579…`, 0664).
- In force: one step at a time; each ships and is verified before the next.

## Names

|               | production                        | testnet                       |
| ------------- | --------------------------------- | ----------------------------- |
| Explorer SPA  | `sorobanscan.com`                 | `testnet.sorobanscan.com`     |
| Explorer API  | `api.sorobanscan.com`             | `api-testnet.sorobanscan.com` |
| Prices API    | `prices-api.sorobanscan.com`      | —                             |
| Prices portal | `sorobanscan.com/api/prices-api/` | —                             |
| ClickHouse    | `ch.sorobanscan.com`              | —                             |

Later portals: `/api/x402-facilitator/`, `/api/sorobanscan/`.

Proxied hosts must sit one label under the zone: the Free plan's Universal SSL
covers `sorobanscan.com` and `*.sorobanscan.com` only. Hence
`api-testnet.sorobanscan.com`, never `api.testnet.sorobanscan.com`.

## Order

1. **0660**: portal hub `/api/` and the Prices portal at `/api/prices-api/`, on
   the current domain. 0631 first if it can be; it is not a blocker.
2. **0661**: ACM wildcards, the API's edge rules and Turnstile hostnames for
   the new zone.
3. **0662**: explorer API on `api.sorobanscan.com`, beside the old host.
4. **0663**: SPA on `sorobanscan.com`; the Prices `portalWebOrigin` moves in
   the same window.
5. **0664**: old explorer hosts answer 301 (SPA) / 308 (API); Search Console
   change of address.
6. **Prices API on `prices-api.sorobanscan.com`**: stellar-prices-api side,
   its task 0340.
   Follow its `docs/runbooks/portal-oauth-deploy-prep.md` §6 (Discord redirect
   URI order). Its old host redirects only after the stored `redirect_uri`
   points at the new one.
7. **0665**: ClickHouse on `ch.sorobanscan.com`.
8. Later: `/api/x402-facilitator/` (no portal or keys exist in stellar-x402
   yet) and `/api/sorobanscan/`.

## Acceptance Criteria

- [ ] Every step task above is archived (completed, or canceled with a reason).
- [ ] Every old public URL answers a redirect to its new host, path and query
      kept.
- [ ] **Docs updated** — each step updates the docs it changes (listed in each
      task).
- [ ] **API types regenerated** — N/A: tracked per step.
