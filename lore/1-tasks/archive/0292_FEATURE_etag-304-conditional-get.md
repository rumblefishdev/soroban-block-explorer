---
id: '0292'
title: 'ETag / 304 conditional GET on head + live lists — cheap idle polls, contract for external API'
type: FEATURE
status: completed
related_adr: []
related_tasks: ['0290', '0291', '0313']
tags:
  [
    'api',
    'cache',
    'http',
    'clickhouse',
    '0277-external-api',
    'phase-future',
    'priority-medium',
  ]
links: []
history:
  - date: 2026-06-15
    status: backlog
    who: fmazur
    note: 'Spawned from FETCHING_PLAN §4b. Conditional GET (ETag=latest_ledger_sequence) — short-circuit before the heavy query; protects external clients (0277).'
  - date: 2026-06-22
    status: active
    who: fmazur
    note: 'Promoted from backlog to active; set as current task.'
  - date: 2026-06-23
    status: completed
    who: fmazur
    note: >
      Shipped. New `common/conditional.rs` (ETag build + RFC 7232
      If-None-Match match + 304 helper); short-circuit on /network/stats
      (weak ETag) and the live first page of /transactions + /ledgers
      (strong, body-derived ETag); 304 exempted from the no-store
      middleware; CH list inlines the known head instead of re-deriving
      max(sequence). Reuses the 0291 `common/head.rs` probe. 11 files,
      cargo test -p api green (+ conditional unit tests + gated handler
      304 tests + middleware-exempt test), clippy clean, OpenAPI regen.
      Verified E2E on a local CH and deployed to prod (Compute stack),
      Lambda healthy. Two review rounds (round 1: 15 findings, 0
      crit/high; round 2 fixes for weak ETag, body-derived ETag, CH head
      injection, test guard). Spawned 0313 (frontend 304 DevTools verify
      + optional CORS expose-header).
---

# ETag / 304 conditional GET on head + live lists

## Summary

Add a **conditional GET (`ETag` / `If-None-Match` → `304`)** to the head
(`/v1/network/stats`) and the **live lists** (`/v1/transactions?limit=10`,
`/v1/ledgers?limit=10`). `ETag = latest_ledger_sequence`. When the head has not
changed → **`304 Not Modified` (empty body) BEFORE the heavy query runs**.
Result: idle polls are ~free (no body, no heavy CH read), and external clients
(the paid API tier, 0277) whose polling we do not control stop hitting CH
redundantly.

## Context

From FETCHING_PLAN.md §4b (transport). Decision: stay on request/response +
polling, but add a conditional GET (long-poll and WS rejected — see §4b).

- The `LIVE` tier (`crates/api/src/common/cache_control.rs` = `max-age=0,
must-revalidate`) is **already** primed for 304 — `must-revalidate` literally
  means "revalidate"; we only need to add the ETag + `If-None-Match` handling.
- **A different layer than [[0291]]**: 0291 = in-process compute cache; 304 =
  the client↔server HTTP contract (cuts egress + CH read per client, protects the
  CDN + external clients 0277).
- **Relation to 0290**: harm-reduction _now_, while Statement A is unfixed. The
  value of 304 is highest in that short window (before 0290 lands); it drops
  afterwards, but it stays as a contract for the external API.

## Implementation

### Step 1: Shared head/version source (do NOT duplicate 0291)

- The ETag takes the **same cheap head** (`latest_ledger_sequence`) as the
  version-keying in [[0291]]. One "head/version" component serves both. Critical:
  the head is computed cheaply (`max(sequence)` over the PK, or a head cache), not
  by running the heavy list.

### Step 2: Emit ETag

- Head + live lists return `ETag: "<latest_ledger_sequence>"` on 200.
- Keep `cache_control::LIVE` (`must-revalidate`) on those endpoints (already set).

### Step 3: Handle If-None-Match (short-circuit)

- On a request with `If-None-Match`: compare against the current head from the
  cheap source **BEFORE** the heavy query.
- Equal → `304 Not Modified`, empty body, **Statement A does NOT run**.
- Different / header absent → `200` + query + fresh `ETag`.
- CONDITION (otherwise a no-op): if the ETag is computed by running the list
  (35M rows), you only save egress — CH still reads. The short-circuit must come
  first.

