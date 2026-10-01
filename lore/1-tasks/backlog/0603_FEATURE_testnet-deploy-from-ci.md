---
id: '0603'
title: 'Deploy testnet from CI instead of an operator laptop'
type: FEATURE
status: backlog
related_adr: ['0052']
related_tasks: ['0553', '0390']
tags: ['testnet', 'ci', 'effort-small', 'priority-low']
links: []
history:
  - date: 2026-10-01
    status: backlog
    who: karolkow
    note: 'Spawned from 0553 (phase 6 of its plan): testnet ships by hand first, CI later.'
---

# Deploy testnet from CI instead of an operator laptop

## Summary

Testnet (task 0553) ships by hand, like production: `make -C infra
deploy-testnet` from a laptop. This task adds a GitHub Actions deploy for it,
so a testnet deploy needs no admin shell and leaves a record in CI.

## Context

Phase 6 of 0553's plan (`notes/I-implementation-phases.md`), deferred out of
the epic on 2026-10-01. Production already deploys from CI on a tag
(`.github/workflows/deploy-production.yml`, task 0390); ADR 0052 §3 maps
branch to environment (`develop → testnet → master`).

## Implementation Plan

- `deploy-testnet.yml` from the dispatch/tag template of 0390.
- GitHub environment `testnet` with its own deploy role from `cicd-stack.ts`
  (today it mints one role, bound to `production`).
- Trigger: manual dispatch from `develop` first; a `testnet` branch only when
  a stable promotion point is wanted (ADR 0052 §3).

## Acceptance Criteria

- [ ] A testnet deploy runs from GitHub Actions with the `testnet` role, and
      that role cannot deploy `Explorer-production-*`.
- [ ] `docs/deployment.md` § Testnet names the CI path next to the manual one.
