---
id: '0680'
title: 'Hand /api/ to the API hub, by a setting'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0670', '0664', '0608']
tags: [layer-infra, priority-high, effort-medium]
links: []
history:
  - date: 2026-11-20
    status: active
    who: adamkoot
    note: 'Task created from the public-launch design (the cutover of the shared sign-in design).'
---

# Hand /api/ to the API hub, by a setting

## Summary

The explorer's CloudFront distribution hands `sorobanscan.com/api/` to the separate API hub app
(`sorobanscan-api-portal`), by a setting in `infra/envs/production.json`, in two stages: the
hub's backend under `/api/hub/*` first, then the pages. Without the settings the stack builds
what it builds today, so the change merges with no effect on production, and testnet never
changes. Turning the pages' switch off is the way back.

Spec (in the `sorobanscan-api-portal` repo): `docs/superpowers/specs/2026-11-20-public-launch-design.md`,
section "The explorer's distribution".

## Stan teraz

- Done: task created, branch `feat/0680_api-hub-routing`.
- Next: the hub's routing function (`api-hub-routing.ts`), then the behaviours, settings and
  tests that attach it.
- In force: one CloudFront Function for `/api/*` and the two Prices behaviours; the old host's
  `301` first, then by the first segment after `/api/`.

## Context

Today `/api/` is the explorer's own `ApiHubPage`, `/api/prices-api/*` and
`/api/x402-facilitator/*` are separate portals, and every other path under `/api/` is redirected
by `api-hub-redirect.ts`. The hub replaces them with one app: one sign-in, one dashboard, a tab
per product.

## Implementation Plan

### Step 1: The hub's routing function

`infra/src/lib/cloudfront-functions/api-hub-routing.ts`, with its tests in `__tests__/`.

### Step 2: Behaviours and settings

The `/api/hub/*` behaviour (backend, secret header), `/api/assets/*`, `/api/prices-api{,/*}`,
`/api/*` and the exact `/api` redirect, armed by the settings in two stages; the x402 portal's
two behaviours and function go once the pages are on.

### Step 3: Tests and docs

Each row of the spec's three tables that the distribution answers, with both stages on; the
backend alone with the first; today's answers with neither.

## Acceptance Criteria

- [ ] With neither setting, the stack's template is unchanged
- [ ] With the backend's values alone, `/api/hub/*` is added and nothing else changes
- [ ] With both stages on, the behaviours and the routing function answer as the spec's tables say
- [ ] **Docs updated** — `docs/architecture/**` describes the distribution's `/api` behaviours,
      or `N/A — reason`.
- [ ] **API types regenerated** — N/A — no change under `crates/api/**`, `Cargo.{toml,lock}` or
      `libs/api-types/**`.

## Notes

Spec: see Summary. The routing function consumes `HostMove`, `legacyHostSnippet` and `MOVED_JS`
from `legacy-host-redirect.ts`.

**Step 4, the viewer's own headers (2026-11-20).** AWS's guide "Add custom headers to origin
requests" says an origin custom header replaces a viewer's header of the same name ("If a header is
present, CloudFront overwrites the header value before forwarding the request to the origin"). The
guides on CloudFront's request headers don't say the same of `CloudFront-Viewer-Address`, so the
client's address travels in `X-Hub-Viewer-Ip`, written by the viewer-request function on
`/api/hub/*` (`cloudfront-functions/api-hub-backend-headers.ts`) from `event.viewer.ip`, as AWS's
own example function writes `True-Client-IP`; the function also deletes a viewer's own
`X-Hub-Origin-Secret` and `X-Hub-Origin-Revision`. Sources:
https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/add-origin-custom-headers.html
https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/using-cloudfront-headers.html

**The pages, synthesized by hand (2026-11-20).** Production's Delivery stack, three times, by the
`cdk` CLI with `infra/cdk.json`'s context and a made-up `backendHost`
(`made-up-backend.lambda-url.eu-central-1.on.aws`), each compared with `origin/develop`'s
(72032230f) by `cdk diff --strict --template <develop's template>`:

- _Neither:_ "There were no differences". The JSON equals develop's once its keys are sorted;
  the file lists some resources in another order, since today's `/api` behaviours are now built
  in `api-portal-behaviors.ts`. Testnet the same.
- _The backend_ (`pages: false`): one origin added (the function URL, HTTPS only, with
  `X-Hub-Origin-Secret` as `{{resolve:secretsmanager:…:SecretString:current::}}` and
  `X-Hub-Origin-Revision`), one origin request policy, one behaviour (`/api/hub/*`, before
  `/api/*`) and its viewer-request function (`api-hub-backend-headers`, step 4's), and the CDK's
  own usage record (`AWS::CDK::Metadata`) with them. Nothing else. The same JSON as task 2's
  backend-only template (abb163bd1), so turning the pages off again gives that one back.
- _Both_ (`pages: true`): under `/api`, `/api/assets/*` (the bucket, `CachingOptimized`, the
  main behaviours' function, so the old host still answers `301`), `/api/prices-api` and
  `/api/prices-api/*` (the hub's function, a policy `…-headers-noindex` with
  `X-Robots-Tag: noindex` and the shared security headers), `/api/hub/*` (as above), `/api` and
  `/api/*` (the hub's function, short TTL); `/api/x402-facilitator`, `/api/x402-facilitator/*`
  and `/api/` gone. Functions `api-spa-routing`, `x402-portal-routing` and `api-hub-redirect`
  destroyed, `api-hub-routing` added. Outside `/api` (the default behaviour, `/assets/*`,
  `/static/*`, `/prices-api*`, `/pricing-api*`, the distribution's other settings and their
  origins) and every other resource: as develop's. The bucket's behaviours are listed before the
  backend's, so the bucket stays the distribution's second origin and keeps its origin access
  control when the pages are turned on or off.
