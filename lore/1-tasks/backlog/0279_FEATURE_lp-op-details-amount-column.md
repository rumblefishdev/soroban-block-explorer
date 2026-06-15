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
  - date: '2026-06-03'
    status: backlog
    who: stkrolikiewicz
    note: >
      0247 concluded → path decision = **Path B (ingest-side extraction)**.
      Measured on prod CH: per-op collision rate 25% (5.75% per-group), which
      quantifies the "reserve-delta unreliable" note above — a pure-CH-SQL
      snapshot-delta approach (0247 "Path E") caps at ~75% per-op coverage.
      Product requires 100% per-tx amounts, so read-time XDR (Path A) as a
      25%-of-rows hot-path fallback is rejected. Ingest-side extraction reads
      each op's own (non-collapsed) LedgerEntryChanges → 100% per-op, no
      collision, no hot-path S3. Now unblocked. See 0247 notes
      (R-clickhouse-snapshot-delta, S-recommendation).
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

~~**Blocked on the path decision from research 0247**~~ — **RESOLVED
2026-06-03: 0247 selected Path B (ingest-side extraction).** Unblocked.

## Path decision (from 0247)

**Selected: Path B (ingest-side extraction).** Path A (read-time XDR) and the
pure-CH-SQL snapshot-delta shortcut are both rejected:

- The snapshot reserve-delta shortcut (0247 "Path E") is exact only for
  ledgers with a single LP op per pool. Measured per-op collision rate on
  prod CH = **25%** (5.75% per-group; collisions are op-dense) → caps at
  ~75% per-op coverage. This is the quantified version of the
  "reserve-delta unreliable" note above.
- Product requires **100%** per-tx amounts → a 25%-of-rows read-time XDR
  fallback (Path A) on this hot list endpoint is too costly.
- **Path B** reads each op's own (non-collapsed) `LedgerEntryChanges` at
  ingest → 100% per-op, no collision, no hot-path S3. Needs a narrow
  side table + an ADR-0029 clarification (LP-only amounts ≈ single-digit
  MB, not the multi-TB corpus ADR 0029 rejected).

Full reasoning + the concrete build steps (xdr-parser extractor, CH
`lp_operation_amounts` table, indexer persist, backfill) are in 0247's
`notes/R-clickhouse-snapshot-delta.md` and `notes/S-recommendation.md`.

## Implementation (Path B — ingest-side, per 0247)

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

- [x] 0247 path decision recorded — **Path B (ingest-side)**, see above.
- [ ] `?expand=lp_op_details` returns correct per-op amounts (verified vs Horizon
      on a known mixed-direction pool).
- [ ] FE "Amount" column un-hidden and rendering deposit/withdraw/trade shapes.
- [ ] Backward compatible (no param → response shape unchanged); OpenAPI +
      api-types updated.
