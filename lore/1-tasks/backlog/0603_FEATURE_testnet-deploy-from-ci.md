---
id: '0603'
title: 'Every production release also deploys testnet, from CI'
type: FEATURE
status: backlog
related_adr: ['0052']
related_tasks: ['0553', '0390']
tags: ['testnet', 'ci', 'effort-medium', 'priority-medium']
links: []
history:
  - date: 2026-10-01
    status: backlog
    who: karolkow
    note: 'Spawned from 0553 (phase 6 of its plan): testnet ships by hand first, CI later.'
  - date: 2026-10-05
    status: backlog
    who: karolkow
    note: >
      Scope widened: a production release deploys testnet too, so the two
      sites never differ in look; the indexer pause leaves the deployed config.
      Shape chosen after an independent devil's-advocate review of three options.
---

# Every production release also deploys testnet, from CI

## Summary

Testnet (task 0553) ships by hand, like production: `make -C infra
deploy-testnet` from a laptop. This task adds a GitHub Actions deploy for it,
so a testnet deploy needs no admin shell and leaves a record in CI, and every
production release deploys testnet from the same commit.

## Context

Phase 6 of 0553's plan (`notes/I-implementation-phases.md`), deferred out of
the epic on 2026-10-01. Production already deploys from CI on a tag
(`.github/workflows/deploy-production.yml`, task 0390); ADR 0052 §3 maps
branch to environment (`develop → testnet → master`).

## Decision — 2026-10-05

The network switcher (0553) links the two sites, and they were deployed
separately, so a release left testnet behind mainnet in look. Options weighed:
(A) one release deploys both networks; (B) one SPA bundle that picks its network
from the hostname; (C) one site for both networks. An independent review found
B leaves the timing problem untouched (it changes how often the bundle is
built, not when each bucket gets it) and C stays rejected (0553, "Address").
Chosen: A, with the indexer pause moved out of the deployed config.

**Rule:** testnet is never behind production. It may be ahead, on purpose (a
manual deploy from `develop`).

## Implementation Plan

- **A second job in `deploy-production.yml`**, `needs: deploy`, so a testnet
  failure never blocks or rolls back production. It deploys
  `Explorer-testnet-Compute --exclusively` and the testnet SPA together: the
  testnet SPA without its API would call an older API. Never `--all` (testnet
  may carry its own parked deltas). Stack selectors in the tag stay
  production-only.
- **GitHub environment `testnet` with its own deploy role** from
  `cicd-stack.ts`. Today it mints one role, bound to `production`; its S3 and
  `DescribeStacks` grants name `production-*` while the CDK role assumption is
  account-wide, so reusing it would deploy testnet's API and then fail the SPA
  upload with 403. The operator runs `deploy-cicd`.
- **The indexer pause leaves `infra/envs/testnet.json`.** Today
  `indexerLambdaConcurrency` is config (`compute-stack.ts`), and the reset
  runbook (`docs/runbooks/testnet-reset.md`, steps 2 and 7) pauses and resumes
  by committing it on `develop`, while a release deploys `master`'s copy. A
  release during a reset would resume the indexer on a dropped database, or
  re-pause one already resumed. Move the pause to operational state a deploy
  does not write, and make the runbook pause by command. Pick the mechanism at
  the start of the task (e.g. a parameter the indexer reads at wake).
- **`publicArchivePrefix` stays in config**: a release merges `develop` into
  `master` first, so it carries the runbook's new prefix once that is
  committed. With the pause outside config, a stale prefix deployed mid-reset
  cannot run. State this in the runbook.
- **`/release` checklist line:** a schema change is applied to the `testnet`
  database as well (the init sidecar log ends `init.sql applied to testnet`);
  otherwise every schema release stalls testnet ingestion.
- **Smoke** on the testnet hosts after its job, like production's.
- Manual dispatch from `develop` stays, for testnet ahead of production.
- Drop `galexieImageTag` from `infra/envs/testnet.json`: testnet runs no
  Galexie (`ledgerSource: public-lake`), and the stale copied tag misleads.

## Acceptance Criteria

- [ ] A testnet deploy runs from GitHub Actions with the `testnet` role, and
      that role cannot deploy `Explorer-production-*`.
- [ ] A production tag deploys testnet's Compute and SPA after production, from
      the same commit; a failing testnet job leaves production deployed.
- [ ] No deploy changes whether the testnet indexer is paused; the reset
      runbook pauses and resumes without a commit.
- [ ] `/release` names the `testnet` schema step.
- [ ] `docs/deployment.md` § Testnet names the CI path next to the manual one.
