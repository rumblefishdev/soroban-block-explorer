---
id: '0662'
title: 'Serve the explorer API at api.sorobanscan.com beside the old host'
type: FEATURE
status: active
related_adr: ['0048']
related_tasks: ['0659', '0661', '0663', '0664']
tags: [infra, cdk, api-gateway, cloudflare, priority-medium, effort-small]
links: []
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
  direct origin hit 403. Step 3 is in code with 0663's PR.
- Next: Step 3 ships with 0663 (SPA build, OpenAPI `base_url`, load tests).
- In force: no gap. The old host serves until the new one is verified.

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

- [ ] `https://api.sorobanscan.com/health` and
      `https://api-testnet.sorobanscan.com/health` → 200.
- [ ] `/v1/ledgers?limit=1` through the new host → 200, not 403 (the edge
      secret arrives).
- [ ] The old hosts still serve.
- [ ] **Docs updated** — `docs/architecture/infrastructure/infrastructure-overview.md`
      (API hostname, who owns its edge rules), `docs/deployment.md` (smoke
      table).
- [ ] **API types regenerated** — N/A unless `crates/api/**` changes; the base
      URL comes from the environment.
