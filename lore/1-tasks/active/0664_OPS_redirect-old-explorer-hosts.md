---
id: '0664'
title: 'Redirect the old explorer hosts to sorobanscan.com'
type: OPS
status: active
related_adr: ['0048']
related_tasks: ['0659', '0662', '0663', '0660']
tags: [infra, cloudfront, cloudflare, seo, priority-medium, effort-medium]
links: []
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
---

# Redirect the old explorer hosts to sorobanscan.com

## Summary

The old explorer hosts keep answering, as redirects to the new ones, path and
query kept: `301` for the SPA hosts, `308` for the API hosts (a `301` lets a
client turn a POST into a GET, and `/auth/session` is a POST). Then the old API
custom domains go, and Search Console gets the change of address.

## Stan teraz

- Done: Step 1 in code, on testnet 2026-11-05 (laptop). Every viewer-request
  function on the distribution answers `301` on `legacyDomainName`; the main
  behaviors, which had none, get `legacy-host-redirect.ts`.
- Found (2026-11-05): the Prices portal cannot work on `sorobanscan.com` while
  the Prices API is on `prices-api.sorobanscan.rumblefish.dev`. Its CORS allows
  one origin, and its session cookie is `SameSite=Lax`, same-site only with
  `*.rumblefish.dev` (stellar-prices-api `portal/auth/cookies.rs`). Moving
  `portalWebOrigin` fixes the first, not the second. Since 0663 the explorer's
  "Prices API" link on the new host led to a portal that could not sign in.
- Decided (2026-11-05): `pricesPortalOnLegacyHost`. Until the Prices API is on
  `prices-api.sorobanscan.com` (0659 step 6), the portal stays on the old host:
  the new host answers `302` back to it, the old host serves it, and
  `portalWebOrigin` stays. Then the flag goes and the portal follows with a
  `301`.
- Carried from 0663 (its Step 4): the GA4 stream URL and the smoke URL in
  `.github/workflows/deploy-production.yml`. Prices `portalWebOrigin` waits for
  the Prices API's move (see Found). GA4 checked 2026-11-05: the GTM container
  fires its one tag on every page, with no host condition.
- Next: Step 1 on production; then Step 2 (dns-cloudformation, which we can
  now apply).
- Decided (2026-11-05): `api.sorobanscan.rumblefish.dev` stays dark. It has
  had no DNS answer since `enableLegacyApiDomain: false`, so nothing depends on
  it.
- In force: one hop. Old portal paths go straight to `/api/prices-api/…`.

## Hosts

| Old                                         | New                           | Code |
| ------------------------------------------- | ----------------------------- | ---- |
| `sorobanscan.rumblefish.dev`                | `sorobanscan.com`             | 301  |
| `testnet.sorobanscan.rumblefish.dev`        | `testnet.sorobanscan.com`     | 301  |
| `api-sorobanscan.rumblefishdev.com`         | `api.sorobanscan.com`         | 308  |
| `api-testnet-sorobanscan.rumblefishdev.com` | `api-testnet.sorobanscan.com` | 308  |

`api.sorobanscan.rumblefish.dev` has been dark since
`enableLegacyApiDomain: false`. Decide: leave it dark or add a 308.

## Implementation Plan

### Step 1: Route 53 names (CloudFront)

A host check in the distribution's viewer-request functions (one function per
behavior, so each carries it). A redirect-only distribution was the other
option; moving an alias between distributions means a short gap or
`aws cloudfront associate-alias`, and Route 53 records repointed.

### Step 2: rumblefishdev.com names (Cloudflare)

A Single Redirect in that zone. Its rulesets belong to dns-cloudformation, so
its owner applies it. After that the old API Gateway custom domain and
`infra/cloudflare/` (its only record is the old host) can go.

### Step 3: Search Console

Add `sorobanscan.com`; Change of Address from `sorobanscan.rumblefish.dev`.
The tool needs the 301s live.

## Acceptance Criteria

- [ ] `curl -sI` on every old host answers its code with `Location` on the new
      host, path and query kept.
- [ ] `https://sorobanscan.rumblefish.dev/prices-api/dashboard?x=1` → one hop
      to `https://sorobanscan.com/api/prices-api/dashboard?x=1`.
- [ ] The old API custom domains are removed.
- [ ] Search Console change of address submitted.
- [ ] Portal sign-in works from `sorobanscan.com/api/prices-api/` (from 0663):
      after the Prices API moves; until then the new host sends the portal to
      the old one.
- [ ] GA4 real-time shows hits from `sorobanscan.com` (from 0663).
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`,
      `docs/deployment.md`, `infra/cloudflare/README.md` (or its removal).
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`.
