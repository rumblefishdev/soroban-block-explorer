# API Gateway stage cache — specification

Spec for the CDK API Gateway stage cache that consumes the `Cache-Control`
headers set by the axum backend (task 0055). Implementation lives in CDK
task 0097.

## Goal

Cache successful responses at the gateway edge for the per-endpoint TTL
the upstream advertises, while never caching errors or the search
endpoint.

## Inputs from the backend

Every successful (`2xx`) response carries one of three `Cache-Control`
values:

- `public, max-age=10` — short tier (lists, network stats, head ledger,
  transaction detail with degraded archive)
- `public, max-age=60` — medium tier (asset / contract / NFT detail, LP
  chart, contract interface)
- `public, max-age=300` — long tier (closed ledgers, finalized
  transaction with full archive overlay)

Search and every non-2xx response carry `Cache-Control: no-store`. The
backend tower middleware
[`enforce_no_store_on_errors`](../../../crates/api/src/common/cache_control.rs)
guarantees this — the gateway can therefore treat `no-store` as the
invariant signal for "do not cache".

The full per-endpoint mapping is documented in
[`backend-overview.md` §6.4](./backend-overview.md#64-response-caching).

## Required gateway behaviour

1. **Stage-level cache: enabled** for the API stage(s) in scope.
2. **Honour upstream `Cache-Control`.** Set the stage so that gateway
   TTL is derived from the response header `max-age` value, not pinned
   stage-side. Don't override headers.
3. **Skip caching when `Cache-Control: no-store`** is present on the
   upstream response (this covers errors + search).
4. **Cache key = full path + every query parameter** (including
   `cursor`, `filter[…]`, `limit`, `q`, etc.). Different filters → distinct
   cache entries. Concretely, configure the stage to hash `path +
normalisedQueryString`. Header-based variation is not used today.
5. **Method scope:** only `GET` (the API has no other public verbs). If
   any future `POST`/`PUT`/`DELETE` is added, exclude it from the
   gateway cache by default.
6. **TTL ceiling:** clamp to 300s. Backend long tier is exactly 300s, so
   the gateway should not extend.
7. **TTL floor (`apiGatewayCacheTtlMutable`):** keep at 10s. The
   backend's short tier is pinned to this value. Lowering below 10s is
   wasted (we'd round-trip more than once per Stellar ledger close);
   raising above 10s is wasted on the short tier specifically.

## What NOT to do

- **Don't cache `4xx` / `5xx`.** The middleware sets `no-store` on every
  non-2xx response, but the stage should also respect this. A defensive
  stage-level "cache-only-200" rule is welcome but not strictly
  necessary if `no-store` honour is correctly wired.
- **Don't cache `/search`.** Same mechanism — backend sets `no-store`.
- **Don't introduce CloudFront** in front of the API. The backend's TTL
  policy assumes a single caching layer. A CloudFront overlay would
  require a re-think of the head-ledger / archive-degraded short
  branches, and is explicitly out of scope per task 0055 and 0097.
- **Don't pin a global stage TTL** that would override per-endpoint
  `max-age`. Long-tier endpoints would either get stale data or
  hot-cache misses depending on which side wins.

## Cache invalidation

No automated invalidation in v1. The TTLs are short enough (≤300s) that
manual purge is rarely needed. If an indexer reindex of historical rows
(tasks 0168/0169/0170 family) materially changes a long-tier response,
operational mitigation is a stage-cache flush — wire that as a runbook
step rather than building automated invalidation here.

## Cache size + eviction

Defer to operator judgement during 0097 sizing. Reasonable starting
point: 0.5 GB per stage with LRU eviction. Adjust after first week of
production traffic based on observed hit rate.

## Verification (post-deploy)

Once 0097 lands the CDK config:

1. Curl a long-tier endpoint twice in succession: assert
   `X-Cache: Hit from cloudfront-or-equivalent` on second call.
2. Curl `/search?q=…` twice: assert no cache hit signal — every call
   reaches origin.
3. Curl an unmatched route: assert response is 404 with
   `Cache-Control: no-store` and no gateway-side caching.
4. Hit `/v1/transactions/:hash` for the same hash twice: if heavy
   archive succeeds both times, assert long-tier cache. If the first
   call degrades, assert short-tier on both (cached for 10s).

## References

- Backend cache-control source of truth:
  [`crates/api/src/common/cache_control.rs`](../../../crates/api/src/common/cache_control.rs)
- Backend overview §6.4: [`backend-overview.md`](./backend-overview.md#64-response-caching)
- Driving task: [`lore/1-tasks/active/0055_FEATURE_backend-api-gateway-caching.md`](../../../lore/1-tasks/active/0055_FEATURE_backend-api-gateway-caching.md)
- Companion CDK task: [`lore/1-tasks/.../0097_FEATURE_…`](../../../lore/1-tasks/) — gateway infrastructure
