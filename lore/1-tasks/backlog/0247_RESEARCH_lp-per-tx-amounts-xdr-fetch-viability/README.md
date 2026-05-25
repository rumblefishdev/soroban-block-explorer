---
id: '0247'
title: 'RESEARCH: LP per-tx amounts — XDR archive fetch viability + alternatives'
type: RESEARCH
status: backlog
related_adr: ['0011', '0027', '0029', '0037']
related_tasks: ['0077', '0163', '0169', '0199', '0246']
tags:
  [
    priority-medium,
    effort-medium,
    layer-api,
    layer-arch,
    layer-research,
    milestone-2,
  ]
milestone: 2
links: []
history:
  - date: 2026-05-20
    status: backlog
    who: karolkow
    note: >
      Split from 0246 Phase 4 (originally bundled). Per-tx LP amounts
      (deposit_a/b, withdraw_a/b, trade direction) need to land in
      `GET /liquidity-pools/:id/transactions` to support Figma "Recent
      transactions" amount column. Default proposal was XDR archive
      read-time fetch (ADR 0029 pattern). Latency on hot read path is
      the open question — 8–10 distinct ledgers per 20-row page × 50–
      150 ms each = 0.5–1.5 s typical, 1.5–3 s p99. Worth benchmarking
      before commit. Alternatives (narrow ADR 0029 exception for LP
      ops, ingest-side extraction) also on the table.
  - date: 2026-05-20
    status: backlog
    who: karolkow
    note: >
      Renumbered 0242 → 0247. Origin commit c82c9fa (M1-M3 sequencing
      plan, merged concurrently) had already grabbed 0242 for the
      ADR ratify ClickHouse-primary task. Sister feature task renumbered
      0241 → 0246 in the same operation. Original spawn commits
      (cac0215, ddbbb34) retain the `lore-0241` scope tag —
      not amended per no-amend convention.
---

# RESEARCH: LP per-tx amounts — XDR archive fetch viability + alternatives

## Summary

The frontend liquidity pool detail page (task 0077, per Figma) needs
per-transaction LP-specific amounts in the "Recent transactions"
section: trades as `100 XLM → 40 USDC`, deposits as
`5,000 XLM + 2,000 USDC`, withdrawals as `10,000 XLM + 4,000 USDC`. Per
ADR 0029, per-operation stroop amounts are intentionally not stored in
the DB — `operations_appearances.amount` is a **fold count**, not a
transfer amount (per task 0163 + 0169 audit). The canonical read-time
solution is XDR archive fetch + parse, but on the hot read path of a
list endpoint the latency cost is non-trivial. This research evaluates
the XDR-fetch baseline against alternatives and produces a
recommendation with measured numbers.

## Status: Backlog

**Current state:** Not started. Spawned from 0246 split on 2026-05-20.

## Open question

**Where should per-tx LP amounts come from in
`GET /liquidity-pools/:id/transactions`?**

Sub-questions:

1. What is the measured S3 latency for `aws-public-blockchain` archive
   fetch from our AWS region(s)? p50 / p95 / p99?
2. What is the realistic cache hit rate for ledger XDR? (multiple tx in
   the same ledger; popular pools concentrate activity)
3. Egress cost? Is the bucket truly free outbound for our compute
   region?
4. Worst-case page (20 rows × 20 distinct ledgers) — user-visible
   latency budget acceptable?
5. Would a narrow exception to ADR 0029 (extract + persist LP-op
   amounts only at ingest) be acceptable to senior backend / ADR
   authors? LP ops are a tiny subset of total op volume.
6. Hybrid: indexer extracts LP amounts during XDR parse + writes to a
   narrow side table; API reads from that without violating the
   ADR 0029 spirit for non-LP ops?

## Candidate paths

### Path A — XDR archive read-time fetch (baseline, ADR-aligned)

- **What:** Server, on `?expand=lp_op_details`, batches S3 GETs per
  unique `ledger_sequence` in the result set, dekompresses (zstd),
  parses XDR + result_meta_xdr, extracts LP-specific amounts, merges
  into response.
- **Pros:** Zero schema change. Reuse E3 (`GET /transactions/:hash`)
  infrastructure. Backward compatible (without param, response shape
  unchanged). Per ADR 0029 by-the-book.
- **Cons:** Hot-path latency. S3 dependency. Cold cache punishes worst
  case.
- **Estimated cost:** Reuse — small (~3–5 days).

### Path B — Narrow ADR 0029 exception: LP-only amounts in DB

- **What:** Add a `lp_operation_amounts` table (or extend
  `operations_appearances` with NULLABLE `lp_amount_a`, `lp_amount_b`,
  `lp_direction` columns, populated only for LP op types).
