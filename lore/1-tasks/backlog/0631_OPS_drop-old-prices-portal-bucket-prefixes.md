---
id: '0631'
title: 'OPS: drop the old api/ and pricing-api/ prefixes from the Prices portal bucket'
type: OPS
status: backlog
related_adr: []
related_tasks: ['0608', '0519']
tags: [infra, prices-portal, phase-future, effort-small, priority-low]
links: []
history:
  - date: '2026-10-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0608 future work (its deploy order, step 4). Waits on the
      Prices asset_id migration.
---

# Drop the old Prices portal bucket prefixes

## Summary

Since 2026-10-02 the Prices portal is served from the `prices-api/` prefix of
`production-soroban-explorer-api-spa`, and CloudFront answers every `/api…` and
`/pricing-api…` URL with a 301 before S3 is reached (task 0608). The old
`api/` and `pricing-api/` prefixes are no longer read.

## Implementation

- After the Prices asset_id migration ends, check that no CloudFront behaviour
  still serves from those prefixes (`infra/src/lib/stacks/delivery-stack.ts`).
- Delete both prefixes — a production S3 write, run by the owner.

## Acceptance Criteria

- [ ] `api/` and `pricing-api/` gone from the bucket
- [ ] `/api/` and `/pricing-api/` still answer 301 to `/prices-api/`
