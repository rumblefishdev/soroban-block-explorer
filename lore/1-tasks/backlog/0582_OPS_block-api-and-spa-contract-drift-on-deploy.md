---
id: '0582'
title: 'OPS: a deploy that leaves API and SPA on different API contracts fails loudly'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0374', '0103', '0087']
tags: [layer-infra, layer-api, layer-frontend, priority-high, effort-small]
links: []
history:
  - date: '2026-09-24'
    status: backlog
    who: karolkow
    note: >
      Spawned from a production outage (2026-09-24): the API was deployed from
      develop without the SPA, and the pool list crashed. Decision 93 B: the
      deploy-time guard only; no canary, no oasdiff, no browser error
      reporting for now.
---

# OPS: a deploy that leaves API and SPA on different API contracts fails loudly

## Summary

Stamp the API and the SPA with the hash of the API contract they were built
from, and make every production deploy end by comparing the two live stamps.
A mismatch stops the deploy with an error that names the half still to ship.

## Context

On 2026-09-24 `Explorer-production-Compute` was deployed from `develop` at
09:33 UTC (`make deploy-production-compute`, laptop). `develop` carried 0374
PR 3 (#479), which replaced `asset_a` / `asset_b` with `legs` on the pool
endpoints. The SPA was not deployed; the live bundle still read `asset_a`,
and `/liquidity-pools` crashed with `Cannot read properties of undefined
(reading 'asset_type_name')` until the SPA was deployed from the same commit.

Nothing caught it: the CI smoke (`deploy-production.yml`) checks only
`/health` and an HTTP 200 on `/`, the page loaded and only its render threw,
and a laptop `make` deploy runs no check at all. The release tag path deploys
Compute and SPA together; the gap is a single-half deploy.

## Implementation

1. **Contract hash.** `sha256` of `libs/api-types/src/openapi.json`, which the
   `API types freshness` gate keeps equal to the API's own spec at every
   commit. Computed once in `infra/Makefile` from the checkout being deployed.
2. **SPA stamp.** `build-production-web` passes it in (e.g. `VITE_API_CONTRACT`)
   and writes `web/dist/api-contract.txt`.
3. **API stamp.** CDK sets it as a Compute Lambda env var; `/health` returns it
   beside `status` (still exempt from auth).
4. **Check.** `check-production-contract`: fetch both live stamps, fail with
   "API and SPA are on different contracts — deploy <the other half> from
   <commit>" when they differ. Run it at the end of `deploy-production-compute`,
   `deploy-production-web` and the CI deploy job.
5. Optional, before a single-half deploy: compare the local hash with the live
   other half and warn that this deploy changes the contract.
6. **Stop on missing stack outputs.** `deploy-production-web` goes on with an
   empty bucket and distribution when `describe-stacks` returns nothing — seen
   on 2026-09-24 with expired admin credentials (`SPA bucket=  distribution=`);
   only `aws s3 sync`'s own parameter check stopped it. Fail right after the
   lookup when either value is empty, naming the likely cause (credentials).

## Acceptance Criteria

- [ ] `/health` and `api-contract.txt` carry the same hash when both halves
      come from one commit
- [ ] Deploying one half from a commit with a changed `openapi.json` makes the
      make target (and the CI job) exit non-zero with the named fix
- [ ] A deploy that does not change the contract passes silently
- [ ] `deploy-production-web` exits before any sync when the bucket or
      distribution lookup comes back empty
- [ ] `docs/deployment.md` describes the check and what to do on failure
