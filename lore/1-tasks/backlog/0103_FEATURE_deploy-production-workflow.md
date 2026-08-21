---
id: '0103'
title: 'CI/CD: Production deployment workflow'
type: FEATURE
status: backlog
related_adr: ['0001']
related_tasks: ['0039', '0110', '0038']
tags: [priority-medium, effort-medium, layer-infra, ci, cd]
milestone: 3
links: []
history:
  - date: '2026-04-07'
    status: backlog
    who: fmazur
    note: 'Spawned from task 0039. Production deploy workflow deferred to milestone 3.'
  - date: '2026-04-08'
    status: backlog
    who: stkrolikiewicz
    note: 'Scope extended: apply region-var + caching + tag-gating improvements from 0110 (staging pilot).'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Narrowed — 0390 covers the core, but not all of it. Verified 2026-07-22.**
      0390 already wrote `deploy-production.yml`: dispatch-only, `cdk diff` →
      approval gate → per-stack deploy, deliberately inert until the `production`
      environment and secrets exist. That subsumes this task's core workflow
      items.
      **Not covered by it, and therefore the remaining scope here:** mirroring the
      Galexie image to ECR with a git-SHA tag, and the post-deploy smoke test on
      `/health`. Node-modules caching is already in 0390's template.
      Two caveats for whoever picks this up. (1) 0390 is **not merged** — it lives
      on `ci/deploy-workflow-cleanup` as PR #338, open since 2026-07-14,
      MERGEABLE but unreviewed; `deploy-staging.yml` is still on develop. (2) The
      finding inherited from the cancelled 0115: passing
      `-c galexieImageTag=${GITHUB_SHA}` makes `cdk diff` report a change on every
      commit, docs-only included, so any diff-based early exit is defeated unless
      the tag is handled separately.
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Absorbed 0455 review finding 66. Measured rather than assumed: the
      committed config pins a digest, the shipped workflow never passes the
      context tag (zero mentions of Galexie), so the override is an unused
      input reachable only by a hand-run deploy — and CDK context never shows in
      a diff. Three exclusive ways to close it recorded above; the choice is
      this task's, since option 1 is the criterion it already carries.
---

# CI/CD: Production deployment workflow

## Summary

Add GitHub Actions workflow for manual production deployment with approval gate. Uses OIDC for AWS auth, shows CDK diff before approval, mirrors Galexie image to ECR, runs CDK deploy, and verifies with a smoke test.

## Context

Task 0039 defined the CI workflow (Rust + TypeScript CI jobs), CDK OIDC/deploy roles, and staging deployment workflow. The production deployment workflow was designed but deferred — not needed until production environment is ready.

The workflow file (`deploy-production.yml`) was drafted in task 0039 and can be used as starting point.

## The image-tag override, as it actually stands (0455 finding 66)

Measured 2026-08-19, because a review flagged the context override as a way to
deploy an image the repository does not name. What is actually true:

- `infra/src/lib/stacks/ingestion-stack.ts:87-89` reads `galexieImageTag` from
  CDK **context**, falling back to the committed config. That is the mechanism
  this task designed.
- `infra/envs/production.json` pins a **digest** (`sha256:…`), not a mutable
  tag. That is the safe half and it is already in place.
- `.github/workflows/deploy-production.yml` does not mention Galexie at all —
  **zero occurrences**. The `-c galexieImageTag=${GITHUB_SHA}` step in the
  acceptance criteria below was never implemented in the shipped workflow.

So the override is not a CI path that drifted; it is an unused input that only a
hand-run `cdk deploy -c galexieImageTag=…` can reach. Reaching it replaces a
digest pin with whatever string is typed, and CDK **context never appears in a
diff** — so the one gate the deploy relies on cannot show it.

Three ways to close this, to decide when this task is picked up:

1. Implement the criterion as written — CI passes the tag, and the committed
   digest stops being the source. Trades a pin for a pipeline.
2. Delete the context read. The committed digest becomes the only way to change
   the image, which is what production does today anyway.
3. Keep it and make it loud — refuse a context override unless it is also a
   digest, and print it prominently.

Option 2 is the smallest and matches current practice; option 1 is what this
task originally specified. They are mutually exclusive, so the choice belongs
here rather than in a separate task.

## Acceptance Criteria

### Core workflow (from 0039)

- [ ] Manual trigger via workflow_dispatch, restricted to master branch
- [ ] CDK diff job runs before approval for changeset review
- [ ] Required reviewers via GitHub Environment "production" protection rules
- [ ] Uses OIDC to assume production deploy role
- [ ] Mirrors Galexie image to ECR with git SHA tag (digest-pinned pull)
- [ ] Runs `cdk deploy --all` with `-c galexieImageTag=${GITHUB_SHA}`
- [ ] Concurrency group prevents parallel deploys
- [ ] Post-deploy smoke test on /health endpoint

### Extended scope — mirror improvements from 0110 (staging pilot)

Apply the three improvements validated on staging in 0110:

- [ ] **Region documentation** — add inline comments next to `us-east-1` literals referencing `infra/envs/production.json` as single source of truth (same approach as 0110 PR 1 — no `vars.AWS_REGION`, region locked by ACM cert).
- [ ] **Deploy caching** — `node_modules/` cache via `actions/cache` (same pattern as 0110 PR 2). Rust/Nx/cargo-lambda caching not worth it per 0110 Phase 0 baseline (CDK deploy is 76% of wall-clock, not build steps).
- [ ] **Tag-gated trigger** — decide tag naming scheme for production (see open questions below).

**Dependency:** 0110 should land first so production reuses the validated
patterns. If 0110 is blocked, production workflow can still be built with
the core workflow criteria, and extended scope applied as a follow-up.

## Open questions

### Production tag naming scheme

Staging uses `staging-YYYY.MM.DD-N` (date-based, per ADR 0009). Production defaults to the same date-based scheme (`prod-YYYY.MM.DD-N`). Consider pivoting to SemVer (`vX.Y.Z`) when activating this task — decision to make in 0103 scope.

### Required Reviewers for production

Staging Required Reviewers gate zostanie wyłączony po wdrożeniu tag-gatingu (tag = explicit deploy decision). Dla produkcji rozważyć:

- Czy tag-gating wystarczy (jak staging)?
- Czy prod potrzebuje dodatkowy Required Reviewers gate mimo tagów (defense in depth)?
- Kto powinien być approverem na prod?
