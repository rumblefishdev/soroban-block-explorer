---
id: '0317'
title: 'BUG: /contracts/{id}/events → CH Code 241 MEMORY_LIMIT_EXCEEDED (full-table hash join)'
type: BUG
status: completed
related_adr: ['0047']
related_tasks: ['0243', '0290', '0319']
tags:
  [
    'bug',
    'api',
    'clickhouse',
    'contracts',
    'events',
    'priority-high',
    'layer-api',
  ]
links:
  - crates/api/src/contracts/queries_ch.rs
  - infra/src/lib/stacks/api-gateway-stack.ts
history:
  - date: 2026-06-23
    status: active
    who: fmazur
    note: >
      Found in prod during the 0243 flip verification. `/contracts/{id}/events`
      returns 500; CloudWatch shows `CH error in list_events: bad response:
      Code: 241` (MEMORY_LIMIT_EXCEEDED), 12× in the last hour — real
      user-facing failure (the contract-detail Events tab shows "Something went
      wrong"). invocations/interface OK.
  - date: 2026-06-23
    status: completed
    who: fmazur
    note: >
      Fixed `fetch_events` → page-then-key-seek (no full-table joins); 3 new row
      structs; map_event_row reused. Verified E2E on a local CH: the
      12.3M-event contract that OOM'd now returns 200 + paginated events
      (cursor works); SQL A/B under a 500MB cap reproduced the old Code 241
      (FillingRightJoinSide) and the new query passed. cargo test -p api 202
      pass, clippy clean. Bundled the CORS preflight max-age fix
      (api-gateway-stack.ts; CDK builds). NOT yet committed/deployed — two
      stacks: Compute (events) + ApiGateway (CORS). Renumbered from 0314 after a
      pull collision.
  - date: 2026-06-23
    status: completed
    who: fmazur
    note: >
      Post-completion correction from a 5-agent code review (HIGH): the first
      cut kept `FINAL` on the step-1 `soroban_events` seek, which still OOMed
      (Code 241) under the prod 4 GB cap on hot contracts — the earlier "pass"
      was misleading (local API runs as the uncapped `default` user, and parts
      were more merged at that moment). Reproduced FINAL OOM at 500 MB–2 GB;
      DROPPED `FINAL` (full-key `LIMIT 1 BY` already dedups re-ingest, columns
      immutable — same as transactions Statement A), now passes at 500 MB.
      Re-verified: build/clippy/tests green; step-1 returns identical rows
      without FINAL. Two LOW findings accepted: (1) missing-lookup defaulting
      keeps rows the old INNER JOIN dropped (intentional — preserves page
      count; near-impossible given the ledger cap); (2) worst-case page still
      reads ~27M rows without FINAL (bounded, no OOM; events are not polled so
      the read_rows quota risk is low — low-priority follow-up, not yet a task).
---

# BUG: /contracts/{id}/events → CH Code 241 (memory limit)

## Summary

`GET /v1/contracts/{id}/events` returns **500** on prod (CH on). The handler
logs `CH error in list_events: bad response: Code: 241`
(`MEMORY_LIMIT_EXCEEDED`). The Events tab on the contract-detail page is broken
for every contract.

## Root cause

`contracts::queries_ch::fetch_events` hash-joins the **full** `transactions`
(billions of rows) and `ledgers` tables:

```sql
FROM soroban_events se FINAL
JOIN transactions t ON t.id = se.transaction_id AND t.ledger_sequence = se.ledger_sequence
INNER JOIN ledgers l ON l.sequence = se.ledger_sequence
```

ClickHouse builds the join hash side from the **right** table, so even though
the events are a `contract_id` PK seek, the join materialises a hash table over
the entire `transactions` table → OOM (Code 241). This is the exact antipattern
already fixed in transactions Statement A (task 0290): a hash-join over
`transactions`/`accounts`/`ledgers` builds the hash side from the whole table.

## Fix

Mirror `transactions::queries_ch::resolve_source_and_closed_at`: split into
**page-then-key-seek**.

1. Page `soroban_events` by the `contract_id` PK seek + cursor + `LIMIT` — **no
   joins** (returns ledger_sequence, transaction_id, event_index, event_type,
   topics, data).
2. Resolve the page's `transaction_hash` / `successful` from `transactions` via
   a PK-prefix seek (`WHERE ledger_sequence IN (page_seqs) AND id IN (page_ids)
LIMIT 1 BY id`, no FINAL — tx immutable), and `closed_at` from `ledgers
WHERE sequence IN (page_seqs)`. Both bounded to ≤ `limit` distinct keys.

## Acceptance Criteria

- [x] `/contracts/{id}/events` returns `200` for a contract with events (no Code
      241); empty contracts return an empty page. — E2E on local CH: the
      12.3M-event contract (`CAS3…XOWMA`) returns 200 + events.
- [x] Output shape unchanged (same `EventItem` fields, same order, same cursor).
      — `map_event_row` reused verbatim; cursor pagination verified (page 2).
- [x] No full-table hash join; the resolve queries are PK-prefix seeks bounded
      to the page. — SQL A/B under a 500MB cap: old → Code 241
      (FillingRightJoinSide), new → passes.
- [x] `cargo test -p api` green; verified against a local CH. — 202 pass, clippy
      clean.
- [x] **Docs**: `N/A` (no API contract / schema change — query internals only).
- [x] **API types**: `N/A` (no DTO/route change).

## Bundled change — CORS preflight max-age (emergent)

While investigating list-endpoint latency (separate from this bug) the SPA was
seen re-running the CORS `OPTIONS` preflight before **every** `/v1` request:
each call carries `Authorization` (a non-safelisted header), and the API
Gateway preflight had **no `Access-Control-Max-Age`**, so the browser never
cached it — an extra edge round-trip per request. Bundled here per request:
added `maxAge: cdk.Duration.hours(1)` to `defaultCorsPreflightOptions` in
`infra/src/lib/stacks/api-gateway-stack.ts`. The preflight is answered by API
Gateway's MOCK integration (not the Lambda), so the max-age must live on the
gateway, not the axum `CorsLayer`. Deploys via the **ApiGateway** stack
(`make deploy-production-apigateway`), independent of the events fix (Compute).
The deeper list-query cost is tracked separately in [[0319]].

## Notes

- Surfaced by the 0243 flip-verification smoke. The list-only smoke missed
  sub-resources — this is the only erroring endpoint in real prod traffic
  (besides nfts/search which have no CH path at all).
