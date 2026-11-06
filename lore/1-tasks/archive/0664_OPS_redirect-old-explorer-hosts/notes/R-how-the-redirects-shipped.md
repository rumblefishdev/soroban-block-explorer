---
title: 'How the redirects shipped, 2026-11-05/06'
type: research
status: mature
spawns: []
tags: [redirects, cloudfront, cloudflare, prices-portal]
links: []
history:
  - date: '2026-11-06'
    status: mature
    who: stkrolikiewicz
    note: "The task file's state log and plan, moved here when 0664 completed (README limit)."
---

# How the redirects shipped, 2026-11-05/06

The state log as it grew while the task ran, and the plan it followed.

## State log

- Done: Step 1 on both networks, 2026-11-05: testnet from a laptop,
  production by `production-2026.11.05-5-Delivery` (#711, release #712). Every
  viewer-request function on the distribution answers `301` on
  `legacyDomainName`; the main behaviors, which had none, get
  `legacy-host-redirect.ts`. Checked with `curl` on both production hosts.
- Found (2026-11-05): that tag's CI run failed at "Smoke test — Frontend",
  which still fetched the old host and now got its `301`. The deploy itself
  went through. The smoke now fetches `https://sorobanscan.com/` (carried from
  0663).
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
- Carried from 0663 (its Step 4): Prices `portalWebOrigin` waits for the Prices
  API's move (see Found). GA4 checked 2026-11-05: the GTM container fires its
  one tag on every page, with no host condition.
- Done: Step 2, 2026-11-05. dns-cloudformation#6: both old API hosts answer
  `308` from the zone's redirect ruleset. dns-cloudformation#7: their records
  moved there (imported, pointed at the new hosts), and their `X-Edge-Secret`
  rule, managed WAF and rate limit went, since the redirect phase runs before
  them; that state holds no edge secret any more. Here: `infra/cloudflare/`
  removed (its state held only those records) and
  `enableCloudflareApiDomain: false`, which drops the old custom domains.
- Done: the cleanup, 2026-11-05. The old custom domains went on testnet
  (laptop) and production (`production-2026.11.05-6-ApiGateway`, release
  #715, smoke green). Their ACM certificates (`10aeba0f…`, `c96addf8…`, not in
  use) were deleted and their validation records went in
  dns-cloudformation#8. This repo's old Cloudflare state objects were deleted
  from `production-soroban-explorer-cf-tfstate` (versioned; a copy is in
  `~/Backups/explorer-cf-tfstate-2026-11-05/` on stkrolikiewicz's laptop).
- Found (2026-11-05): one more unused certificate in eu-central-1,
  `api.sorobanscan.rumblefishdev.com` (`67947579…`), from before the
  one-label host of task 0277. Not deleted; nothing uses it.
- Done: Step 3, 2026-11-05, by the GA/Search Console owner (Aga). The domain
  property `sorobanscan.com` is verified through a TXT record
  (soroban-infrastructure#8). The URL-prefix property
  `https://sorobanscan.rumblefish.dev/` was added and verified through her
  ownership of `rumblefish.dev`, and the change of address to `sorobanscan.com`
  was submitted from it (301 and both verifications passed). Never from
  `rumblefish.dev` itself, which would move the company site.
- Done: GA4, 2026-11-05 (Aga). The stream URL is `https://sorobanscan.com`,
  `sorobanscan.rumblefish.dev` is on the unwanted-referrals list while the
  portal stays there, and real-time shows the explorer's views, which only the
  new host serves now.
- Done: the portal follows, 2026-11-06 (about 09:00 UTC). With
  stellar-prices-api 0340's switch (its #412: `portalWebOrigin`, `redirect_uri`,
  the bundle on `prices-api.sorobanscan.com`), `pricesPortalOnLegacyHost: false`
  went out: #718, release #719, Delivery from a laptop to keep the window
  short. The old host answers `301` for the portal too. A Discord sign-in
  passed on `https://sorobanscan.com/api/prices-api/`.
- Left outside this task: the GA4 unwanted-referral rule for
  `sorobanscan.rumblefish.dev` can go (Aga). The unused certificate
  `67947579…` stays until someone deletes it (0659).
- Decided (2026-11-05): `api.sorobanscan.rumblefish.dev` stays dark. It has
  had no DNS answer since `enableLegacyApiDomain: false`, so nothing depends on
  it.
- In force: one hop. Old portal paths go straight to `/api/prices-api/…`.

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

## Design Decisions

### From Plan

1. **The old SPA hosts redirect from the same distribution**: a host check in
   every viewer-request function, plus `legacy-host-redirect.ts` for the
   behaviors that had none. No second distribution, so no alias moved.
2. **`308` for the API hosts**, from a Single Redirect in the
   `rumblefishdev.com` zone, so a POST stays a POST.

### Emerged

3. **`pricesPortalOnLegacyHost`, a temporary move the other way (`302`).** The
   portal's session cookie (`SameSite=Lax`) and CORS need page and API
   same-site. Until the Prices API had a `sorobanscan.com` host, the portal had
   to stay on the old one. That also fixed a live regression: since 0663 the
   "Prices API" link on the new host had led to a portal that could not sign
   in.
4. **The old API hosts' records moved to dns-cloudformation**, imported and
   pointed at the new hosts. Their edge rules went, since the redirect phase
   runs before the transform, WAF and rate-limit phases. That state holds no
   edge secret any more, and `infra/cloudflare/` went from this repo.
5. **Search Console's change of address from a URL-prefix property**
   `https://sorobanscan.rumblefish.dev/`, never from the `rumblefish.dev`
   domain property, which would have moved the company site.
6. **Delivery for the flag from a laptop** (release #719), not a tag, so the
   Prices switch window stayed a few minutes instead of a CI run.

## Issues Encountered

- `production-2026.11.05-5-Delivery`'s CI run failed at the frontend smoke,
  which fetched the old host and got its new `301`. The deploy itself went
  through. Fixed in #713.
- CloudFormation kept the production Delivery update in progress for 14
  minutes (842 s on 2026-11-06) while the CloudFront functions were already
  `LIVE` and serving the new behavior. Check the function stage and the hosts,
  not the stack.
