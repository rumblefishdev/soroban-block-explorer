---
id: '0287'
title: 'FEATURE: OpenAPI security scheme so Swagger Authorize works (paid-tier)'
type: FEATURE
status: backlog
related_adr: ['0048']
related_tasks: ['0277']
tags: [phase-future, effort-small, priority-low, api, openapi, swagger, dx]
links: []
history:
  - date: '2026-06-10'
    status: backlog
    who: claude
    note: 'Spawned from 0277 future work.'
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
- [ ] Swagger "Authorize" accepts x-api-key/Bearer; "Try it out" returns 200 with a valid key
- [ ] api-types regenerated + committed
