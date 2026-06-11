---
id: '0287'
title: 'FEATURE: OpenAPI security scheme so Swagger Authorize works (paid-tier)'
type: FEATURE
status: completed
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-small, priority-low, api, openapi, swagger, dx]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: fmazur
    note: 'Spawned from 0277 future work.'
  - date: '2026-06-11'
    status: active
    who: fmazur
    note: 'Promoted to active — implementing on feat/0287 branch.'
  - date: '2026-06-11'
    status: completed
    who: fmazur
    note: >
      Security scheme implemented (api_key x-api-key + bearer_jwt http/JWT, global
      OR; /health exempt via security(())) + api-types regenerated + backend-overview
      doc updated. Verified by a 3-agent review: no bugs (60 schemas preserved, OR
      semantics, /health -> [{}]), no secret/credential leaks, check-generated gate
      passes, SDK change inert at runtime (web client injects JWT via interceptor,
      never sets hey-api `auth`). Live "Try it out -> 200" pending the next
      `make deploy-production-compute` (spec is baked into the API Lambda binary).
---

# OpenAPI security scheme for Swagger Authorize

## Summary

Declare the auth requirement in the OpenAPI spec (utoipa `SecurityScheme`: `x-api-key` apiKey +
http bearer) so Swagger "Authorize" offers a field and "Try it out" works for the gated API.

## Context

After 0277 the API is gated; the spec has NO security scheme, so Swagger "Try it out" gets 401 with
no way to add a key from the UI. `API_BASE_URL` already points Swagger at the CF host.

## Implementation

- Add utoipa security scheme(s) (apiKey x-api-key + http bearer) + `security` on routes.
- **API-types gate applies** (spec changes) → regenerate `libs/api-types/{openapi.json,generated/}`.

## Acceptance Criteria

- [x] OpenAPI spec declares both schemes — `api_key` (`x-api-key` header) + `bearer_jwt`
      (http bearer, JWT) — as a global OR requirement; `/health` opts out via an empty
      per-path requirement. Verified in the extracted spec (`securitySchemes` + root
      `security` + `paths./health.get.security == [{}]`).
- [~] Swagger "Authorize" / "Try it out": the spec is correct so the Authorize dialog
      renders both fields and gated `/v1/*` ops inherit the requirement. Live "returns 200
      with a valid key" pending a running instance (needs a `swagger-ui`-feature build +
      reachable ClickHouse) — verify post-deploy or locally with a DB.
- [x] api-types regenerated (`openapi.json` + `generated/sdk.gen.ts`) — committed in this branch.

## Implementation Notes

- `crates/api/src/openapi/mod.rs` — added `struct SecurityAddon` (`impl utoipa::Modify`)
  registering `api_key` (`ApiKey::Header("x-api-key")`) and `bearer_jwt`
  (`HttpAuthScheme::Bearer`, `bearerFormat: JWT`). Wired via `#[openapi(modifiers(&SecurityAddon),
  …, security(("api_key" = []), ("bearer_jwt" = [])))]`.
- `crates/api/src/ops/mod.rs` — `health` `#[utoipa::path(... security(()))]` to override the
  global requirement (liveness is exempt from the gate, mirrors `auth::is_exempt`).
- `libs/api-types/src/{openapi.json,generated/sdk.gen.ts}` — regenerated via
  `nx run @rumblefish/api-types:generate`; the SDK now stamps `security: [...]` on each `/v1` op.
- `docs/architecture/backend/backend-overview.md` — noted the spec's security schemes (ADR 0032).

## Design Decisions

### From Plan

1. **Two schemes (`api_key` + `bearer_jwt`)** exactly mirroring the gate's accepted credentials
   (paid `x-api-key` OR free-tier session JWT).

### Emerged

2. **Global `security` (OR) instead of per-route annotations.** Every `/v1/*` op is gated, so a
   single root-level requirement is far less churn than annotating each handler, and ops inherit
   it automatically (`security: None` at op level → inherits root). Chose this over touching ~30
   `#[utoipa::path]` blocks.
3. **`/health` exempted with `security(())`.** Keeps the spec accurate (it's the only gated-spec
   path that the auth middleware lets through) rather than leaving it cosmetically marked as
   requiring auth. `/auth/session` needs no change — it isn't registered in the OpenAPI router.
4. **Schemes injected in a `Modify` impl, not the attribute.** utoipa's `security(...)` attribute
   can only *reference* schemes by name; the `ApiKey`/`Http` scheme values must be built in code.

## Issues Encountered

- `nx typecheck` for the web/ui projects fails on pre-existing `@testing-library/*` type errors in
  `*.test.tsx` (UI lib) — confirmed identical on a clean `develop` (stash test), unrelated to this
  change. Not a regression introduced here.
