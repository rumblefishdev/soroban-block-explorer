---
id: '0253'
title: 'FEATURE: mTLS client cert auto-rotation pipeline (AWS → Hetzner CH)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0227', '0239', '0240']
tags:
  [
    priority-high,
    effort-medium,
    layer-infrastructure,
    mtls,
    security,
    automation,
  ]
links: []
history:
  - date: '2026-05-21'
    status: backlog
    who: fmazur
    note: 'Spawned from task 0239 acceptance criterion #6 (mTLS cert rotation strategy). Cert lifetime is 365 days per issue-client-cert.sh; first prod certs issued at 0239 deploy time. Without automation, every AWS service stops authenticating to Hetzner CH on cert expiry. Hard deadline: one year from first issuance.'
---

# FEATURE: mTLS client cert auto-rotation pipeline (AWS → Hetzner CH)

## Summary

Build an automated rotation pipeline for the mTLS client certificates
that AWS workloads (Lambdas, Galexie) present to the Hetzner-hosted
ClickHouse box. Current certs have a 365-day lifetime; without
automation, every AWS service silently loses CH connectivity at expiry
and a human must manually re-issue + re-upload + redeploy each cert.

## Context

Task 0239 (AWS-side cutover) wired per-service mTLS into the CDK app:

- Each AWS service has its own client cert (CN convention
  `<service>-<environment>`, e.g. `lambda-api-production`,
  `galexie-production`).
- Certs live in AWS Secrets Manager under
  `${mtlsSecretNamePrefix}/<cn>` as `{cert, key, ca}` JSON bundles.
- Caddy on the Hetzner box maps CN → CH user via
  `CLICKHOUSE_CN_USER_MAP`; CH users are `<no_password/>`
  (proxy-trust per task 0240).

Cert issuance today is **manual**, via `infra-hetzner/ca/issue-client-cert.sh`
on the operator's Linux laptop (CA key sourced from password manager onto
`/dev/shm`). Each cert is valid 365 days.

**The deadline.** First production certs are issued at task 0239 deploy
time. Roughly 11 months later, the **first** cert in the set will hit
the 30-day warning window. Roughly 12 months later, the **last** safe
day. After that, AWS workloads fail TLS handshake to Hetzner CH and
prod is down until certs are rotated by hand.

There is no silent failure path here — CH connectivity is hard-required
for every Lambda + Galexie, so the visibility of the outage will be
loud. But the cost of rotation-by-calendar is meaningful: a human must
re-run the script, upload via AWS CLI, edit operator env, replay
ansible, and redeploy each Lambda (or trigger refresh) every year.
That's both error-prone and easy to forget.

## Acceptance Criteria

- [ ] At least 30 days before any cert in the production set expires,
      it is automatically re-issued and the new bundle is in Secrets
      Manager.
- [ ] After re-upload, AWS workloads pick up the new cert without
      manual operator action (extension cache TTL handles this if
      Lambda redeploys are not desired; otherwise a redeploy trigger
      is part of the pipeline).
- [ ] Caddy `CLICKHOUSE_CN_USER_MAP` does NOT need to change on
      rotation — CN naming stays the same, so no Ansible re-run is
      required for routine rotations.
- [ ] An alarm fires if the rotation pipeline fails (cert not issued,
      SM update failed, or expiry is < 14 days and rotation hasn't
      run).
- [ ] Operator runbook documents what to do if rotation fails.
- [ ] **API types regenerated** — N/A.
- [ ] **Docs updated** — `docs/architecture/security/clickhouse-rbac.md`
      and/or `infra-hetzner/ca/README.md` updated to describe the
      automated rotation flow.

## Open Design Questions

These need a short ADR or notes pass before implementation:

1. **Where does the CA private key live during automated issuance?**
   - The current `issue-client-cert.sh` reads the CA key from
     `/dev/shm` (operator laptop, tmpfs). An automated pipeline
     needs a different model:
     - (a) Store CA key in AWS Secrets Manager itself (encrypted at
       rest, accessible only to a scheduled Lambda). Trade-off: AWS
       now holds the CA — single source of compromise.
     - (b) Move CA to AWS Private CA (ACM PCA). Cleanly managed,
       but ~$400/month base cost.
     - (c) Keep CA on a dedicated EC2/Hetzner box that the rotation
       Lambda calls via mTLS (chicken-and-egg) or via SSH-tunneled
       trigger. Complex.
   - **Recommend (a) for cost, but only after explicit IAM scoping
     review.**
2. **Where does the rotation Lambda run?** EventBridge cron in the
   same `eu-central-1` deploy as the rest of AWS workloads. New
   stack or extend an existing one (probably a new stack to keep
   ownership clean).
3. **What triggers Lambda redeploy after secret update?** Options:
   - (a) Lambda redeploy via CodeDeploy / CDK pipeline.
   - (b) Touch the `lastModified` of the Lambda config (forces new
     execution environment on next cold start).
   - (c) Rely on extension cache TTL (`SECRETS_MANAGER_TTL` default
     300s, max 300s) — but the extension only refreshes on
     `GET`, so cold-start Lambdas without traffic could go stale.
4. **Galexie cert refresh.** ECS native secrets injection picks up
   the new value only on task restart. Plan a forced task restart
   on rotation? Galexie is singleton; restart = brief gap in S3
   exports. Acceptable.

## Implementation Sketch

(Detail in notes/ once activated.)

1. **Phase 1** — Move CA key to Secrets Manager (option (a) above) +
   IAM scoping so only the rotation Lambda can read it.
2. **Phase 2** — `cert-rotator` Lambda in `crates/cert-rotator` (Rust):
   reads CA, scans `soroban/${env}/mtls/*` secrets, identifies certs
   expiring within 30 days, issues new ones, writes back via
   `update-secret-value`. Idempotent — re-running before expiry is a
   no-op for already-rotated certs.
3. **Phase 3** — EventBridge daily cron triggers the Lambda. Lambda
   metrics in CloudWatch; alarm on `Errors > 0` or "any prod cert
   < 14 days from expiry" via a separate freshness check.
4. **Phase 4** — Lambda refresh strategy (likely option (b):
   touch each Lambda's environment variable map to force a new
   execution environment — minimal disruption, no actual code
   redeploy needed).
5. **Phase 5** — Galexie restart hook: after rotation, the cert
   rotator triggers an ECS task replacement.
6. **Phase 6** — Runbook + drill: simulate a near-expiry state in
   non-prod (or with a short-lived cert) and verify the pipeline.

## Dependencies

- [[task-0239]] — landing this depends on the AWS-side cutover being
  live; the rotation pipeline operates on the secrets that 0239
  creates.

## Out of Scope

- Rotation of CA cert itself (much rarer event; covered by
  `infra-hetzner/ca/README.md` §Compromise response).
- Hetzner-side cert rotation (Let's Encrypt auto-renews via Caddy
  already).
- Cross-account / multi-environment rotation — only production for
  now.
