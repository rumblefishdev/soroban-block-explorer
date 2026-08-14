---
id: '0090'
title: 'Security audit: OWASP Top 10, IAM least-privilege, infrastructure hardening'
type: FEATURE
status: backlog
related_adr: []
related_tasks: []
tags: [priority-high, effort-medium, layer-testing]
milestone: 3
links:
  - docs/architecture/technical-design-general-overview.md
history:
  - date: 2026-03-30
    status: backlog
    who: fmazur
    note: 'Task created — D3 scope coverage (task 0085)'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Checklist targets a retired database — corrected 2026-07-22, the obligation
      itself stands.** D3 (§7.4) still requires a signed-off security checklist and
      §7.1F still allocates 3 days; none of that changes. What changed is the
      system underneath.
      Verified: there is **no RDS resource anywhere in `infra/src`** — no
      `DatabaseInstance`, `DatabaseCluster` or `aws-rds` import. The store is
      ClickHouse on Hetzner behind an mTLS endpoint. Three of the nine criteria
      (public endpoint, backups/PITR/deletion-protection, KMS on RDS) therefore
      audit something that does not exist, and a fourth (KMS) only half applies.
      Two real exposures are absent from the list and were found this month:
      **0250** — ClickHouse does not count requests authenticated via the
      `X-ClickHouse-User` header against user quotas, and that is exactly the
      production path (Caddy proxy-trust), so the per-user query/row/byte/time
      caps in `users.d/quotas.xml` are not enforced at all. **0253** — no mTLS
      certificate rotation pipeline; expiry is a total AWS↔Hetzner outage with no
      warning.
      Criteria annotated rather than rewritten: substituting them is a scope
      decision for whoever signs the audit off, not a bookkeeping fix. Also worth
      folding in: GitHub currently reports 37 dependency vulnerabilities on the
      default branch (2 critical, 17 high), which no criterion covers.
---

# Security audit: OWASP Top 10, IAM least-privilege, infrastructure hardening

## Summary

Perform security audit covering OWASP Top 10 for the API, IAM least-privilege review, and infrastructure hardening verification. Produce signed-off security checklist required by D3 acceptance criteria.

## Status: Backlog

**Current state:** Not started.

## Context

D3 (§7.4) requires "Security audit checklist (OWASP Top 10, IAM least-privilege, no public RDS endpoint)." The effort breakdown (§7.1F) allocates 3 days. D3 acceptance criteria include: "no wildcard IAM, WAF/throttling active, RDS has no public endpoint, production RDS backups/PITR/deletion protection enabled, RDS and S3 encrypted with KMS-backed keys, all secrets in Secrets Manager, all API inputs validated."

## Implementation Plan

### Step 1: OWASP Top 10 review

Audit all API endpoints against OWASP Top 10: injection, broken auth, sensitive data exposure, XXE, broken access control, security misconfiguration, XSS, insecure deserialization, insufficient logging, SSRF.

### Step 2: IAM least-privilege review

Verify all IAM roles follow least-privilege: no wildcard policies, Lambda roles scoped to required resources, ECS task roles minimal.

### Step 3: Infrastructure hardening verification

Verify: RDS no public endpoint, RDS backups/PITR/deletion protection enabled, RDS and S3 encrypted with KMS, ~~WAF active on API Gateway~~ → **replace**: API Gateway throttling + usage-plan limits in force, and the Cloudflare edge fronting the API hostname with the origin locked to it (there is no AWS WAF — both WebACLs dropped, task 0302), all secrets in Secrets Manager, no hardcoded credentials.

### Step 4: Produce security checklist

Document findings, remediations, and sign-off.

## Acceptance Criteria

> ⚠ **Three criteria below target a database that no longer exists** — verified
> 2026-07-22, there is no `DatabaseInstance` / `DatabaseCluster` / `aws-rds`
> resource anywhere in `infra/src`. The store is ClickHouse on Hetzner, reached
> over mTLS. Auditing straight off this list would check absent things and miss
> the real ones. See the 2026-07-22 history entry.

- [ ] OWASP Top 10 review completed for all API endpoints
- [ ] No wildcard IAM policies in production
- [ ] ~~WAF/throttling active on public ingress~~ → **replace**: API Gateway
      throttling active on every public route, and Cloudflare edge protection on
      the API hostname with the origin locked to it. There is no AWS WAF (both
      WebACLs dropped, task 0302), and the CloudFront frontend distribution
      deliberately carries no edge filter — do not audit for one.
- [ ] ~~RDS has no public endpoint~~ → **replace**: ClickHouse reachable only
      through the mTLS endpoint, never directly
- [ ] ~~Production RDS: backups, PITR, deletion protection~~ → **replace**:
      ClickHouse backup + restore per `docs/backups.md`, including the gap it
      documents (a restore does not re-deliver the rolled-back range)
- [ ] ~~RDS and S3 encrypted with KMS-backed keys~~ → **narrow to S3**; the
      ClickHouse half needs its own answer
- [ ] All secrets in Secrets Manager
- [ ] All API inputs validated
- [ ] **Add**: per-user query quotas are actually enforced — task 0250 found
      they are not, on the production auth path
- [ ] **Add**: mTLS certificate rotation has an owner and a procedure —
      task 0253; expiry means a total AWS↔Hetzner outage with no warning
- [ ] Security checklist signed off
