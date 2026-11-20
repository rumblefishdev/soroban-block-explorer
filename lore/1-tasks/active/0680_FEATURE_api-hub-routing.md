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

**Step 4, the viewer's own headers (2026-11-20).** AWS's guide "Add custom headers to origin requests"
says an origin custom header is overwritten onto a viewer's header of the same name ("If a header is
present, CloudFront overwrites the header value before forwarding the request to the origin").
Neither "Add CloudFront request headers" nor "Control origin requests with a policy" says what
CloudFront does with a viewer-supplied `CloudFront-Viewer-Address`, so that was not confirmed.
`/api/hub/*` therefore has a viewer-request function
(`cloudfront-functions/api-hub-backend-headers.ts`) deleting `X-Hub-Origin-Secret`,
`X-Hub-Origin-Revision` and `CloudFront-Viewer-Address` from the viewer's request. Sources:
https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/add-origin-custom-headers.html
https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/using-cloudfront-headers.html
