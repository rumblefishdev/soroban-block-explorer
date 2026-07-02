---
id: '0312'
title: 'OPS: deploy CloudflareBootstrap slim-down — orphan the dead OriginSecret (0277 leftover)'
type: OPS
status: backlog
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

- [ ] `cdk diff` on `Explorer-production-CloudflareBootstrap` is clean (no
      pending OriginSecret orphan), OR the diff is formally accepted +
      documented as a permanent no-op.
- [ ] Edge-auth verified intact after any deploy (API still rejects requests
      without `X-Edge-Secret`).
- [ ] No other stack depends on the orphaned `OriginSecret`.
