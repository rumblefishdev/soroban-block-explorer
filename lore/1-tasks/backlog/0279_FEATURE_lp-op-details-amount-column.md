---
id: '0279'
title: 'LP per-op amounts: wire ?expand=lp_op_details + un-hide Amount column'
type: FEATURE
status: backlog
related_adr: ['0029']
related_tasks: ['0274', '0247']
tags:
  [
    phase-future,
    effort-medium,
    priority-medium,
    layer-api,
    layer-frontend,
    milestone-2,
  ]
links: []
history:
  - date: '2026-06-03'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0274 future work (gap #2). 0274 closed with #2 deferred
      after a deep-dive confirmed per-op LP amounts are not in the DB and
      cannot be served cheaply today. Blocked on the path decision from
      research 0247 (read-time XDR fetch vs ingest-side extraction).
---

# LP per-op amounts: wire `?expand=lp_op_details` + un-hide Amount column

## Summary

Gap #2 of the FE→API audit (task 0274). `GET /v1/liquidity-pools/{pool_id}/transactions`
must return per-operation LP amounts so the FE pool-tx table's **"Amount"**
column (currently intentionally hidden) can show
`deposit 5,000 XLM + 2,000 USDC` / `trade 100 XLM → 40 USDC` / withdrawals.

## Context

Spawned from **0274** (closed; #2 deferred). Deep-dive (recorded in 0274
Issues) confirmed per-op amounts are genuinely absent:
`operations_appearances.amount` is a fold count (ADR 0029), `xdr_parser` has
no claimedOffers/deposit/withdraw extraction, the LP-tx endpoint is DB-only,
and reserve-delta is unreliable (multi-op/ledger netting). So this is a real
feature, not a field add.

**Blocked on the path decision from research 0247** (read-time S3 XDR fetch vs
ingest-side extraction + narrow side table).

## Implementation (after 0247 picks a path)

- **Parser**: add LP-op amount extraction to `xdr_parser` — `claimedOffers[]`
  (PathPayment, op types 2 + 13) for swaps + `LiquidityPoolDeposit`/`Withdraw`
  amounts. Bidirectional swap normalization (see 0199 §Phase 1).
- **Path A (read-time)**: wire the archive-fetch + parse + merge layer into the
  LP-tx handler behind `?expand=lp_op_details` (reuse `runtime_enrichment/stellar_archive`,
  as the contract-events endpoint does). Backward compatible — absent param → unchanged shape.
- **Path B (ingest-side)**: indexer extracts at parse time → narrow side table;
  API reads from DB. Needs schema + an ADR-0029 exception.
- **Response**: `lp_operation_detail { operation_type, amount_a, amount_b }` per row.
- **FE**: un-hide the "Amount" column in the pool-tx table; render the new shape.
- OpenAPI + `libs/api-types` regen.

## Acceptance Criteria

- [ ] 0247 path decision recorded.
- [ ] `?expand=lp_op_details` returns correct per-op amounts (verified vs Horizon
      on a known mixed-direction pool).
- [ ] FE "Amount" column un-hidden and rendering deposit/withdraw/trade shapes.
- [ ] Backward compatible (no param → response shape unchanged); OpenAPI +
      api-types updated.
