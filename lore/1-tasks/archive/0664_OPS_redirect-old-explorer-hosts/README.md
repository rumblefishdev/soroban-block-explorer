---
id: '0664'
title: 'Redirect the old explorer hosts to sorobanscan.com'
type: OPS
status: completed
related_adr: ['0048']
related_tasks: ['0659', '0662', '0663', '0660']
tags: [infra, cloudfront, cloudflare, seo, priority-medium, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/711'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/714'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/718'
  - 'https://github.com/rumblefishdev/dns-cloudformation/pull/6'
  - 'https://github.com/rumblefishdev/dns-cloudformation/pull/7'
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659. Needs 0662 and 0663.'
  - date: '2026-11-05'
    status: active
    who: stkrolikiewicz
    note: >
      Started after 0663 shipped. Decided: the old SPA hosts redirect from the
      same distribution (a host check in its viewer-request functions), not a
      second one; api.sorobanscan.rumblefish.dev stays dark.
  - date: '2026-11-06'
    status: completed
    who: stkrolikiewicz
    note: >
      Every old host redirects: the SPA hosts 301 from CloudFront, the API hosts
      308 from the rumblefishdev.com zone, the portal too since the Prices API
      moved to prices-api.sorobanscan.com (stellar-prices-api 0340, flag off on
      2026-11-06). Old API domains, certificates and edge rules removed; Search
      Console change of address and GA4 done.
---

# Redirect the old explorer hosts to sorobanscan.com

## Summary

The old explorer hosts keep answering, as redirects to the new ones, path and
query kept: `301` for the SPA hosts, `308` for the API hosts (a `301` lets a
client turn a POST into a GET, and `/auth/session` is a POST). Then the old API
custom domains go, and Search Console gets the change of address.

## Stan teraz

- Done (2026-11-06): every old explorer host redirects, path and query kept.
  The SPA hosts answer `301` from CloudFront (a host check in every
  viewer-request function), the API hosts `308` from the `rumblefishdev.com`
  zone. Since stellar-prices-api 0340's switch, the portal follows too
  (`pricesPortalOnLegacyHost: false`).
- Done: the old API domains, their certificates, records and edge rules are
  gone or moved to dns-cloudformation. Search Console change of address and
  GA4 are done.
- Left outside this task: the GA4 unwanted-referral rule for
  `sorobanscan.rumblefish.dev` can go (Aga). The unused certificate
  `67947579…` stays until someone deletes it (0659).
- Detail, decisions and issues: [notes/R-how-the-redirects-shipped.md](notes/R-how-the-redirects-shipped.md).

## Hosts

| Old                                         | New                           | Code |
| ------------------------------------------- | ----------------------------- | ---- |
| `sorobanscan.rumblefish.dev`                | `sorobanscan.com`             | 301  |
| `testnet.sorobanscan.rumblefish.dev`        | `testnet.sorobanscan.com`     | 301  |
| `api-sorobanscan.rumblefishdev.com`         | `api.sorobanscan.com`         | 308  |
| `api-testnet-sorobanscan.rumblefishdev.com` | `api-testnet.sorobanscan.com` | 308  |

`api.sorobanscan.rumblefish.dev` has been dark since
`enableLegacyApiDomain: false`. Decide: leave it dark or add a 308.

## Acceptance Criteria

- [x] `curl -sI` on every old host answers its code with `Location` on the new
      host, path and query kept (2026-11-05, both networks), the portal's
      paths excepted while `pricesPortalOnLegacyHost` is set.
- [x] `https://sorobanscan.rumblefish.dev/prices-api/dashboard?x=1` → one hop
      to `https://sorobanscan.com/api/prices-api/dashboard?x=1` (2026-11-06).
- [x] The old API custom domains are removed (2026-11-05, both networks).
- [x] Search Console change of address submitted (2026-11-05, from
      `https://sorobanscan.rumblefish.dev/`).
- [x] Portal sign-in works from `sorobanscan.com/api/prices-api/` (from 0663;
      2026-11-06, by a person).
- [x] GA4 real-time shows hits from `sorobanscan.com` (from 0663; 2026-11-05).
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`,
      `docs/deployment.md`, `infra/cloudflare/README.md` (removed with its
      directory), `docs/architecture/backend/api-gateway-cache-spec.md`.
- [x] **API types regenerated** — N/A: nothing under `crates/api/**`.

## Design Decisions

From plan: (1) the same distribution, with a host check in its functions, not
a second one; (2) `308` for the API hosts from a Single Redirect.
Emerged: (3) `pricesPortalOnLegacyHost`, a temporary `302` back until the
Prices API was same-site; (4) the old API records moved to dns-cloudformation
and their edge rules dropped; (5) the change of address from the URL-prefix
property, never `rumblefish.dev`; (6) the flag's Delivery from a laptop.
Reasons in the note.

## Issues Encountered

The `-5-Delivery` smoke failed on the old host's new `301` (fixed in #713), and
CloudFormation waited 14 minutes while the functions were already live. Both
are in the note.