### Step 4: Edge passthrough (verify)

- The Cloudflare worker (edge auth, 0277) + API Gateway **must pass through**
  `ETag` / `If-None-Match` / `304` without stripping headers. Verify.

### Step 5: Frontend / client

- The generated client (`@rumblefish/api-types`) + TanStack: a 304 must not be
  treated as an error — the client should **reuse the previous data**. Check
  whether fetch/ETag works transparently, or whether handling must be added.

## Acceptance Criteria

- [x] Head unchanged → `304`, empty body, **heavy query NOT run** — verified
      E2E on a local CH (matching `If-None-Match` → `304` empty body) **and**
      locked by an automated gated handler test asserting the heavy query does
      not run (`list_query_count` audit counter stays unchanged on a `304`).
- [x] New ledger → `200` + fresh body + new `ETag` — verified E2E (inserting
      synthetic ledger 62080000 flips the stats `ETag` to the new head; lists
      stay body-derived).
- [x] `If-None-Match` / `ETag` / `304` **pass through Cloudflare + API GW** —
      verified by code inspection (CF edge only injects `X-Edge-Secret`; API GW
      proxy integration forwards untouched) and the prod Lambda deploy serves
      cleanly. Residual: an explicit through-edge `curl` with a key (folded into
      [[0313]]).
- [x] An external client (0277) using conditional GET gets a `304` and does not
      read CH — server contract is ready (the short-circuit runs before the CH
      read); external clients send their own `If-None-Match`.
- [x] The frontend handles `304` — **no frontend change needed**: the generated
      `@hey-api` client uses the default `fetch` cache mode, so the browser HTTP
      cache revalidates transparently (`ETag` + `must-revalidate`) and the SPA
      never sees a `304`. DevTools spot-check + optional `Access-Control-Expose-Headers: ETag`
      deferred to [[0313]].
- [x] Head **shared with [[0291]]** — both use `crate::common::head` (no
      duplicate head source); list path also reuses the head via
      `current_head_opt`.
- [x] **Docs updated** — `docs/architecture/backend/backend-overview.md` §6.4
      (new conditional-GET subsection). Per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [x] **API types regenerated** — NOT `N/A`: the `304` responses were added to
      the `utoipa` annotations, producing `openapi.json` + `generated/*` diffs
      staged in the same PR. CI gate `API types freshness`: PASS (PR merged).

## Notes

- **Does not replace 0290.** 304 cuts _how often_ you fetch, not the _cost of one
  fetch_. When a client must fetch (new ledger / first paint), Statement A still
  reads 35M rows. The load-bearing CH fix stays 0290.
- Synergy with the frontend §4 (invalidate-on-new-ledger): §4 cuts the request
  count from the SPA; 304 makes every request that does happen (focus, mount,
  external client) cheap. It also resolves the `staleTime:0` +
  `refetchOnWindowFocus:true` contradiction (a focus-refetch returns 304).
- Historical / paginated pages (immutable data) can go further — strong ETag /
  longer `max-age`; the live path here is the priority.

## Implementation Notes

Files (10 changed + 1 new, in one PR on `feat/0292_etag-304-conditional-get`):

- `crates/api/src/common/conditional.rs` **(new)** — `etag_for`/`weak_etag_for`,
  `if_none_match_satisfied` (RFC 7232 §3.2: `*`, `W/`, list, quotes; fail-safe
  on malformed), `not_modified`/`not_modified_weak`, `attach_etag`/`attach_weak_etag`
  - unit tests.
- `crates/api/src/common/mod.rs` — `pub mod conditional;`.
- `crates/api/src/common/cache_control.rs` — `enforce_no_store_on_errors` exempts
  `304 NOT_MODIFIED` (a `304` is a successful conditional response, keeps `LIVE`)
  - test.
- `crates/api/src/common/head.rs` — `current_head_opt` (non-fatal, datasource-aware
  head read for the list short-circuit; swallows errors → serves without ETag).
- `crates/api/src/network/handlers.rs` — `HeaderMap` extractor; `If-None-Match`
  short-circuit after the cheap head read, before `try_get_with`; **weak** ETag
  from `stats.latest_ledger_sequence`.
