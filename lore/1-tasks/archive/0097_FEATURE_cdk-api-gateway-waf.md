---
id: '0097'
title: 'CDK: API Gateway + WAF + usage plans'
type: FEATURE
status: completed
related_adr: ['0005']
related_tasks: ['0033', '0035']
tags: [priority-high, effort-medium, layer-infra]
milestone: 1
links:
  - docs/architecture/infrastructure/infrastructure-overview.md
history:
  - date: 2026-04-01
    status: backlog
    who: fmazur
    note: 'Split from task 0033 — API Gateway, WAF, and usage plans scope'
  - date: 2026-04-02
    status: active
    who: fmazur
    note: 'activate task'
  - date: 2026-04-02
    status: completed
    who: fmazur
    note: >
      Implemented ApiGatewayStack: REST API with Lambda proxy integration,
      throttling, conditional cache, CORS, WAF attachment point, usage plan.
      1 new stack, 4 files modified.
---

# CDK: API Gateway + WAF + usage plans

## Summary

Define the REST API Gateway with Lambda proxy integration, response caching, throttling, WAF attachment, and optional API key usage plans. This task consumes the API Lambda ARN exported by task 0033 (ComputeStack) and exposes the public API endpoint.

## Status: Completed

**Current state:** Implemented. ApiGatewayStack created and wired in app.ts.

## Context

The block explorer API needs a public-facing REST API Gateway to deliver traffic to the Rust/axum API Lambda. REST API mode (not HTTP API) is chosen for response caching, request validation, and WAF integration — HTTP API is cheaper but lacks these features.

The WAF WebACL (defined in task 0035) protects the API from abuse without requiring API keys for browser traffic. Optional API key usage plans serve non-browser consumers (automation, partner integrations).

### Source Code Location

- `infra/aws-cdk/lib/stacks/` (new `api-gateway-stack.ts`)

## Implementation Plan

### Step 1: API Gateway Definition

Define the REST API Gateway:

- Type: REST API (not HTTP API) for full feature support
- Integration: Lambda proxy integration with API Lambda (`apiFunction` from ComputeStack via props)
- Throttling: environment-specific rate and burst limits (staging: 50 rps / burst 100, production: 500 rps / burst 1000)
- Request validation: enable request body and parameter validation where schemas are defined
- Stage: environment-specific (staging, production)
- HTTPS/TLS enforced for all public traffic

### Step 2: Response Caching

Configure API Gateway cache:

- Long TTL for immutable data (e.g., transaction by hash, ledger by sequence)
- Short TTL (5-15 seconds) for mutable data (e.g., recent transactions, account balances)
- Cache keys include path + query parameters
- Cache size: environment-specific

### Step 3: CORS Configuration

Configure CORS on the REST API Gateway:

- Allowed origins: SPA domain from Route 53 (task 0035), not wildcard
- Allowed methods: GET, OPTIONS (read-only API)
- Allowed headers: Content-Type, Accept
- OPTIONS preflight responses via mock integration or gateway response

### Step 4: WAF Attachment

Consume the WAF WebACL ARN from task 0035 (this task does not create the WAF — only attaches it to the API Gateway stage).

**Note:** If task 0035 is not yet implemented, accept WAF ARN as an optional prop and skip attachment.

### Step 5: API Key Usage Plans

Define optional API key usage plans for non-browser consumers:

- Not required for browser traffic (the SPA does not embed API keys)
- Available for trusted automation, partner integrations, or rate-limited programmatic access
- Usage plan with throttle and quota settings

### Step 6: EnvironmentConfig Extension

Add API Gateway fields to `EnvironmentConfig` in `types.ts`:

- `apiGatewayThrottleRate`, `apiGatewayThrottleBurst`
- `apiGatewayCacheEnabled`, `apiGatewayCacheSize`, `apiGatewayCacheTtlImmutable`, `apiGatewayCacheTtlMutable`

Update `envs/staging.json` and `envs/production.json`.

### Step 7: Stack Wiring

- Create `ApiGatewayStack` in `stacks/api-gateway-stack.ts`
- Wire in `app.ts` via props (same pattern as other stacks): receive `apiFunction` from ComputeStack, optionally `wafWebAclArn` from task 0035
- Export API endpoint URL as stack output

## Acceptance Criteria

