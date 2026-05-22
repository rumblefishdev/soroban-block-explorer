---
id: '0252'
title: 'VALIDATION: ClickHouse endpoint parity against Horizon / stellar.expert'
type: FEATURE
status: active
related_adr: ['0044']
related_tasks: ['0207', '0228']
tags: [priority-high, effort-medium, layer-api, validation]
milestone: 1
links:
  - docs/architecture/database-schema/endpoint-queries-clickhouse/README.md
  - docs/runbooks/0228_phase6_validation.md
  - docs/runbooks/artifacts/phase6_validation_20260521.md
  - .claude/skills/compare-with-stellar-api/SKILL.md
history:
  - date: '2026-05-21'
    status: active
    who: stkrolikiewicz
    note: >
      Spawned from task 0228 Phase 6 wrap-up. After the merged Hetzner
      CH passed sample-compare Tier 5 (hash-set 205/205 on probed ledger,
      tx-count 0/32 mismatch across 32 stratified ledgers), the next
      validation layer is per-endpoint parity — taking each of the 23
      hand-tuned ClickHouse read queries under
      `docs/architecture/database-schema/endpoint-queries-clickhouse/`
      and confirming the response shape + field values match Horizon
      (REST API) and stellar.expert (Soroban-only contracts) within
      documented tolerances. Task 0207 shipped the reference SQL but
      explicitly deferred Tier 2-4 validation to a follow-up gated on
      CH writer + populated data; both prerequisites are now satisfied
      (0228 Phase 5 + full Soroban-era backfill landed).

      Intentionally not blocked_by 0228 — 0228 closes in parallel with
      its smoke Phase 6 verdict (acceptance criteria already met); this
      task does the deeper per-endpoint sweep that 0207 deferred. Both
      can land independently.
---

# VALIDATION: ClickHouse endpoint parity against Horizon / stellar.expert

## Summary

Run the 23 ClickHouse endpoint queries under
`docs/architecture/database-schema/endpoint-queries-clickhouse/` against
the merged Hetzner CH (post-0228 Phase 5) and compare their output —
field by field — against the canonical reference data sources: Horizon
REST API for classic Stellar entities (ledgers, transactions, accounts,
assets, liquidity pools) and stellar.expert for Soroban-only entities
(contracts, events, invocations). Capture per-endpoint pass / fail in a
validation artifact and fix any divergences that turn up.

## Status: Active

**Current state:** Task spawned 2026-05-21 from 0228 Phase 6 wrap-up.
Plan drafted, no code yet.

## Context

### Where the gap comes from

Task 0207 shipped the canonical CH endpoint SQL (23 files, one per
public REST route per `backend-overview.md §6.2`) but explicitly
deferred Tier 2-4 validation:

| Tier | What                                                                       | 0207 status |
| ---- | -------------------------------------------------------------------------- | ----------- |
| 1    | Schema parse — `clickhouse-client --format=Null` exit 0                    | **Done**    |
| 2    | Row-count equivalence — same params against PG vs CH within tolerance      | Deferred    |
| 3    | Sample-row diff — column-by-column PG vs CH on 10 random keys per endpoint | Deferred    |
| 4    | Aggregate equivalence — totals PG vs CH on aggregating queries             | Deferred    |

The deferral was gated on (a) the CH writer landing non-stub and (b)
the historical backfill populating CH. Task 0228 closes both prereqs.

