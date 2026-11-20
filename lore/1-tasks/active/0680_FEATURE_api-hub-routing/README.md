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

- Done: the function, the behaviours, the `apiHub` setting (the backend's host from SSM), their
  tests and the notes below.
- Next: the pull request, then the deployment in two pull requests.
- In force: `infra/envs/production.json` has no `apiHub`: production builds what it built.

## Context

Today `/api/` is the explorer's own `ApiHubPage`, `/api/prices-api/*` and
`/api/x402-facilitator/*` are separate portals, and every other path under `/api/` is redirected
by `api-hub-redirect.ts`. The hub replaces them with one app: one sign-in, one dashboard, a tab
per product.

## Implementation Plan

1. The hub's routing function (`api-hub-routing.ts`), with its tests.
2. The behaviours and settings, armed in two stages; the x402 portal goes once the pages are on.
3. Tests for each row of the spec's three tables, with both stages on; the backend alone; neither.

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

- [notes/R-viewer-headers.md](notes/R-viewer-headers.md): why the client's address travels in
  `X-Hub-Viewer-Ip` and the viewer's own `X-Hub-Origin-*` headers are dropped (step 4).
- [notes/R-templates-by-hand.md](notes/R-templates-by-hand.md): the Delivery templates of the
  three states, synthesized by hand and compared with develop's.

## The setting (2026-11-20)

`apiHub` in `EnvironmentConfig` (`infra/src/lib/types.ts`):
`{ backendHostParameter, originSecretName, originSecretRevision, pages }`. Three states:

- **Unset (every environment today).** The stack builds what it built before: the Prices portal,
  the x402 portal, the explorer's `ApiHubPage` at `/api` and `/api/`, and `301`/`302` for the other
  `/api/*` paths (`api-portal-behaviors.ts`). Checked by `cdk diff --strict`: no differences.
- **`pages: false` (the backend).** The same, plus `/api/hub/*`: the hub's Lambda function URL
  (its host from SSM, below), HTTPS only, never cached, every method, cookies and query
  forwarded, with the secret from Secrets Manager as `X-Hub-Origin-Secret` and
  `originSecretRevision` as `X-Hub-Origin-Revision`. A viewer-request function writes the
  client's address into `X-Hub-Viewer-Ip`. No page changes and nothing links to the backend, so
  it can be tried from AWS first.
- **`pages: true` (the start).** The hub's bundle in the API bucket serves `/api`, `/api/*` and
  `/api/prices-api{,/*}` through `api-hub-routing`; `/api/assets/*` comes from the bucket with the
  long cache; `/api/hub/*` is as above. The x402 portal's two behaviours and function, and the
  explorer's `/api/` route, are gone. `validateConfig` refuses `pages: true` with
  `pricesPortalOnLegacyHost`.

**Since the review (2026-11-20).** `validateConfig` also refuses an empty secret name or revision,
`apiHub` with `enableOriginSecretLock` (behind Cloudflare the viewer's address is Cloudflare's)
and `pages` with `enableBasicAuth`. CloudFormation reads the secret again on any update of the
distribution, not only on a new revision: rotate it as the hub's runbook says.

**The host from SSM (2026-11-20, approved by adamkoot).** The setting held the function URL's
host, to be written into `production.json`. Both repos are public, the host can't be found
otherwise (wildcard certificate and DNS), and a call straight to it skips CloudFront and uses up
the backend's 20 reserved concurrency. So `backendHostParameter` names the SSM String parameter
the hub's own stack writes (`/sorobanscan-hub/production/backend-host`, same account and region),
read as a template parameter of type `AWS::SSM::Parameter::Value<String>`, as the CloudWatch stack
reads the Slack IDs; not a `{{resolve:ssm:…}}` reference. `validateConfig` wants a name starting
with `/` and refuses a host or a URL there, without repeating it. What follows:

- The parameter must exist before the first deploy with `apiHub`, or the change set fails.
- CloudFormation reads it on every deploy of Delivery. A new host takes a Delivery deploy, and
  `cdk diff` doesn't show it: it compares templates.
- The CDK CLI never skips a stack with such a parameter as unchanged (2.1116.0, `canSkipDeploy`:
  "some parameters come from SSM"), so with `apiHub` every deploy naming Delivery, `-all` too,
  makes a change set; `docs/deployment.md` says so. `deploy-scope.sh` needs no change.
- Without `apiHub` nothing changes: every template is byte for byte `e75ef6219`'s, and no
  parameter is added (a test checks the template's parameters).

**Deployment.** The pull request changes nothing on production. `infra/envs/production.json`
changes only at the deployment, in two pull requests, as the hub's runbook has it
(`docs/runbook.md` in `rumblefishdev/sorobanscan-api-portal`): first `apiHub` with
`pages: false`, then, once the backend answers from AWS, `pages: true`. Testnet is never changed.
The distribution answers every 403 and 404 with the explorer's `index.html`, so a failing backend
(no invoke permission, a wrong host in the parameter) looks like the explorer: call the function
URL directly first.

**The way back.** Set `pages` to `false`: the stack goes back to the backend-only template, with
the old pages and the x402 portal as before.
Removing `apiHub` goes back to today's. Either recreates `api-spa-routing`,
`x402-portal-routing` and `api-hub-redirect` under the names the start deletes: after the start's
deploy, check they are gone: `aws cloudfront list-functions --stage LIVE --query
"FunctionList.Items[?contains(Name,'api-spa-routing')||contains(Name,'x402-portal-routing')||contains(Name,'api-hub-redirect')].Name"`
must print `[]`.

## Left for a clean-up after the start

Not done here, since the way back needs them:

- `ApiHubPage` and its route in the explorer's app (the way back shows the old hub).
- `enableX402PortalBasicAuth`, which gates nothing once `pages` is on.
- The unarmed path: `api-portal-behaviors.ts` and the x402 portal's function, behaviours and
  `api-hub-redirect`.
- The bucket's `api/x402-facilitator/` prefix.
- The KeyValueStore's `auth-token-x402` key.
