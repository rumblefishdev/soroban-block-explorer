---
id: '0312'
title: 'OPS: deploy CloudflareBootstrap slim-down — orphan the dead OriginSecret (0277 leftover)'
type: OPS
status: completed
related_adr: ['0048']
related_tasks: ['0277', '0291']
tags: ['infra', 'cloudflare', 'cdk', 'cleanup', 'phase-future', 'priority-low']
links: []
history:
  - date: 2026-06-22
    status: backlog
    who: fmazur
    note: >
      Spawned while reviewing the prod `cdk diff` before the 0291 Compute
      deploy. `Explorer-production-CloudflareBootstrap` shows a pending
      `[-] AWS::SecretsManager::Secret OriginSecret ... orphan` — a committed
      but never-deployed remnant of the 0277 API-only re-scope.
  - date: 2026-07-27
    status: backlog
    who: karolkow
    note: >
      Still pending five weeks on, re-measured during the task 0302 prod diff:
      `[-] AWS::SecretsManager::Secret OriginSecret OriginSecret5DDC59F1 orphan`
      is unchanged. Not folded into 0302 — that task deployed `Delivery` and
      `ApiGateway` only, and left this stack untouched. Two things worth adding
      to the decision: the drift is now the oldest of at least three undeployed
      deltas sitting in production (`Compute` also carries two secret-description
      fixes plus three Lambda asset hashes), and every one of them would ship
      silently under `make deploy-production`, which is `--all`. A periodic
      read-only `cdk diff --no-changeset` across the app would surface this class
      of drift before someone runs `--all` by accident. Note also that
      `cdk diff` hides entries containing non-ASCII characters unless `--strict`
      is passed, so a diff read without it is not a complete diff.
  - date: 2026-08-10
    status: completed
    who: karolkow
    note: >
      Deployed with `--exclusively` from a clean checkout (the flag matters:
      this stack has no dependencies, but the habit is what stops a repeat of
      the incident where a per-stack deploy dragged in a dependency's
      half-finished change). Verified after: stack UPDATE_COMPLETE, the secret
      dropped from stack resources, the physical secret intact in Secrets
      Manager (Retain), TF-state bucket untouched, API still 401s a raw
      request. Two earlier attempts never reached CloudFormation - the first
      was run from the home directory and died on `cd: no such file or
      directory`, which the stack's event history confirmed (no events since
      creation in June). Production diff is now clean, so the confirmation
      prompt added to `make deploy-production` under 0455 has a meaningful
      baseline: a non-empty diff means something new.
---

# Deploy CloudflareBootstrap slim-down — orphan the dead OriginSecret

## Summary

`make diff-production` reports a pending change on
`Explorer-production-CloudflareBootstrap`:

```
[-] AWS::SecretsManager::Secret OriginSecret OriginSecret5DDC59F1 orphan
[-] Output OriginSecretName ...
```

This is the undeployed tail of the 0277 re-scope (commit `43ef9b8e`,
"rescope Cloudflare edge IaC to API-only split"), which slimmed
`CloudflareBootstrapStack` to just the TF-state bucket and dropped the
`OriginSecret` from its definition. The stack was never re-deployed after
that commit, so the deployed stack still manages the now-dead secret.

## Context

- **Harmless / verified.** The live edge-auth secret is a **different**
  resource: `EdgeSecret`, created in the **Compute** stack
  (`infra/src/lib/stacks/compute-stack.ts:194`) and injected into the API
  Lambda as `EDGE_SECRET` (line 293). Nothing references the
  CloudflareBootstrap `OriginSecret` anymore (no cross-stack import; the
  separate `OriginSecret*` in `delivery-stack.ts` is the unrelated CloudFront
  KVS lock).
- The diff is `orphan` (`RETAIN`), **not** `destroy` — applying it removes the
  secret from stack management but leaves the physical secret in AWS;
  edge-auth is unaffected either way.
- Surfaced because the 0291 PR's prod diff was reviewed before deploying the
  Compute stack. **Not bundled into 0291** (Compute-only deploy does not touch
  this stack).

## Implementation

- Decide: apply the cleanup (`make deploy-production` or a targeted
  `cdk deploy Explorer-production-CloudflareBootstrap`) to drop the dead
  secret from stack management, **or** explicitly accept it as a known no-op
  and document.
- If applying: confirm post-deploy that edge-auth still works (it reads
  Compute's `EdgeSecret`, so it should be untouched) and that no other stack
  imported the orphaned secret.
- Optionally delete the now-unmanaged physical `OriginSecret` in Secrets
  Manager afterwards (manual, only once confirmed truly unused).

## Acceptance Criteria

- [x] `cdk diff` on `Explorer-production-CloudflareBootstrap` is clean —
      deployed 2026-08-10, stack now UPDATE_COMPLETE with only CDKMetadata,
      the TF-state bucket and its policy; the `OriginSecretName` output is
      gone.
- [x] Edge-auth verified intact after the deploy — a raw request to the API
      still answers `401 authentication required` (it reads Compute's
      `EdgeSecret`, a different resource).
- [x] No other stack depends on the orphaned `OriginSecret` — verified before
      the deploy from the deployed template (`DeletionPolicy: Retain`) and
      CloudTrail: the secret's entire access history is `DescribeSecret` calls
      by humans, never a `GetSecretValue`, and `enableOriginSecretLock` is
      false in production. The physical secret survives, unmanaged.