- **Pros:** Sub-millisecond SELECT. No S3 on hot path. No cache
  complexity.
- **Cons:** Schema migration. **ADR 0029 amendment required** (formal
  process — RFC + senior buy-in). Backfill across full mainnet history
  for LP ops (manageable — LP ops are rare, ~tens of thousands of rows
  total, not millions). Sets precedent for further exceptions.
- **Estimated cost:** Larger — schema + backfill + ADR work.

### Path C — Hybrid: indexer-side extraction + side table

- **What:** Indexer parses LP amounts during its normal XDR pass (it
  already touches every op for fold-count purposes per task 0163).
  Writes a narrow side table with columns: transaction_id, op_index,
  pool_id, amount_a, amount_b, direction. API reads from that side
  table when `expand=lp_op_details` is set.
- **Pros:** No hot-path S3. No re-parsing (indexer already parses each
  op). Cleaner DB schema separation than Path B (LP-only side table,
  not coupling to `operations_appearances`).
- **Cons:** Still ADR 0029 amendment / exception. Backfill required.
  Indexer code change + ingest pipeline review.
- **Estimated cost:** Medium — indexer work + backfill + ADR.

### Path D — Drop the column from MVP

- **What:** Frontend renders Transactions section without amounts —
  type badge + truncated hash + account + relative time. Add amounts
  back in v2 once path decision made.
- **Pros:** Ships fastest. Zero backend work.
- **Cons:** UX regression vs Figma. User-visible "less detail than
  Horizon / stellar.expert".

## Research plan

1. **Benchmark Path A end-to-end.** Set up a local test that fetches
   N=20 ledger XDR files from `aws-public-blockchain`, decompresses,
   parses, extracts LP amounts. Measure p50 / p95 / p99 from our AWS
   compute region(s). Test both warm-cache and cold-cache scenarios.
   Document in `notes/R-s3-fetch-benchmark.md`.
2. **Quantify cache hit rate.** Analyze a representative production
   slice of `liquidity_pool_transactions` query patterns to estimate
   how often the same ledger XDR would be re-requested (e.g., pages 1–
   N of a popular pool; cross-pool overlap). Document in
   `notes/Q-cache-hit-rate.md`.
3. **Verify egress cost.** Confirm with AWS billing / SDF docs that
   `aws-public-blockchain` bucket is requester-pays-free for our
   region. Document in `notes/Q-egress-cost.md`.
4. **Storage cost of Path B/C.** Count LP ops in full mainnet history.
   Multiply by row size (~80 bytes). Project growth rate. Compare
   against ADR 0029 rationale ("DB size explodes"). Document in
   `notes/R-lp-op-storage-cost.md`.
5. **Sound out senior backend / 0163 authors.** Bring Path B/C
   proposal — would they accept a narrow exception? What
   precedent-setting concerns? Document in
   `notes/R-team-feedback.md`.
6. **Synthesis.** Compare paths on (latency, complexity, schema
   impact, ADR posture, ship time). Recommend one in
   `notes/S-recommendation.md`.

## Acceptance Criteria

- [ ] Benchmark data captured for Path A (p50 / p95 / p99, warm + cold)
- [ ] Cache hit rate estimate documented with method
- [ ] Egress cost verified (bucket terms, our region)
- [ ] Storage cost estimate for Path B/C (rows, GB, growth rate)
- [ ] Team feedback summarized (acceptance of narrow exception)
- [ ] Synthesis note with explicit recommendation + reasoning
- [ ] If recommendation ≠ Path A: pre-draft ADR amendment / new ADR
- [ ] Spawn implementation task with chosen path (FEATURE, references
      this task as parent)

## Notes

- Frontend FE 0077 can ship without amount column for MVP (Path D as
  interim). 0247 unblocks the proper amount column.
- 0199 (LP analytics, blocked-on-oracle) is orthogonal — that task
  aggregates USD volume per snapshot, not per-tx amounts. No overlap.
- Indexer already parses every op for fold-count + identity tuple
  (task 0163). Path C reuses that pass — incremental cost is low.
- ADR 0029 explicitly rejected the original `operations.transfer_amount`
  column. Path B/C re-opens that conversation but **only for LP ops**,
  which are a tiny subset (vs CREATE_CLAIMABLE_BALANCE worst-case
  102k rows that triggered 0163).

## Future work

- Implementation task (spawn after research conclusion) — exact path
  TBD.
- Cache strategy refinement if Path A wins (LRU sizing, eviction
  policy).
- If Path B/C wins: backfill plan for historical LP ops.
