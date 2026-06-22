---
id: '0292'
title: 'ETag / 304 conditional GET on head + live lists — cheap idle polls, contract for external API'
type: FEATURE
status: active
related_adr: []
related_tasks: ['0290', '0291']
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

- [ ] Head unchanged → `304`, empty body, **heavy query NOT run** (verified the
      ETag is computed from the cheap head, not from the list).
- [ ] New ledger → `200` + fresh body + new `ETag`.
- [ ] `If-None-Match` / `ETag` / `304` **pass through Cloudflare + API GW**
      (edge auth 0277 does not strip them).
- [ ] An external client (0277) using conditional GET gets a 304 and does not
      read CH.
- [ ] The frontend handles 304 (no error, reuses data) — or confirmed it works
      transparently.
- [ ] Head **shared with [[0291]]** (one head/version provider, no duplication).
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      (cache/HTTP layer) + possibly API docs re ETag. Per
      [ADR 0032](../../2-adrs/0032_docs-architecture-evergreen-maintenance.md).
- [ ] **API types regenerated** — `ETag`/`If-None-Match` are headers, not a
      DTO/route change → likely `N/A`. Confirm; regen if any `crates/api/**`
      contract-layer change. CI gate: `API types freshness`.

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
