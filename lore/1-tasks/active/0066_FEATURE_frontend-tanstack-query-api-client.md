---
id: '0066'
title: 'Frontend: TanStack Query setup, API client, polling, env config'
type: FEATURE
status: active
related_adr: []
related_tasks: []
tags: [priority-high, effort-small, layer-frontend-shared]
milestone: 2
links: []
history:
  - date: 2026-03-24
    status: backlog
    who: fmazur
    note: 'Task created'
  - date: 2026-05-07
    status: active
    who: FilipDz
    note: 'Activated'
  - date: 2026-05-11
    status: active
    who: FilipDz
    note: 'Implemented under web/src/api/ (scaffold lives in web/, not apps/web/). Built on top of the already-generated @hey-api/openapi-ts @tanstack/react-query plugin output in libs/api-types — Step 6 hooks are thin wrappers over generated queryOptions/infiniteQueryOptions plus per-resource staleTime/refetchInterval from polling.ts. Step 4 queryKeys.ts exposes invalidateResource() predicates over the generated _id-prefixed keys instead of duplicating the key structure.'
  - date: 2026-05-11
    status: active
    who: FilipDz
    note: 'Env config refined per PR review: only web/.env.development + web/.env.example are committed. Staging/production VITE_API_BASE_URL is passed by the deployment pipeline (CI/CDK) at build time so the URL is owned by infra, not the repo. config.ts already fails fast at runtime when the variable is missing.'
---

# Frontend: TanStack Query setup, API client, polling, env config

## Summary

Set up the TanStack Query provider, typed API client, polling configuration, and environment-based API URL injection in `apps/web/src/api/`. TanStack Query IS the browser cache for all server state -- no Redux, Zustand, or manual global cache layer is permitted.

## Status: Backlog

**Current state:** Not started.

## Context

The explorer frontend is a read-heavy SPA consuming a REST API. TanStack Query provides request de-duplication, background refetching, stale-state handling, and caching out of the box. Every page and section in the explorer fetches data through this layer.

API base URL: injected from environment variable at build time. Separate configs for dev, staging, and production. The app is deployed as a static SPA via CloudFront.

Security: no API keys in the bundle. The frontend is an anonymous public client. Protection is at the API Gateway/WAF layer.

Stale time configuration:

- Home page: 10-30 seconds + polling
- List pages: 60 seconds
- Detail pages: 5 minutes
- Search: no cache, debounced at approximately 300ms

Query key structure: `[resourceType, identifier?, { filters?, cursor? }]`

Independent section fetching: detail pages issue separate queries per section (e.g., account summary and account transactions are separate queries).

## Implementation Plan

### Step 1: API client setup

Create `apps/web/src/api/client.ts`:

- Base HTTP client (fetch or axios) with API base URL from env var
- Typed request/response helpers
- Error response parsing (extract status code, error message, error type)
- No API keys or auth headers

### Step 2: Environment configuration

Create `apps/web/src/api/config.ts`:

- Read API base URL from `import.meta.env.VITE_API_BASE_URL` (or equivalent)
- Validate URL at startup
- Dev/staging/prod environment configs via `.env` files

### Step 3: TanStack Query provider

Create `apps/web/src/api/QueryProvider.tsx`:

- Configure `QueryClient` with default options:
  - Request de-duplication enabled (default)
  - Background refetching enabled
  - Retry: 1 retry for 5xx/network errors, no retry for 4xx
  - Default stale time: 60 seconds (overridden per query)
- Wrap app root with `QueryClientProvider`

### Step 4: Query key factory

Create `apps/web/src/api/queryKeys.ts`:

- Structured query key factory per resource type
- Pattern: `[resourceType, identifier?, { filters?, cursor? }]`
- Examples:
  - `queryKeys.transactions.list({ cursor, filters })` -> `['transactions', { cursor, filters }]`
  - `queryKeys.transactions.detail(hash)` -> `['transactions', hash]`
  - `queryKeys.accounts.detail(id)` -> `['accounts', id]`
  - `queryKeys.accounts.transactions(id, { cursor })` -> `['accounts', id, 'transactions', { cursor }]`

### Step 5: Polling configuration

Create `apps/web/src/api/polling.ts`:

- Home page queries: `refetchInterval: 10000-15000` (10-15 seconds)
- List page queries: no automatic polling (refetch on window focus)
- Detail page queries: no polling (long stale time)
- Search queries: no polling, no cache (`cacheTime: 0`)

### Step 6: Typed query hooks (base patterns)

Create `apps/web/src/api/hooks/` with base hook patterns:

- `useNetworkStats()` -> GET /network/stats
- `useTransactionsList(filters, cursor)` -> GET /transactions
- `useTransactionDetail(hash)` -> GET /transactions/:hash
- Pattern established for all other resource types to follow

## Acceptance Criteria

- [x] API client reads base URL from environment variable, no hardcoded URLs
- [x] No API keys or auth tokens in the frontend bundle
- [x] TanStack Query provider wraps app root with configured QueryClient
- [x] Request de-duplication works by default (identical queries share one request)
- [x] Background refetching enabled
- [x] Stale times configured: Home (10-30s), Lists (60s), Detail (5min), Search (no cache)
- [x] Query keys follow structured pattern: `[resourceType, identifier?, { filters?, cursor? }]` — codegen emits `[{ _id, path, query, ... }]` which is a single structured shape across all queries; `queryKeys.ts` wraps it with `matchResource()`/`invalidateResource()` for resource-level invalidation
- [x] Home page queries poll at 10-15 second intervals (`homePolicy.refetchInterval = 12_000`)
- [x] Detail pages issue separate queries per section (pattern via independent wrappers per sub-resource)
- [x] Environment configs exist for dev, staging, and production — `web/.env.development` is committed for zero-config local dev; staging/production `VITE_API_BASE_URL` is injected at build time by CI/CDK so the URL lives in the deployment pipeline (single source of truth = infra) and isn't hardcoded in the repo. `config.ts` fails fast at runtime if the variable is missing.
- [x] TanStack Query is the sole browser cache -- no Redux/Zustand/manual global state for server data
- [x] **Docs updated** — [docs/architecture/frontend/frontend-overview.md](../../../docs/architecture/frontend/frontend-overview.md) §8.1 "Implementation Layout" added per [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md). Other `docs/architecture/**` files: N/A — task is frontend-only, does not change backend/schema/infra shape.
- [x] **API types regenerated** — `nx run @rumblefish/api-types:check-generated` passes. The only edit under `libs/api-types/**` was a hand-written re-export of `client` in [src/index.ts](../../../libs/api-types/src/index.ts) (not under the gated `src/openapi.json` or `src/generated/**` paths). `openapi.json` and codegen output unchanged.

## Notes

- TanStack Query replaces any need for global state management for server data. Local UI state (modals, form inputs) can use React state.
- The query key factory is critical for cache invalidation predictability across the app.
- All page-specific query hooks will be implemented in their respective page tasks (0067-0087).
- The API client should integrate with the error classification utility from task 0064 for consistent error handling.