But Tier 2-4 as conceived only compares CH ↔ PG. The audit team's
stated principle ("validate against canonical reference data, not just
internal cross-store consistency") makes a CH ↔ Horizon / stellar.expert
compare the stronger signal — that is the scope of this task.

### Why this matters now

The merged Hetzner CH is the production read store after the
`indexer-cutover` (task 0241). Before any read traffic, every endpoint
that ships to the API layer must be verified to return correct data,
not just executable SQL.

The `compare-with-stellar-api` skill formalises the per-record compare
pattern (Horizon + stellar.expert + independently parsed XDR); this
task applies it systematically to all 23 endpoints.

## Implementation Plan

Five phases, sequential. Each phase produces a section in the final
validation artifact at
`docs/runbooks/artifacts/endpoint_validation_<YYYYMMDD>.md`.

### Phase A — Smoke + dispatch matrix

For each of the 23 endpoints:

1. Run `./run_endpoint_ch.sh <NN>` against Hetzner CH with one realistic
   parameter set. Capture: rows returned, basic field non-null sanity.
2. Categorise the endpoint into one of three groups:
   - **Group A (Horizon-comparable):** `02, 03, 04, 05, 06, 07, 09, 18,
19, 20`. Classic Stellar entities with documented Horizon
     equivalents.
   - **Group B (stellar.expert-only, Soroban-native):** `11, 12, 13,
14`. No Horizon equivalent — Soroban contracts / events.
   - **Group C (internal consistency):** `01, 08, 10, 15, 16, 17, 21,
22, 23`. Network stats, NFT internals, search — no clean external
     comparator; validate via cross-row sanity (sums match, FKs
     resolve, ordering monotonic).

### Phase B — Group A: Horizon parity (10 endpoints)

For each endpoint:

1. Sample 20–50 keys (random + stratified by partition).
2. For each key, fetch the CH response (via `run_endpoint_ch.sh` or a
   direct SQL invocation) and the Horizon response (REST API).
3. Field-by-field diff per the documented divergence list (§5 of
   `endpoint-queries-clickhouse/README.md` — `created_at` dropped on
   non-`ledgers` tables, `nfts.metadata` absent, etc.).
4. Categorise diffs as:
   - **Expected** — documented divergence; record + move on.
   - **Tolerance-bound** — small numeric drift (e.g., op count due to
     Horizon "successful only" semantics — already seen on 0228
     Tier 5); record + tolerance threshold.
   - **Unexpected** — drop into the issue tracker; pause endpoint
     until fixed.

Paginated endpoints (02, 04, 07, 10, 20) require cursor walk on both
sides — verify the cursor encoding round-trips correctly.

Per-tx hash compare via `transaction_hash_index` (see 0228 Phase 6
note — `transactions.id` is an Int64 surrogate, NOT the canonical
32-byte hash) is the canonical pattern for E02 / E03.

### Phase C — Group B: stellar.expert validation (4 endpoints)

For each contract endpoint:

1. Sample 10 contract IDs (mix of classified Token / Nft / Fungible /
   Other from `soroban_contracts`).
2. Fetch CH response and stellar.expert response
   (`https://api.stellar.expert/explorer/public/contract/<id>` + sub-
   resources).
3. Diff: `contract_type`, `deployed_at_ledger`, `deployer_id`,
   interface function list, recent invocation count, event payload
   round-trip.

stellar.expert has no public pagination API on every sub-resource —
some sub-validations may have to be manual (open browser, eyeball).
Record the gap honestly.

### Phase D — Group C: internal consistency (9 endpoints)

For each:

1. Define one or more "internal invariant" SQL queries (e.g., `SELECT
sum(holder_count) FROM assets FINAL` ≈ `count(DISTINCT account_id)
FROM account_balances_current FINAL` for relevant types — within
   small tolerance).
2. Run, document pass / fail.
3. For `22_get_search.sql`, smoke-test 20 known queries (USDC, XLM,
   well-known account prefixes) → expected hit per documentation.

### Phase E — Write up + close

1. Author `docs/runbooks/artifacts/endpoint_validation_<YYYYMMDD>.md`
   per the **Reporting Shape** below.
2. Each unexpected divergence found in B / C / D → spawn a bug task
   (status `backlog`, scope: one endpoint fix) linked back to this
   task ID via `related_tasks`.
3. If aggregate pass rate ≥ 22/23 endpoints (≥ 95%), declare task
   complete and archive. Otherwise carry to a Phase F revision.

## Reporting Shape

The final artifact at `docs/runbooks/artifacts/endpoint_validation_<YYYYMMDD>.md`
MUST contain three sections, each answering a different question:

### Section 1 — Per-endpoint detail (23 entries)

For every endpoint, a stanza recording:

- **Endpoint id + path** (e.g. `E03 — GET /transactions/:hash`)
- **CH tables read** by the query (resolved from the SQL header)
- **Sample method** (random by ledger DESC / stratified / first row /
  hand-picked — and the SQL used to materialise the sample set)
- **Sample size** N
- **Compared with** (one of: `Horizon REST`, `stellar.expert API`,
  `Tier 5 hash-set`, `Internal only`, `S3 archive XDR`)
- **Compare method** (e.g. `hash-set`, `field-by-field (7 fields)`,
  `cursor walk`, `count + sum`, `interface name list`)
- **Tolerances applied** with citation to the divergence rationale
  (e.g. "op_count drift accepted — Horizon successful-only semantics
  per 0228 Phase 6 Tier 5")
- **Coverage**: N sampled / total rows in the CH table (with the
  percent expressed in scientific notation when small)
- **Per-field accuracy** when field-by-field compare applies — one
  line per field with pass / N count and percent
- **Verdict**: PASS / TOL (tolerance-bound only) / FAIL — with the
  count of unexpected diffs

### Section 2 — Table coverage matrix

One row per CH table in the canonical 17-table set
(per Phase 6 report). Columns:

| Column                 | Description                                      |
| ---------------------- | ------------------------------------------------ |
| `CH table`             | table name (with current row count from Phase 6) |
| `Rows`                 | total rows (from `system.parts` active = 1)      |
| `Sampled rows`         | sum across all endpoints that read this table    |
| `Endpoints exercising` | comma-separated `E0N` list                       |
| `Compared via`         | union of external sources used                   |
| `Compare method`       | union of compare methods used                    |
| `Pass / Tol / Fail`    | aggregated counts                                |

Empty tables (`nfts`, `nft_ownership` — both 0 per 0228 Phase 6)
appear with `Sampled rows = 0` and a note "empty by design" in the
Compare method column.

### Section 3 — Group roll-up

Three short blocks summarising Group A, B, C:

```
Group A (Horizon-comparable):    N endpoints, M CH tables, K sample compares
  Pass: X  Tolerance: Y  Fail: Z (Fn description, link to spawned task)
Group B (stellar.expert):         N endpoints, M tables, K compares
  Pass: X  Tolerance: Y  Fail: Z
Group C (internal):               N endpoints, M tables, K compares
  Pass: X  Tolerance: Y  Fail: Z
```

Plus an **Overall** line: `K compares, P/23 endpoints PASS (P/23 %)`
with the go-live verdict derived from the 95 % threshold.

### Source legend (used by Section 1 + 2)

- `Horizon REST` — `horizon.stellar.org/...`
- `stellar.expert API` — `api.stellar.expert/explorer/public/...`
- `Tier 5 hash-set` — 0228 Phase 6 hash-set cross-reference
- `Internal only` — CH cross-row consistency (no external API)
- `S3 archive XDR` — for pre-Horizon-retention ledgers

### Implementation hint

Each phase script (A/B/C/D) emits a per-endpoint structured TSV row
to `/tmp/sbe-artifacts/endpoint_validation_<phase>.tsv`. Phase E is a
small aggregator that reads all four TSVs and emits the three
sections above as Markdown. Keep the TSV schema stable across phases
so the aggregator stays simple.

## Acceptance Criteria

- [ ] Phase A complete — all 23 endpoints categorised (A/B/C) with
      smoke run captured.
- [ ] Phase B complete — 10 Horizon-compared endpoints documented;
      per-endpoint diff matrix in the artifact.
- [ ] Phase C complete — 4 stellar.expert-compared endpoints
      documented; manual-spot-check gaps noted honestly.
- [ ] Phase D complete — 9 internal-consistency endpoints documented;
      structural invariants run + passed (or pinned with follow-up).
- [ ] Validation artifact at
      `docs/runbooks/artifacts/endpoint_validation_<YYYYMMDD>.md`
      committed with per-tier pass/fail + sign-off.
- [ ] Every unexpected divergence has a spawned backlog task with
      `related_tasks: ['0252']` and a one-paragraph repro recipe.
- [ ] Aggregate pass rate ≥ 22/23 (95 %) before close.
- [ ] **Docs updated** — `docs/architecture/database-schema/endpoint-queries-clickhouse/README.md`
      Validation tiers table promoted from "Deferred" → "Done"
      (or "Done with N/M divergences tracked in spawned tasks") for
      Tier 2-4.
- [ ] **API types regenerated** — N/A: this task only reads through
      the API surface, does not touch `crates/api/**`, `Cargo.{toml,lock}`,
      or `libs/api-types/**`.

## Notes

- Skill: `.claude/skills/compare-with-stellar-api/SKILL.md` — formalised
  per-record compare pattern. Use it as the inner loop for B + C.
- Hetzner target: `ch-prod-01`, container `app-clickhouse-1`. Connect
  via the existing `ch-docker` wrapper or direct
  `docker exec app-clickhouse-1 clickhouse-client`.
- Wall-clock estimate: 3–5 days incl. divergence investigation.
- This task does NOT touch CH schema, indexer, or API code. If a fix
  is required, the fix is a separate spawned task — keep this one a
  pure validation pass.
- Horizon retention boundary (around ledger 56657428 per 0228 Phase 6
  Tier 5 finding): for sampling, prefer the newer ledger half where
  Horizon still serves full responses; for the older half, use
  stellar.expert or the S3 archive directly.