- `crates/api/src/transactions/handlers.rs` + `ledgers/handlers.rs` — live-first-page
  gate (`cursor.is_none()`; ledgers also `sort == Desc`); short-circuit + **strong,
  body-derived** ETag (newest row's sequence, fallback `live_head`); `live_head`
  threaded into the CH fetch.
- `crates/api/src/transactions/queries_ch.rs` — `fetch_list(head: Option<i64>)`:
  on the live first page inlines the head literal into the partition prune +
  `<= head` cap instead of `(SELECT max(sequence) FROM ledgers)` subqueries
  (cursored pages keep the subquery form).
- `crates/api/src/state.rs` — `#[cfg(test)] list_query_count: Arc<AtomicU64>`
  (audit counter for the 304-skips-query test; compiled out of release).
- `docs/architecture/backend/backend-overview.md` — §6.4 conditional-GET section.
- `libs/api-types/{openapi.json,generated/*}` — `304` responses regen.

Verification: `cargo test -p api` green (conditional unit tests + 2 gated handler
304 tests + middleware-exempt test), clippy `-D warnings` clean, OpenAPI fresh.
E2E on a local CH (25k-ledger backfill, head 62079999): 200+ETag, 304 on weak &
strong `If-None-Match`, cursored → no ETag, `order=asc` → no ETag, new-ledger
freshness. Deployed to prod via `make deploy-production-compute`; Lambda
`production-soroban-explorer-api` Active/healthy, clean cold start.

## Issues Encountered

- **`enforce_no_store_on_errors` would have stamped `no-store` on the `304`.**
  `304` is not 2xx, so the blanket non-2xx → `no-store` middleware would have
  broken the conditional-GET contract. Fixed by exempting `NOT_MODIFIED`. Caught
  before any deploy (pre-implementation read of the middleware).
- **Local CH was down at verification time** (a day had passed; `docker compose ps`
  empty). Restarted with the existing volume — data persisted (25k ledgers).
  The transient `500`s in the log were CH connection-refused, not the new code.
- **API overloaded (`529`) during the agentic sanity review** — both 8-agent runs
  failed server-side; substituted a concrete inline grep-based audit (clean: only
  ETag + Cache-Control on responses; raw errors only in server logs; CH password
  is a separate field, never in the URL).

## Broken/modified tests

- No existing tests broken. Added: `conditional` unit tests; `cache_control`
  `middleware_exempts_304_from_no_store`; gated `conditional_tests` in
  `transactions`/`ledgers` handlers (304 short-circuit + counter; `order=asc`
  no-ETag).

## Design Decisions

### From Plan

1. **ETag = `latest_ledger_sequence`, short-circuit before the heavy query**,
   reusing the cheap `common/head` probe shared with [[0291]].
2. **Scope = live first page only.** Cursored/historical pages are head-independent
   (excluded); ledgers `order=asc` (oldest, immutable) also excluded.
3. **Keep `cache_control::LIVE` (`must-revalidate`)** on the conditional endpoints.

### Emerged

4. **Weak ETag for `/network/stats`, strong for the lists** (round-2 review fix).
   The stats body carries `generated_at`, so the same head can yield byte-different
   bodies — a strong validator would violate RFC 7232 §2.1. Lists are byte-stable
   per head.
5. **List ETag derived from the body** (newest returned row), not the pre-query
   head — so a strong validator always equals the bytes sent even if a ledger
   lands mid-query.
6. **`304` exempted from the no-store middleware** (see Issues).
7. **CH `fetch_list` takes `head: Option<i64>`** and inlines it on the live page —
   drops the duplicate `max(sequence)` subqueries and pins the candidate scan to
   the ETag'd head (review #5/6).
8. **No frontend code** — the `@hey-api` client's default `fetch` cache mode lets
   the browser HTTP cache revalidate transparently; the SPA never sees a `304`.
   DevTools confirmation + optional `Access-Control-Expose-Headers: ETag` → [[0313]].
9. **`#[cfg(test)]` query-audit counter** to prove the `304` skips the heavy query
   (a status-only test would pass even if the short-circuit regressed).

## Future Work

- **[[0313]]** — verify 304 transparency in the frontend (DevTools), optionally add
  `Access-Control-Expose-Headers: ETag` + an explicit through-edge `curl` smoke.
- Historical / cursored pages: a content/long-lived ETag (out of scope here).
