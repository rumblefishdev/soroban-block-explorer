---
id: '0663'
title: 'Serve the explorer SPA at sorobanscan.com and testnet.sorobanscan.com'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0659', '0661', '0662', '0664', '0660']
tags: [infra, cdk, cloudfront, web, priority-medium, effort-medium]
links: []
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659. Needs 0661 and 0662.'
---

# Serve the explorer SPA at sorobanscan.com and testnet.sorobanscan.com

## Summary

Serve the explorer SPA at `sorobanscan.com` and `testnet.sorobanscan.com`.
Their DNS is Cloudflare, not Route 53, so the delivery stack must stop creating
Route 53 records for the SPA: a JSON-only change fails at synth.

## Stan teraz

- Done: nothing yet.
- Next: the CDK flag that skips the SPA's Route 53 records.
- In force: the Prices `portalWebOrigin` moves in the same window.

## Context

- `delivery-stack.ts` gives the distribution `domainNames: [config.domainName]`
  and adds A/AAAA records in `hostedZone*`. `relativeRecordName` (`types.ts`)
  throws when `domainName` is outside `hostedZoneName`.
- `hostedZone*` stay: `HetznerDnsStack` uses them for ClickHouse until 0665.
- `domainName` also feeds API Gateway `allowOrigins` and the Lambda's
  `CORS_ALLOW_ORIGIN`, so the API's CORS moves in the same deploy.
- The Prices portal is served by this distribution, and the Prices backend's
  CORS names `portalWebOrigin` (stellar-prices-api `infra/envs/production.json`).
- `web/src/network.ts` hardcodes both hosts for the network menu.
- Analytics: GTM `GTM-TBF2GP5S` in `web/index.html`. Neither GTM nor GA4 is
  bound to a domain, so the history stays. `_ga` and the consent cookie are per
  domain: returning visitors count as new once, and the consent banner shows
  again.
- The old alias stays on the distribution beside the new one (decided
  2026-11-04, SAN certificate from 0661) until 0664 redirects it. The API's
  CORS allows both origins for that window.

## Implementation Plan

### Step 1: CDK

A flag that skips the SPA's Route 53 records when the domain lives in
Cloudflare. `DistributionDomainName` is already an output.

### Step 2: Config

`domainName` and `cloudFrontCertificateArn` (0661) in `production.json` and
`testnet.json`; both hosts in `web/src/network.ts`.

### Step 3: DNS

DNS-only records in soroban-infrastructure: `sorobanscan.com` (apex CNAME
flattening) and `testnet.sorobanscan.com` → their distributions. The SPA keeps
no edge, as today.

### Step 4: Same window

Prices `portalWebOrigin` → `https://sorobanscan.com`. GA4 stream URL → the new
host. Smoke URL in `.github/workflows/deploy-production.yml`.

## Acceptance Criteria

- [ ] `https://sorobanscan.com` and `https://testnet.sorobanscan.com` → 200
      `text/html`; pages load data with no CORS errors.
- [ ] Portal sign-in works from `sorobanscan.com/api/prices-api/`.
- [ ] GA4 real-time shows hits from the new hostname.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (frontend hostname, DNS owner), `docs/deployment.md`.
- [ ] **API types regenerated** — N/A: nothing under `crates/api/**`.
