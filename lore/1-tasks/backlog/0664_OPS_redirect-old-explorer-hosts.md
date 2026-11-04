---
id: '0664'
title: 'Redirect the old explorer hosts to sorobanscan.com'
type: OPS
status: backlog
related_adr: ['0048']
related_tasks: ['0659', '0662', '0663', '0660']
tags: [infra, cloudfront, cloudflare, seo, priority-medium, effort-medium]
links: []
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659. Needs 0662 and 0663.'
---

# Redirect the old explorer hosts to sorobanscan.com

## Summary

The old explorer hosts keep answering, as redirects to the new ones, path and
query kept: `301` for the SPA hosts, `308` for the API hosts (a `301` lets a
client turn a POST into a GET, and `/auth/session` is a POST). Then the old API
custom domains go, and Search Console gets the change of address.

## Stan teraz

- Done: nothing yet.
- Next: decide how the Route 53 names redirect (see Step 1).
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

Either a host check in the main distribution's viewer-request functions (one
function per behavior, so each needs it), or a small redirect-only
distribution. Moving an alias between distributions means a short gap or
`aws cloudfront associate-alias`. Decide with 0663.

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
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`,
      `docs/deployment.md`, `infra/cloudflare/README.md` (or its removal).
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`.
