---
id: '0631'
title: 'OPS: drop the old api/ and pricing-api/ prefixes from the Prices portal bucket'
type: OPS
status: completed
related_adr: []
related_tasks: ['0608', '0519', '0660']
tags: [infra, prices-portal, phase-future, effort-small, priority-low]
links: []
history:
  - date: '2026-10-06'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0608 future work (its deploy order, step 4). Waits on the
      Prices asset_id migration.
  - date: '2026-11-04'
    status: completed
    who: stkrolikiewicz
    note: >
      Done ahead of 0660, so the new api/prices-api/ prefix never shares api/
      with dead files. 57 objects under api/ and 18 under pricing-api/ deleted
      (versioning off; local backup kept by stkrolikiewicz). The asset_id
      wait was dropped: the live distribution reads neither prefix.
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

- [x] `api/` and `pricing-api/` gone from the bucket (only `prices-api/`
      left, 2026-11-04)
- [x] `/api/` and `/pricing-api/` still answer 301 to `/prices-api/`
      (`/api/dashboard?x=1` → `/prices-api/dashboard?x=1`, query kept)

## Design Decisions

### Emerged

1. **Not waiting for the Prices asset_id migration.** On EA2TLS5SS5M87 the
   `/api*` and `/pricing-api*` behaviors run the redirect function before S3,
   and no other distribution reads the bucket, so nothing could serve the old
   files. The local backup covers a restore.
