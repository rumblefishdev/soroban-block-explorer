---
id: '0663'
title: 'Serve the explorer SPA at sorobanscan.com and testnet.sorobanscan.com'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0659', '0661', '0662', '0664', '0660']
tags: [infra, cdk, cloudfront, web, priority-medium, effort-medium]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/704'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/707'
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/6'
  - 'https://github.com/rumblefishdev/soroban-infrastructure/pull/7'
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659. Needs 0661 and 0662.'
  - date: '2026-11-04'
    status: active
    who: stkrolikiewicz
    note: >
      Started: Steps 1-2 and 0662's Step 3 in code, with the old SPA host kept
      as a second alias (legacyDomainName) and the API's CORS open to both.
  - date: '2026-11-05'
    status: completed
    who: stkrolikiewicz
    note: >
      sorobanscan.com and testnet.sorobanscan.com serve the SPA with live data,
      checked in a browser on both networks. Testnet from a laptop, production
      by release #707 (tags -1-Compute, -2-ApiGateway, -3-Delivery, -4-web).
      Portal sign-in from the new host and the GA4 check move to 0664.
---

# Serve the explorer SPA at sorobanscan.com and testnet.sorobanscan.com

## Summary

Serve the explorer SPA at `sorobanscan.com` and `testnet.sorobanscan.com`.
Their DNS is Cloudflare, not Route 53, so the delivery stack must stop creating
Route 53 records for the SPA: a JSON-only change fails at synth.

## Stan teraz

- Done (2026-11-05): Steps 1-3 on both networks. `legacyDomainName` keeps
  the old host as a second alias with its Route 53 records; `domainName`
  (`sorobanscan.com`) gets none. `CORS_ALLOW_ORIGIN` takes a comma list. The
  records are DNS-only CNAMEs in soroban-infrastructure (#7):
  `sorobanscan.com` → `d34voyfq3p0u6o.cloudfront.net`,
  `testnet.sorobanscan.com` → `d2gnhtk4ij3myu.cloudfront.net`.
- Moved to 0664: Step 4 (Prices `portalWebOrigin`, GA4 stream URL, smoke URL).
- In force: the smoke URL stays on the old host until 0664 (it still serves).
- Found: the Prices backend allows one CORS origin (`portalWebOrigin`), and the
  OAuth popup posts back to its opener's origin. Moving it to
  `sorobanscan.com` while the old host still serves the portal breaks the
  portal there, so it moves with 0664's redirect, or the backend takes both.

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

- [x] `https://sorobanscan.com` and `https://testnet.sorobanscan.com` → 200
      `text/html`; pages load data with no CORS errors. Preflight and `GET`
      answer the right `Access-Control-Allow-Origin` for both origins on both
      networks; both home pages loaded live data in a browser.
- [ ] Portal sign-in works from `sorobanscan.com/api/prices-api/` (moved to
      0664: the Prices backend allows one origin).
- [ ] GA4 real-time shows hits from the new hostname (moved to 0664, with the
      stream URL).
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (frontend hostname, DNS owner), `docs/deployment.md`.
- [x] **API types regenerated** — N/A: `crates/api/**` changed only in how
      `CORS_ALLOW_ORIGIN` is read; CI's API types freshness check passed.

## Design Decisions

### From Plan

1. **Both hosts on one distribution** until 0664: the old one as a second alias,
   the API's CORS open to both origins.

### Emerged

2. **Testnet first, from a laptop**: Compute, ApiGateway and Delivery, then its
   record and `deploy-testnet-web`, all checked before production.
3. **Production released from `release/0663`** (`d6267dcd`), not `develop`:
   `develop` took #696 and #705 while the release was open, and neither had
   been on testnet. #706 was closed for #707.
4. **Compute carried #693** (0553, the enrichment runner's report counts),
   parked since #701. Its worker change writes nothing new.
5. **One tag at a time**: `deploy-production` has one concurrency group, which
   keeps only one pending run, so a third tag pushed at once would cancel one.
6. **Records behind variables** in soroban-infrastructure
   (`explorer_spa_origin_<env>`), set only after that network's Delivery deploy
   added the alias, like the API's records.
7. **`-web` after the record**: the new bundle's network menu points at the new
   hosts, so it shipped once they resolved.

## Issues Encountered

- **CloudFront took about 20 minutes** to deploy the alias and certificate change
  (testnet and production alike). The old hosts answered 200 throughout.
- **A fresh record looked missing locally for a while**: the laptop's resolver
  had cached the name's absence. `dig @1.1.1.1` and `curl --resolve` checked it
  meanwhile.
- **An automated browser gets Turnstile's interactive challenge** (managed
  mode), so the data check needs a person's browser.
