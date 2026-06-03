---
id: '0247'
title: 'RESEARCH: LP per-tx amounts — XDR archive fetch viability + alternatives'
type: RESEARCH
status: active
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
  - date: 2026-06-02
    status: active
    who: stkrolikiewicz
    note: >
      Activated to begin research. Plan: benchmark Path A first
      (cheap kill/confirm gate, reuses stellar_archive E3 client),
      then re-evaluate ADR 0029 "DB size explodes" rationale under
      the now-merged ClickHouse-primary read path (commits 236/237/243),
      which may shift the calculus toward Path C (ingest-side extraction).
  - date: 2026-06-03
    status: active
    who: stkrolikiewicz
    note: >
      Reframed after code investigation (no CH/network access used —
      static analysis only). Two findings: (1) Path A is not a free E3
      reuse — actual LP amounts live in result-meta LedgerEntryChanges,
      not the op body; trade direction needs OperationResult, which
      xdr-parser does not expose (new extractor required). (2) NEW Path E:
      liquidity_pool_snapshots already stores reserve_a/b per
      (pool_id, ledger_sequence); lagInFrame delta joined to
      operations_appearances yields per-tx amounts in pure CH SQL with no
      XDR fetch / no new table / no extractor — exact only for ledgers with
      one LP op per pool. Central question pivoted from "S3 latency" to
      "Path E collision-rate coverage". README reframed, Path E added,
      research plan reprioritized (E1 collision-rate gate first, Path A
      benchmark demoted to fallback fraction). Notes added:
      R-xdr-amount-source, R-clickhouse-snapshot-delta, S-recommendation,
      plus 2 explanatory SVG diagrams. Next: run E1/E2 against prod CH
      (needs mTLS bundle).
  - date: 2026-06-03
    status: active
    who: stkrolikiewicz
    note: >
      CONCLUDED. Collision gate measured on prod CH: 5.75% per-group, but
      25% per-op (collisions are op-dense), 9.5% per-group on hot top-50.
      Path E therefore caps at ~75% per-op coverage. Product decision came
      back must-have 100% per-tx amounts → Path E and Path-E+degrade ruled
      out; a 25% Path-A XDR hot-path fallback rejected as too costly.
      Verdict: Path C (ingest-side per-op extraction from each op's own,
      non-collapsed LedgerEntryChanges → 100%, no collision, no hot-path
      S3). Path decision recorded in the existing task 0279 (spawned from 0274). Recommendation
      delivered; research goal met.
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

## Status: CONCLUDED 2026-06-03 → Path C (impl = task 0279)

**Verdict:** measured per-op collision rate = **25%** (prod CH) → Path E
(pure CH SQL) caps at ~75% per-op coverage. Product requires **100%**
per-tx amounts (Figma must-have), and a 25% Path-A XDR hot-path fallback is
too costly. **Selected path: C** — extract per-op LP amounts at ingest from
each op's own (non-collapsed) `LedgerEntryChanges`. Implementation spawned
as **task 0279** (FEATURE). Full reasoning in `notes/S-recommendation.md` +
`notes/R-clickhouse-snapshot-delta.md`.

The reframe + path analysis below is retained for the decision trail.

## Reframe — 2026-06-03 (READ FIRST)

The original framing below treats this as a **Path A latency benchmark**
("is read-time XDR fetch fast enough?"). Code investigation this session
shifts the centre of gravity. Two findings (see `notes/`):

1. **Path A is not a free reuse of E3.** Actual LP amounts are NOT in the
   operation body (`extract_operations` LP `details` carry only request
   bounds — `maxAmountA/B`, `minAmountA/B`). The executed amount lives in
   the **result-meta `LedgerEntryChanges`** (pre/post pool reserve delta);
   trade direction needs the **operation result**, which `xdr-parser` does
   not expose today. So Path A needs a new extractor and pays a full
   result-meta diff per request. (`notes/R-xdr-amount-source.md`.)

2. **ClickHouse already holds the reserve time-series → a pure-SQL path
   (Path E) exists.** `liquidity_pool_snapshots` stores post-state
   `reserve_a/reserve_b` per `(pool_id, ledger_sequence)`. The delta
   between consecutive snapshots (`lagInFrame` window) = net amount moved;
   joined to `operations_appearances(pool_id, transaction_id)` it yields
   per-tx amounts with **no XDR fetch, no new table, no new extractor** —
   _for ledgers with exactly one LP op per pool_.
   (`notes/R-clickhouse-snapshot-delta.md`.)

**The new central question is no longer "S3 latency" — it is:**

