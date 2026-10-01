---
title: 'Implementation, decisions and the first deploy'
type: synthesis
status: mature
spawns: []
tags: [frontend, infra, deploy]
links:
  - ../README.md
history:
  - date: '2026-09-30'
    status: mature
    who: stkrolikiewicz
    note: 'Moved out of the task README when it passed the size limit'
---

# Implementation, decisions and the first deploy

## Implementation Plan

### Step 1: headers at upload

Split the sync in `deploy-production-web` as the portal does. `assets/*` gets
`public, max-age=31536000, immutable`. The rest, `index.html` included, gets
`public, max-age=0, must-revalidate`.

### Step 2: keep the previous build's assets

Drop `--delete` from the assets sync, so a stale `index.html` still finds its
files. Decide how old assets are pruned: an S3 lifecycle rule on a prefix, a
second sync after a grace period, or none, since they are small.

### Step 3: verify

After a deploy, check the headers with `curl -sI`: `/` shows
`max-age=0, must-revalidate`, and `/assets/<hash>.js` shows `immutable`. Also
check that the previous build's `/assets/<hash>.js` still returns JavaScript.

## Implementation Notes

- `infra/Makefile`, `deploy-production-web`: two `aws s3 sync` passes, then
  the invalidation.
  - Pass 1: `assets/*` with `public, max-age=31536000, immutable`, without
    `--delete`.
  - Pass 2: everything else with `public, max-age=0, s-maxage=60,
must-revalidate`, with `--delete`. The CLI excludes filtered paths from
    deletion, so this pass never removes `assets/*`.
- `infra/src/__tests__/deploy-web-caching.test.ts` (new, 3 cases): reads the
  recipe from the Makefile. It checks that there are two syncs with assets
  first, the headers, no `--delete` on the assets pass, and the invalidation
  after both. Run against the old recipe, the same parser fails. Infra: 8
  tests, lint and typecheck green.
- Measured on 2026-09-29: the bucket's `index.html` has no `CacheControl`
  metadata, and one build's `assets/` is 49 objects, 1.34 MB.

- Deploy on 2026-09-30, 06:58 UTC. A throwaway worktree at 52e0b07a (the
  live content) ran `make -C infra -f <develop's Makefile>
deploy-production-web` with `soroban-admin`. The build reproduced the live
  `index-BmVZ6qtC.js` and `index-CkSoW2dv.css`. The log shows 61 uploads
  (49 assets, then 12 others) and 0 deletes. In S3, the assets'
  `LastModified` is 2 s earlier than `index.html`'s. Invalidation
  IUW7OLF7A95I0Y8MR4ENRB27A completed.

## Design Decisions

### From Plan

1. **Headers at upload, as the portal does**, rather than a response headers
   policy in the delivery stack. The rule sits next to the deploy that writes
   the files, and no CDK deploy is needed.

### Emerged

2. **`s-maxage=60` on `index.html`.** Without it, `max-age=0` would also drop
   the CloudFront edge TTL to 0 and send every request to S3. With it, the
   edge keeps the 60 s that [[0106]] chose, and browsers still revalidate.
   The deploy invalidates `/*` anyway.
3. **Old assets are never pruned.** At ~1.3 MB per build the bucket grows by
   megabytes a year. A pruning step would have to know which builds may still
   be cached. The recipe carries a `ponytail:` note with the upgrade path.
4. **A text test of the Makefile, not `make -n`.** The recipe resolves the
   bucket through `aws cloudformation` in a shell substitution, and a text
   check needs neither AWS nor `make`.

## Issues Encountered

- **Develop's SPA could not ship with this fix.** The live API Lambda
  (deployed 2026-09-29 11:34 UTC) still carries the old contract strings
  ("Participant account StrKey (G...)", "Current owner G-StrKey"), with none
  of 0374's soroban providers or 0376's contract owners. Develop's frontend
  queries soroban pool providers, which that API cannot answer. The recipe
  was therefore proven on the content already live.
- **`aws s3 sync` output uses `\r` progress lines.** `grep -c '^upload:'`
  counted 0 until the log went through `tr '\r' '\n'`. The S3 metadata was
  the reliable check.
