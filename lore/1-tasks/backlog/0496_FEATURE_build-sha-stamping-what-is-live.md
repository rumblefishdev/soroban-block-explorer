---
id: '0496'
title: 'FEATURE: stamp the build SHA so what is live on production is knowable'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0390']
tags: [ci, frontend, layer-infra, observability, priority-medium, effort-small]
links:
  - .github/workflows/deploy-production.yml
history:
  - date: '2026-08-17'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0390's deferred hardening, the most useful of the three left
      behind. Twice now the question "what is actually running on production?"
      has had no cheap answer: during the 0437 401 incident it took manual
      bundle-hash comparison, and on 2026-08-17 the only way to judge whether a
      Compute drift was real code or rebuild noise was to diff `crates/` against
      the commit that was master's tip at the stack's last-updated timestamp —
      an inference, not a fact.
---

# Stamp the build SHA so what is live is knowable

## Summary

Bake the commit SHA into the SPA build and expose it, so the live version can
be read in one request instead of reconstructed.

## Context

Nothing on production names the commit it was built from. The stack's
`LastUpdatedTime` plus a git log is the current method, and it is only an
inference — it assumes the operator deployed master's tip and deployed nothing
since. Rollback has the same gap: [0390](../archive/0390_REFACTOR_deploy-workflow-cleanup-prod-template.md)
records that the reliable rollback is "rebuild the known-good commit **with**
the prod env and deploy", which first requires knowing which commit is good.

## Implementation

- Pass `VITE_COMMIT_SHA` into the SPA build: `github.sha` in
  `deploy-production.yml`, `git rev-parse --short HEAD` as the laptop fallback
  in `infra/Makefile` (`build-production-web`), so a manual deploy is stamped
  too and not silently blank.
- Surface it in `index.html` as a `<meta name="commit-sha">`. One `curl` +
  `grep` then answers the question, with no JS and no new endpoint.
- **Decide the Nx cache question deliberately.** 0390 added `VITE_*` vars as
  `build` inputs so an env change busts the cache. A SHA changes every commit,
  so listing it there defeats caching entirely. Either keep it out of the
  inputs (accepting that a cache hit can carry a stale stamp) or accept the
  rebuild — record which, and why, in the task.

## Out of scope

A `/version` endpoint on the API. The Lambda side is already pinned by the
asset hashes visible in CloudFormation, so it would answer a question that is
not open. Add it if the SPA stamp turns out to be the wrong surface.

## Acceptance Criteria

- [ ] Production HTML carries the commit SHA; one `curl` reads it.
- [ ] The value is correct for a CI tag deploy **and** for a laptop
      `make -C infra deploy-production-web` — no blank, no stale stamp.
- [ ] The Nx-cache trade-off above is decided and written down, not left
      implicit.
- [ ] **Docs updated** — `docs/deployment.md` rollback section points at the
      stamp instead of describing a hunt.
- [ ] **API types regenerated** — N/A (no API surface change).
