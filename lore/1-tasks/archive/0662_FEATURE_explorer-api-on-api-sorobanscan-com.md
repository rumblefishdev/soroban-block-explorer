---
id: '0662'
title: 'Serve the explorer API at api.sorobanscan.com beside the old host'
type: FEATURE
status: completed
related_adr: ['0048']
related_tasks: ['0659', '0661', '0663', '0664']
tags: [infra, cdk, api-gateway, cloudflare, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/700'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/702'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/703'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/704'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/pull/707'
history:
  - date: '2026-11-03'
    status: backlog
    who: stkrolikiewicz
    note: 'Created under 0659. Needs 0661.'
  - date: '2026-11-03'
    status: active
    who: stkrolikiewicz
    note: >
      Started: the CDK side (Step 1), off until both new config fields are set.
      0661's certificate does not exist yet.
  - date: '2026-11-05'
    status: completed
    who: stkrolikiewicz
    note: >
      Step 3 shipped with 0663 (#704, release #707): the SPA bundle, the
      OpenAPI base_url and the load-test default read api.sorobanscan.com on
      production and api-testnet.sorobanscan.com on testnet. The old hosts
      still serve until 0664.
---

# Serve the explorer API at api.sorobanscan.com beside the old host

## Summary

Serve the explorer API at `api.sorobanscan.com` (production) and
`api-testnet.sorobanscan.com` (testnet), proxied through the sorobanscan.com
zone. The old `api-*sorobanscan.rumblefishdev.com` hosts keep serving until
0664 turns them into redirects.

## Stan teraz

- Done (2026-11-04): Steps 1 and 2 on both networks. `api.sorobanscan.com`
  (`production-2026.11.04-3-ApiGateway`) and `api-testnet.sorobanscan.com`
  (laptop) answer like the old hosts: `/health` 200, `/v1/ledgers` 401, a
  direct origin hit 403.
- Done (2026-11-05): Step 3 with 0663. Testnet from a laptop, production by
  `production-2026.11.05-1-Compute` (`API_BASE_URL`) and `-4-web`
  (`VITE_API_BASE_URL`).
- In force: the old hosts serve the same API until 0664 redirects them.

## Context

- `infra/src/lib/stacks/api-gateway-stack.ts` has a legacy domain (Route 53,
  off) and one Cloudflare domain, `cloudflareApiDomainName`. Changing that
  field replaces the custom domain: the old host stops at that deploy, and the
  SPA, built against it, breaks until rebuilt.
- The API host is read by `VITE_API_BASE_URL` (`infra/Makefile`), the OpenAPI
  `base_url` (`compute-stack.ts`), the load-test default
  (`crates/load-tests/src/main.rs`) and the smoke table in `docs/deployment.md`.

## Implementation Plan

### Step 1: CDK

Shortest diff: an optional second Cloudflare domain and its certificate (0661's
eu-central-1 wildcard), with its regional target as an output. Drop it once
0664 lands.

### Step 2: DNS

Proxied CNAME in soroban-infrastructure → that regional target.

### Step 3: Switch the readers

`VITE_API_BASE_URL`, `base_url` and the load-test default → the new host. The
SPA redeploy can ride with 0663.

## Acceptance Criteria

- [x] `https://api.sorobanscan.com/health` and
      `https://api-testnet.sorobanscan.com/health` → 200.
- [x] `/v1/ledgers?limit=1` through the new host → not 403: the edge secret
      arrives. It answers 401 without a key, as the old host does, since
      `enableAuthLayer` is on.
- [x] The old hosts still serve.
- [x] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (API hostname, who owns its edge rules), `docs/deployment.md` (smoke
      table; the dark `api.sorobanscan.rumblefish.dev` rows replaced).
- [x] **API types regenerated** — N/A: `crates/api/**` changed in #704 (the
      comma-list CORS origin), not the API's types; CI's API types freshness
      check passed.

## Design Decisions

### From Plan

1. **A second custom domain**, not a renamed one: renaming
   `cloudflareApiDomainName` replaces the domain and stops the old host at
   that deploy.

### Emerged

2. **The 200 criterion read as "not 403"**: with the auth layer on, an
   anonymous `/v1/ledgers` is 401 on every host. 401 proves the edge secret
   arrived; a missing one is 403.
