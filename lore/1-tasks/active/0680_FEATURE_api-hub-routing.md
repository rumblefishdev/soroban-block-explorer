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

- Done: the function, the behaviours, the `apiHub` setting, their tests and the notes below.
- Next: the pull request, then the deployment in two pull requests (see "The setting").
- In force: `infra/envs/production.json` has no `apiHub`: production builds what it built.

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
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md` describes
      the `apiHub` setting and its `/api` behaviours.
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

## The setting (2026-11-20)

`apiHub` in `EnvironmentConfig` (`infra/src/lib/types.ts`):
`{ backendHost, originSecretName, originSecretRevision, pages }`. Three states:

- **Unset (every environment today).** The stack builds what it built before: the Prices portal,
  the x402 portal, the explorer's `ApiHubPage` at `/api` and `/api/`, and `301`/`302` for the other
  `/api/*` paths (`api-portal-behaviors.ts`). Checked by `cdk diff --strict`: no differences.
- **`pages: false` (the backend).** The same, plus `/api/hub/*`: the hub's Lambda function URL,
  HTTPS only, never cached, every method, cookies and query forwarded, with the secret from
  Secrets Manager as `X-Hub-Origin-Secret` and `originSecretRevision` as `X-Hub-Origin-Revision`.
  A viewer-request function writes the client's address into `X-Hub-Viewer-Ip`. No page changes
  and nothing links to the backend, so it can be tried from AWS first.
- **`pages: true` (the start).** The hub's bundle in the API bucket serves `/api`, `/api/*` and
  `/api/prices-api{,/*}` through `api-hub-routing`; `/api/assets/*` comes from the bucket with the
  long cache; `/api/hub/*` is as above. The x402 portal's two behaviours and function, and the
  explorer's `/api/` route, are gone. `validateConfig` refuses `pages: true` together with
  `pricesPortalOnLegacyHost`.

**Deployment.** The pull request changes nothing on production. `infra/envs/production.json`
changes only at the deployment, in two pull requests, as the hub's runbook has it
(`docs/runbook.md` in `rumblefishdev/sorobanscan-api-portal`): first `apiHub` with
`pages: false`, then, once the backend answers from AWS, `pages: true`. Testnet is never changed.

**The way back.** Set `pages` to `false`: the stack goes back to the backend-only template, which
is the same JSON as task 2's backend-only one, with the old pages and the x402 portal as before.
Removing `apiHub` goes back to today's.

## Left for a clean-up after the start

Not done here, because the way back needs them:

- `ApiHubPage` and its route in the explorer's app (the way back shows the old hub).
- `enableX402PortalBasicAuth`, which gates nothing once `pages` is on.
- The unarmed path: `api-portal-behaviors.ts` and the x402 portal's function, behaviours and
  `api-hub-redirect`.
- The bucket's `api/x402-facilitator/` prefix.
- The KeyValueStore's `auth-token-x402` key.
