---
title: 'The pages, synthesized by hand (2026-11-20)'
type: research
status: mature
spawned_from: null
spawns: []
tags: [layer-infra]
links: []
history:
  - date: 2026-11-20
    status: mature
    who: adamkoot
    note: 'Moved from the task file when the task became a directory.'
---

# The pages, synthesized by hand (2026-11-20)

Production's Delivery stack, three times, by the `cdk` CLI with `infra/cdk.json`'s context and a
made-up `backendHost` (`made-up-backend.lambda-url.eu-central-1.on.aws`), each compared with
`origin/develop`'s (`cf1e5601e` after the rebase; 72032230f before) by
`cdk diff --strict --template <develop's template>`:

- _Neither:_ "There were no differences". The JSON equals develop's once its keys are sorted;
  the file lists some resources in another order, since today's `/api` behaviours are now built
  in `api-portal-behaviors.ts`. Testnet the same.
- _The backend_ (`pages: false`): one origin added (the function URL, HTTPS only, with
  `X-Hub-Origin-Secret` as `{{resolve:secretsmanager:…:SecretString:current::}}` and
  `X-Hub-Origin-Revision`), one origin request policy, one behaviour (`/api/hub/*`, before
  `/api/*`) and its viewer-request function (`api-hub-backend-headers`, step 4's), and the CDK's
  own usage record (`AWS::CDK::Metadata`) with them. Nothing else. The same JSON as the
  backend-only template of commit `170567856`, so turning the pages off again gives that one back.
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

**Since then (2026-11-20).** The setting held the host itself then. It now names an SSM parameter
(the README, "The host from SSM"): the backend's origin is a `Ref` to a template parameter, which
the backend-only template adds, so that template is no longer `170567856`'s. Without `apiHub`
every production and testnet template was synthesized again and is byte for byte the one of
`e75ef6219`, the commit before the change.