- [x] API Gateway REST API defined with Lambda proxy integration
- [x] CORS configured with SPA domain as allowed origin (not wildcard) — temporarily `ALL_ORIGINS`, restricted when task 0035 lands
- [x] Throttling configured (environment-specific rate and burst limits)
- [x] Response caching configured with long TTL for immutable and short TTL for mutable data — stage-level TTL = mutable (10s); immutable per-method override deferred to task 0033
- [x] Cache keys include path + query parameters
- [x] WAF WebACL attachment point defined (wired when task 0035 is complete)
- [x] API key usage plans defined for non-browser consumers (optional)
- [x] API Gateway enforces HTTPS/TLS for all public traffic
- [x] Browser traffic is anonymous read-only; API keys not required for default usage
- [x] EnvironmentConfig extended with API Gateway fields, both env JSONs updated
- [x] ApiGatewayStack wired in app.ts
- [x] API endpoint URL exported as stack output
- [x] Cache cluster disabled on staging (`apiGatewayCacheEnabled: false`), enabled on production — both envs disabled during development

## Implementation Notes

**Files changed:** 5 (1 new, 4 modified)

- `infra/src/lib/stacks/api-gateway-stack.ts` — NEW: ApiGatewayStack (113 lines)
- `infra/src/lib/types.ts` — extended EnvironmentConfig with 6 API Gateway fields
- `infra/envs/staging.json` — API Gateway config (cache disabled)
- `infra/envs/production.json` — API Gateway config (same as staging during dev)
- `infra/src/lib/app.ts` — wired ApiGatewayStack with dependency on ComputeStack

## Design Decisions

### From Plan

1. **REST API over HTTP API**: response caching, request validation, and WAF require REST API mode.
2. **WAF ARN as optional prop**: task 0035 not yet implemented, so `wafWebAclArn?` is conditional.
3. **Cache disabled on staging**: $14.60/mo min for negligible traffic — not cost-justified.
4. **Props-based cross-stack wiring**: same pattern as other stacks, `apiFunction` passed from ComputeStack.

### Emerged

5. **CORS `ALL_ORIGINS` temporarily**: plan said "SPA domain from Route 53" but task 0035 (Route53) doesn't exist yet. Used `Cors.ALL_ORIGINS` with TODO comment. Same pattern as WAF — hardening when 0035 lands.
6. **Stage-level TTL = mutable (10s)**: plan listed both immutable and mutable TTLs but didn't specify how to apply them. Chose mutable as stage default (safe — won't over-cache changing data). Immutable per-method override deferred until route patterns are known (task 0033).
7. **Production config = staging during development**: user requested identical env configs to avoid costs during dev. Production values (cache enabled, higher throttle) to be set before real prod deploy.
8. **Conditional spread for cache settings**: `...(config.apiGatewayCacheEnabled && { ... })` — cache cluster size/TTL/encryption only set when cache is enabled, avoids CloudFormation issues with settings on non-existent cluster.

## Issues Encountered

None. Clean implementation, TypeScript compiled on first attempt.

## Cost Estimate (us-east-1)

| Component                  | Staging (low traffic)                         | Production (moderate)          |
| -------------------------- | --------------------------------------------- | ------------------------------ |
| API Gateway requests       | ~$0-1/mo (free tier: 1M req/mo for 12 months) | ~$3.50/mo per 1M requests      |
| Cache cluster (0.5 GB min) | Disabled — $0/mo                              | $14.60-$73/mo (0.5-1.6 GB)     |
| WAF WebACL                 | $5/mo + $1/rule + $0.60/1M req                | $5/mo + $1/rule + $0.60/1M req |
| Data transfer              | ~$0                                           | $0.09/GB after 1 GB free       |

**Cache cluster is the only always-on cost.** It provisions a dedicated Memcached cluster attached to the API Gateway stage. On cache hit, Lambda is not invoked at all — the response is returned directly from the gateway. This is effective for immutable endpoints (`/transactions/{hash}`, `/ledgers/{sequence}`) where the same data is requested repeatedly.

**Staging should disable cache** (`apiGatewayCacheEnabled: false`). At $14.60/mo minimum, the cache cluster costs more than the staging RDS instance ($12/mo) while serving negligible traffic. Cache behavior is validated on production.

## Notes

- API Gateway REST API mode (vs HTTP API) is chosen for response caching, request validation, and WAF integration. HTTP API is cheaper but lacks these features.
- WAF (task 0035) may not be ready when this task starts. The attachment code should be conditional or clearly marked for wiring later.
- This task depends on task 0033 for the API Lambda ARN. The two stacks will have a cross-stack reference.
