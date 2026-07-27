---
id: '0438'
title: 'OPS: CloudflareBootstrap stack drift — deployed OriginSecret no longer exists in code'
type: OPS
status: backlog
related_adr: ['0048']
related_tasks: ['0302', '0277']
tags: [effort-small, priority-medium, infra, cloudflare, drift, secrets]
links: []
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from task 0302. Found while diffing production before the AWS WAF
      teardown — unrelated to WAF, so it was kept out of that task's scope.
---

# CloudflareBootstrap stack drift — deployed `OriginSecret` no longer exists in code

## Summary

`Explorer-production-CloudflareBootstrap` still manages an
`AWS::SecretsManager::Secret` named `OriginSecret` that was deleted from the CDK
source. The stack has not been redeployed since, so code and deployed state
disagree. The next deploy of that stack — including any `make deploy-production`,
which is `--all` — will silently orphan the secret.

## Context

Measured 2026-07-27 in account `750702271865` with `cdk diff --no-changeset`:

```
Stack Explorer-production-CloudflareBootstrap
Resources
[-] AWS::SecretsManager::Secret OriginSecret OriginSecret5DDC59F1 orphan

Outputs
[-] Output OriginSecretName: …
```

`OriginSecret` appears nowhere in `infra/src` — it was removed by
`43ef9b8e refactor(lore-0277): rescope Cloudflare edge IaC to API-only split`.
That commit narrowed the Cloudflare IaC to the API side; the frontend origin-lock
path it belonged to was never adopted (`enableOriginSecretLock` is `false`, and per
[ADR 0048](../../2-adrs/0048_cloudflare-edge-over-aws-waf.md) plus task 0302 the
frontend is deliberately left without edge protection, so it is not coming back).

`orphan`, not `destroy`: CloudFormation would stop managing the secret and leave it
in Secrets Manager. Nothing is lost at the moment of the deploy. The problem is what
follows — an unmanaged secret that no code creates, no code rotates, and no one is
watching, still billing and still holding a value that may or may not be live.

Not deployed as part of task 0302: that task ships `Delivery` and `ApiGateway` only,
so this stack was left exactly as found.

## Implementation

1. Establish what the secret currently holds and whether anything reads it. The
   CloudFront viewer-request function that consumed the `X-Origin-Secret` header is
   not deployed (`enableOriginSecretLock: false`), so the expectation is nothing —
   confirm rather than assume.
2. Decide between two ends, and record which:
   - **delete it** — nothing references it and the origin-lock path is not being
     adopted; take the value out of Secrets Manager entirely rather than orphaning
     it; or
   - **keep it deliberately** — then it needs an owner and a reason written down,
     because the code will not recreate it.
3. Deploy `Explorer-production-CloudflareBootstrap` so code and deployed state
   agree again. Use `--exclusively`; the stack is standalone (no `addDependency`).
4. Re-run `cdk diff` on that stack and confirm it reports no differences.

## Acceptance Criteria

- [ ] Established whether anything reads `OriginSecret`
- [ ] Outcome recorded: deleted, or kept with a stated owner and reason
- [ ] `Explorer-production-CloudflareBootstrap` redeployed
- [ ] `cdk diff Explorer-production-CloudflareBootstrap` reports no differences

## Notes

Worth a wider sweep in the same pass: `cdk diff` across the whole app on
2026-07-27 also showed `Explorer-production-Compute` carrying two undeployed
`Secret` description fixes (a mojibake `?` replaced with an em dash). Harmless in
itself, but it is the same class of problem — production drifting behind `master`
because every deploy is manual and per-stack. A periodic full `cdk diff --no-changeset`
(read-only) would surface these before someone runs `--all` and ships them by accident.
