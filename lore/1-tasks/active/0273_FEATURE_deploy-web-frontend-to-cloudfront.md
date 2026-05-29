---
id: '0273'
title: 'FEATURE: Deploy web frontend to CloudFront (production, API-stale)'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0066', '0257']
tags:
  [priority-medium, effort-small, layer-frontend, frontend, cloudfront, deploy]
links:
  - web/vite.config.ts
  - web/src/api/config.ts
  - infra/src/lib/stacks/delivery-stack.ts
history:
  - date: 2026-05-29
    status: active
    who: fmazur
    note: >
      Created. Stand up the existing Vite/React SPA on the already-deployed
      Explorer-production-Delivery CloudFront + S3 infra so the explorer is
      browsable at https://sorobanscan.rumblefish.dev before the API is live.
---

# Deploy web frontend to CloudFront (production, API-stale)

## Summary

The CDK frontend hosting (`Explorer-production-Delivery`: private S3 bucket
`production-soroban-explorer-spa` + CloudFront distribution with SPA routing,
WAF, Route 53 `sorobanscan.rumblefish.dev`) is fully deployed but the S3 bucket
is empty — nothing is served yet. This task builds the `web/` Vite/React SPA
and syncs it to the bucket so the site is reachable from a browser. The API is
not live yet (indexer cut over in 0241, API read-path lands in 0243), so the
frontend ships "API-stale": the shell loads, data sections degrade via
`SectionErrorBoundary`.

## Status: Active

**Current state:** Infra confirmed present (Delivery + CloudFrontWaf). Build +
S3 sync + CloudFront invalidation pending.

## Context

- Frontend is a static SPA (Vite + React, `createBrowserRouter`), builds to
  `web/dist/`. No SSR. Servable directly from S3 + CloudFront.
- `web/src/api/config.ts` **throws at app init** if `VITE_API_BASE_URL` is
  missing or not a valid URL. So a valid URL must be baked at build time even
  though the API is not serving yet — chosen value:
  `https://api.sorobanscan.rumblefish.dev` (the `apiDomainName` from
  `infra/envs/production.json`). The URL is only validated, not fetched at
  load, so the app renders; per-section data fetches fail gracefully.
- No CDK `BucketDeployment` — content is delivered via `aws s3 sync` +
  `cloudfront create-invalidation` (pattern mirrored from the retired
  `.github/workflows/deploy-staging.yml`).

## Implementation Plan

### Step 1: Build the SPA

```bash
VITE_API_BASE_URL=https://api.sorobanscan.rumblefish.dev \
  npx nx build @rumblefish/soroban-block-explorer-web
# output: web/dist/
```

### Step 2: Resolve Delivery stack outputs (operator / AWS)

```bash
aws cloudformation describe-stacks --region eu-central-1 \
  --stack-name Explorer-production-Delivery \
  --query 'Stacks[0].Outputs[?OutputKey==`SpaBucketName`||OutputKey==`DistributionId`]' \
  --output table
# If the stack is absent: cd infra && make deploy-production-delivery
```

### Step 3: Sync + invalidate (operator / AWS)

```bash
aws s3 sync web/dist/ s3://<SpaBucketName>/ --delete
aws cloudfront create-invalidation --distribution-id <DistributionId> --paths "/*"
```

### Step 4: Verify

Open `https://sorobanscan.rumblefish.dev` — shell + client routing work; data
sections show graceful errors until the API is live.

## Acceptance Criteria

- [ ] SPA built with `VITE_API_BASE_URL=https://api.sorobanscan.rumblefish.dev`
- [ ] `Explorer-production-Delivery` confirmed deployed (SpaBucketName +
      DistributionId resolved)
- [ ] `web/dist/` synced to the SPA bucket
- [ ] CloudFront invalidated
- [ ] `https://sorobanscan.rumblefish.dev` loads in a browser (shell renders,
      client-side routes resolve, no white-screen crash)
- [ ] **Docs updated** — N/A — no change to system shape (uses existing
      Delivery infra; no schema/API/topology change)
- [ ] **API types regenerated** — N/A — task does not touch `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`

## Notes

- API-stale window is expected and accepted; full data lights up once the API
  read-path (task 0243) is deployed against CH. No frontend rebuild needed then
  — same `api.sorobanscan.rumblefish.dev` URL is already baked in.
- Follow-up worth considering: a production deploy GitHub Actions workflow
  (`deploy-production.yml`) modelled on the retired staging one, so future SPA
  deploys are push-button instead of manual sync.
