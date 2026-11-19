---
id: '0679'
title: 'REFACTOR: remove dead API Gateway config (partner usage plan, second CORS, unused keys)'
type: REFACTOR
status: active
related_adr: ['0048']
related_tasks: ['0677', '0678']
tags: [phase-future, effort-small, priority-low, layer-infra]
links: []
history:
  - date: 2026-11-19
    status: backlog
    who: karolkow
    note: 'Spawned from the 0677 foundations review (W32 A).'
  - date: 2026-11-19
    status: active
    who: karolkow
    note: >
      Activated. Checked what is really dead: the partner usage plan and key
      (AWS applies usage plans per key on key-required methods only; the proxy
      method requires none; 0 usage on both networks in 30 days; the Lambda
      allowlist is a separate secret) and `apiGatewayCacheTtlImmutable` (never
      read). Not dead, kept: the legacy and rumblefishdev.com API domain
      branches (they carry the mTLS attachment task 0678 needs and the
      API_BASE_URL fallback) and the preflight MOCK (it answers the credentialed
      mint preflight of task 0677; removing it is a behaviour change).
---

# REFACTOR: remove dead API Gateway config

## Summary

The 0677 foundations review found infrastructure that does nothing. Each piece
is a second place a reader must keep in mind.

## Context

- **Partner usage plan + `PartnerApiKey` + daily quota**
  (`infra/src/lib/stacks/api-gateway-stack.ts:159-185`): the `{proxy+}`
  method has `apiKeyRequired=false`, so the plan gates and counts nothing.
  Measured 2026-11-19: `aws apigateway get-usage` on `production-partner-plan`
  shows 0 usage for 2026-10-21..11-19. The real paid check is the Lambda
  allowlist (`crates/api/src/auth/mod.rs`).
- **Two CORS truths**: the API Gateway preflight MOCK
  (`api-gateway-stack.ts:128-147`) and the axum `CorsLayer`
  (`crates/api/src/main.rs:149-159`) must agree. axum can answer preflights
  alone (cost: ~60–120 more Lambda calls a day, F12 in task 0677). This one
  changes behaviour; it may need its own PR.
- **Unused config**: `apiGatewayCacheTtlImmutable` is declared
  (`infra/src/lib/types.ts:114`) and never read; the legacy and
  `rumblefishdev.com` API domain branches are off in both envs. The mTLS
  truststore branch stays: task 0678 uses it.
- Stale comment: `api-gateway-stack.ts:19` calls the stage throttle the only
  volumetric protection; Cloudflare's rate limit exists too.

## Premise

Fresh opus review of the goal alone (2026-11-19):

- **holds** — the partner usage plan, key and quota do nothing: both APIs
  have `ANY`/`OPTIONS` on `/` and `/{proxy+}` with `apiKeyRequired=false`;
  `get-usage` is empty on both plans (prod since 2026-08-21, testnet since
  2026-10-05); no script, load test, alarm or doc reads the key; the paid
  tier is the Lambda allowlist; the stage throttle (50/100) comes from
  `deployOptions`, not the plan.
- **holds** — `apiGatewayCacheTtlImmutable` is never read.
- **gap** — three docs claim the usage plan is a control (technical design
  overview, infrastructure overview, SCF milestone-3 security checklist
  row 3); they change with the plan's removal; the SCF checklist wording is
  the owner's call.
- **doubtful** — "only the immutable TTL is dead": the whole stage-cache
  config (`apiGatewayCacheEnabled`, size, mutable TTL, the cache branch) is
  parked and never runs (`api-gateway-cache-spec.md`: NOT ADOPTED). Scope
  question for the owner.

## Acceptance Criteria

- [ ] Partner usage plan, key and quota removed; `cdk diff` shows only their
      deletion.
- [ ] Decision recorded on the second CORS layer (remove the MOCK or keep it).
- [ ] Unused config keys and dead domain branches removed.
- [ ] **Docs updated** — `docs/architecture/infrastructure/**` where they
      describe these.
- [ ] **API types regenerated** — N/A unless `crates/api` changes.