> What fraction of LP ops are the **sole LP op on their pool in their
> ledger** (= Path E exact-coverage), and does the snapshot reserve delta
> equal the true amount? The S3 benchmark now only governs the **fallback
> fraction** (colliding ledgers), not the main path.

See `notes/S-recommendation.md` for the current lean (Path E primary →
Path A fallback for collisions → Path D as MVP interim).

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

### Path E — Pure ClickHouse SQL: snapshot reserve deltas (NEW, 2026-06-03)

- **What:** Compute per-tx amounts entirely in SQL from data already in CH.
  `reserve_a/b` deltas via `lagInFrame() OVER (PARTITION BY pool_id ORDER BY
ledger_sequence)` on `liquidity_pool_snapshots`, joined to
  `operations_appearances` (`pool_id, transaction_id, application_order`) to
  attribute the delta to the tx/op that touched the pool. Direction from
  delta signs (both + = deposit, both − = withdraw, opposite = trade).
- **Pros:** No XDR archive fetch. No new table. No new indexer extractor. No
  ADR 0029 amendment (reads existing columns). Sub-ms SELECT. Reuses data
  ingested for the chart endpoint.
- **Cons:** **Per-(pool, ledger) granularity, not per-tx.** Exact ONLY when a
  ledger has a single LP op on that pool — `uq_lp_snapshots_pool_ledger DO
NOTHING` keeps only the ledger's final post-state, so multiple LP ops on
  one pool in one ledger collapse to a net sum that cannot be split per-op.
  Coverage = the non-colliding fraction; likely lower on hot pools (frequent
  path-payment trades). Edge cases: `state` Δ=0 rows, pool creation (no prior
  snapshot), fee accrual / Decimal128 scale in the delta.
- **Estimated cost:** Small — a query + cursor wiring. The risk is coverage,
  not effort.
- **Posture:** primary for single-op ledgers; **Path A as the fallback** for
  the colliding minority. If collisions dominate hot pools, escalate to
  Path C (true per-op amounts at ingest, no collision problem).

### Path D — Drop the column from MVP

- **What:** Frontend renders Transactions section without amounts —
  type badge + truncated hash + account + relative time. Add amounts
  back in v2 once path decision made.
- **Pros:** Ships fastest. Zero backend work.
- **Cons:** UX regression vs Figma. User-visible "less detail than
  Horizon / stellar.expert".

## Research plan

> **Reprioritized 2026-06-03.** Path E (CH SQL) is now the lead candidate;
> the steps below are reordered so the Path E coverage gate runs FIRST and
> the Path A benchmark is demoted to "fallback-fraction only". New lead
> steps (need CH mTLS access):
>
> - **E1 — Collision rate.** Query CH: of all `(pool_id, ledger_sequence)`
>   groups in `operations_appearances WHERE pool_id IS NOT NULL`, what % have
>   `>1` LP op? Weight by traffic (hot pools dominate views). This is the
>   Path E exact-coverage number and the primary go/no-go.
> - **E2 — Delta validation.** For a sample of single-op ledgers, compare the
>   snapshot `lagInFrame` reserve delta against the true amount parsed from
>   XDR via the `compare-with-stellar-api` skill. Confirms delta = amount
>   moved (watch fee accrual, rounding, Decimal128 scale).
> - **E3 — Edge cases.** `state` Δ=0 rows, pool creation (LAG null), withdraw
>   to zero, fee-only reserve drift.
> - **E4 — Direction rule.** Validate the delta-sign → deposit/withdraw/trade
>   mapping against known txs.
>
> The original Path A benchmark (step 1 below) is now scoped to the
> **colliding fallback fraction** surfaced by E1, and must measure the
> heavier **full result-meta diff** cost (not the lighter single-tx E3
> extract the latency note assumed).

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

- [x] **Path E collision rate measured** — 5.75% per-group, **25% per-op**,
      9.5% per-group hot top-50 (prod CH, 2026-06-03)
- [ ] ~~Path E delta validated against XDR (E2)~~ — moot; Path E not
      selected. Delta-vs-XDR validation moved to 0279 acceptance (Path C
      uses the same per-op reserve-diff logic).
- [ ] ~~Path E edge cases + direction rule (E3, E4)~~ — moved to 0279.
- [ ] ~~Path A benchmark (p50/p95/p99)~~ — moot; Path A not selected
      (25% hot-path XDR rejected).
- [x] **Recommendation produced** — Path C, driven by the 25% collision
      measurement + the 100% product requirement
- [x] **Path decision recorded in existing impl task** — 0279 (spawned from 0274)
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
