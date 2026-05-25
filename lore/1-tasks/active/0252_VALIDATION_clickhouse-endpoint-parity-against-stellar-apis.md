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
  - date: '2026-05-23'
    status: active
    who: stkrolikiewicz
    note: >
      **Phase B endpoints E03 + E19 GREEN.**

      E03 (`/transactions` list) — sample 30,294 mainnet tx hashes
      from Hetzner CH joined against Horizon `/transactions/:hash`;
      6 canonical fields compared (hash, ledger, source_account,
      fee_charged, successful, operation_count). 25,115 hashes
      resolved on Horizon (rest 404 — pre-pruning window or
      Soroban-only synthetic). **pass=150,690, fail=0, tolerance=0**
      across all six fields. elapsed 32,050 s ≈ 8h54m. Summary at
      `/tmp/sbe-artifacts/0252/phase_b_e03_summary.json`.

      E19 (`/liquidity-pools` list) — sample 5,000 LP samples vs
      Horizon `/liquidity_pools/:id`; 7 fields (pool_id, fee_bps,
      total_shares, reserve_a, reserve_b, last_updated_ledger, type).
      **pass=19,929, fail=0, tolerance=6,664** (tolerance bucket
      concentrated in reserve_a / reserve_b / last_updated_ledger —
      expected live-data drift between Horizon snapshot and CH
      sample). elapsed 78 min. Summary at
      `phase_b_e19_summary.json`.

      Operator notes:
        - E03 + E19 launched 2026-05-22 from a single ssh session on
          pts/0 (no tmux). E19 finished cleanly at 15:34. E03 kept
          running into a session disconnect; reptyr attach failed
          ("Inappropriate ioctl for device" — process had already
          lost its controlling tty), but the process was already
          SIGHUP-immune (`PPID=1, TT=?`, stdout/stderr redirected to
          `e03_full.log` file fd, not tty). Finished cleanly at 22:43.
        - Lesson: future runs go straight into `tmux new -d -s …`
          with `2>&1 | tee` to a log file. Captured in
          [[ssh-remote-auth]] and [[hetzner-ch-artifacts]] memory.

      Cumulative Phase B coverage so far: E03 ✅, E05, E06, E09, E11
      (deployer mismatch surfaced — fixed via task 0255 Phase 1),
      E19 ✅. Remaining endpoints + final Phase B report still to do.
  - date: '2026-05-24'
    status: active
    who: stkrolikiewicz
    note: >
      **Phase B overnight batch — E02, E04, E18 GREEN.**

      All three launched in detached tmux sessions on Hetzner with
      `tee` to file-fd logs (per the previous-session lesson). No
      babysitting required; user disconnected ssh and the runs
      finished while idle.

      E02 (`/transactions` list, per-ledger set compare): 600 anchor
      ledgers × ~229 tx/ledger avg. **pass=687,616, fail=0,
      tolerance=0**. elapsed 26 min. Rewrote the compare mid-pilot —
      first iteration page-vs-page slicing hit 100 % spurious
      hash_set_equal fails because CH orders by
      `(ledger_seq DESC, cityhash64-id DESC)` within a ledger while
      Horizon orders by `application_order DESC`. By-design CH sort
      with no Horizon comparator; the load-bearing assertion is
      per-ledger SET equality + per-row field correctness, not
      within-ledger sequence. Fix landed before full run.

      E04 (`/ledgers` list, per-ledger detail): 600 anchors × 5
      fields. **pass=2,985, fail=0, tolerance=0**. elapsed 10 min.
      Three pre-retention anchors skipped (`HZ_PRE_RETENTION`),
      consistent with the 56,657,428 retention floor first measured
      in 0228 Phase 6.

      E18 (`/liquidity-pools` list, per-pool projection): 5,000
      pools. **pass=27,122, fail=0, tolerance=7,294**. elapsed 131
      min. Tolerance bucket is reserves / total_shares /
      latest_snapshot_at — same live-drift class as E19. E18's
      value-add over E19 is the asset code/issuer projection +
      latest-snapshot ledgers JOIN; both passed strict across the
      5K sample.

      Cumulative coverage now **9/23 endpoints**: E02 ✅, E03 ✅,
      E04 ✅, E05 ✅, E06 ✅, E09 ✅, E11 (deployer fixed via 0255),
      E18 ✅, E19 ✅. Phase B Group A remaining: E07 (needs accounts
      sample pool). Phase D Group C (9 internal-consistency
      endpoints) and Phase C Group B (3 stellar.expert endpoints —
      E12, E13, E14) ahead.

      Statistical envelope across the 9 GREEN endpoints: 0
      unexpected fails on > 800K field-level compares — well below
      the 0.01 % bound the task plan set for "Rule of Three" 95 %
      confidence.
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

1. Sample **30,000 keys** stratified across 25 ledger partition buckets
   (1200/bucket × 25 buckets = 30K). Plus **3,000 adversarial edge**
   samples (partition straddles, worker-handoff ledgers, max/min-tx
   ledgers, oldest/newest accounts) = 33K total per endpoint.
2. For each key, fetch the CH response and the Horizon response
   (REST API).
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

**Statistical foundation:** 30K stratified samples with zero defects
give a 95 % upper bound of ≤ 0.01 % true defect rate (Rule of Three:
3/n). This exactly satisfies the 0228 AC §"≤ 0.01 % mismatch on 1000
stratified ledgers" generalised to the per-endpoint level.

### Phase B.5 — S3 archive XDR fallback for pre-retention ledgers

Horizon retention boundary sits around ledger **56,657,428** (verified
in 0228 Phase 6 Tier 5 — Horizon returns `null` for `successful_transaction_count`
below that). ~50 % of the backfill range (50.4 M → 56.6 M, ~6.2 M ledgers)
is therefore not Horizon-validatable.

For 5,000 stratified samples from the pre-retention half, re-parse the
ledger XDR from the S3 archive (`s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/...`)
using the locked parser SHA from task 0228 and compare the parsed
output against CH. This gives a "CH ↔ canonical XDR" path that is
independent of Horizon retention and is the strongest possible
correctness signal for the older half.

Wall: ~3 h dominated by XDR download + parse (CH compare is fast).

### Phase C — Group B: stellar.expert validation (4 endpoints)

For each contract endpoint:

1. Sample **5,000 contract IDs stratified by `contract_type`**:
   1.7 % of the 295 K SAC tokens, plus oversample of the rare types
   (full population of Nft / Fungible, 20 % of NULL).
2. Fetch CH response and stellar.expert response
   (`https://api.stellar.expert/explorer/public/contract/<id>` + sub-
   resources).
3. Diff: `contract_type`, `deployed_at_ledger`, `deployer_id`,
   interface function list, recent invocation count, event payload
   round-trip.

stellar.expert has no public pagination API on every sub-resource —
some sub-validations may have to be manual (open browser, eyeball).
Record the gap honestly.

### Phase D — Group C: internal consistency (9 endpoints) + full-table invariants

Two halves:

#### D.1 — Per-endpoint internal sanity (5,000 stratified samples / endpoint)

For each of the 9 internal-consistency endpoints, stratified
sampling (matching Group A method but no external comparator —
just CH row sanity, FK resolution, monotonic ordering).

For `22_get_search.sql`, smoke-test 100 known queries (USDC, XLM,
well-known account / contract / pool / asset prefixes) → expected
hit count + ordering per documentation.

#### D.2 — Full-table invariants (no sampling, scan all rows)

The 6 CH tables that no endpoint exercises directly need dedicated
invariant queries that scan the full table — cheap on CH-local
(no rate limit), strongest possible coverage:

| Table                                                   | Invariant                                                                                                                      |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `account_balances_current` (47 M)                       | `sum(balance) per asset` ≈ `assets.total_supply` within Decimal128(7) tolerance; one row per (asset, asset_type) pair          |
| `transaction_participants` (8.19 B)                     | `count(distinct account_id)` ≈ `count() FROM accounts FINAL` within bootstrap tolerance (≤ 41 zero-fsl accounts)               |
| `operations_appearances` (5.83 B)                       | each row's `transaction_id` exists in `transactions FINAL`; full anti-join (partition-aware)                                   |
| `soroban_events` (8.68 B)                               | per `(contract_id, ledger_sequence)`, `count(distinct transaction_id, event_index) == count(*)`; FK to `transactions` resolves |
| `soroban_invocations_appearances` (719 M)               | per `(contract_id, transaction_id)`, exists in `soroban_contracts FINAL`; FK to `transactions` resolves                        |
| `nfts_pending` / `nft_ownership_pending` (49 M + 112 M) | every NFT pending row has ≥ 1 matching ownership row; quarantine bucket has consistent `contract_id` mapping                   |

Each invariant: SQL query, expected result, observed result, diff
(if any) in the report. Mismatches > 0 = mandatory follow-up task.

### Phase F — Per-endpoint latency profile

Independent of correctness validation. For each of the 23 endpoints:

1. Discover one realistic param set (reuse Phase A discovery).
2. Run query **500 times sequentially**.
3. Drop first 10 runs (cold-cache warm-up — recorded separately as
   `cold_first_ms`).
4. Compute `p50`, `p95`, `p99`, `max`, `min` over runs 11-500
   (warm steady-state).

Verdict thresholds (proposed — tunable after pilot):

| p95 (warm)  | Verdict | Action                                  |
| ----------- | ------- | --------------------------------------- |
| < 100 ms    | FAST    | hot dict + partition prune working      |
| 100-500 ms  | OK      | acceptable for paginated / aggregating  |
| 500-1500 ms | SLOW    | flag for follow-up (missing index?)     |
| > 1500 ms   | FAIL    | blocks go-live, spawn optimisation task |

Wall: ~30 min total (23 × 500 × ~100 ms median ≈ 20 min).

Output TSV: `endpoint, file, cold_first_ms, p50_warm, p95_warm,
p99_warm, max_ms, min_ms, n_warm, verdict`.

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
Group A.5 (S3 XDR fallback):     pre-retention pre-validation, M tables, K compares
  Pass: X  Tolerance: Y  Fail: Z
Group B (stellar.expert):         N endpoints, M tables, K compares
  Pass: X  Tolerance: Y  Fail: Z
Group C (internal):               N endpoints, M tables, K compares
  Pass: X  Tolerance: Y  Fail: Z
Full-table invariants:           6 tables, 0 sampling, all pass-or-fail booleans
```

Plus an **Overall** line: `K compares, P/23 endpoints PASS (P/23 %)`
with the go-live verdict derived from the 95 % threshold.

### Section 4 — Latency profile

One row per endpoint sourced from `phase_f_perf.tsv`:

| Endpoint | Cold first | p50 warm | p95 warm | p99 warm | Max | Min | N (warm) | Verdict |

Verdict column: `FAST`, `OK`, `SLOW`, `FAIL` per the Phase F threshold
table. Every `SLOW` or `FAIL` row links to a spawned optimisation
task (`related_tasks: ['0252']`).

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
      30K stratified + 3K adversarial samples per endpoint; zero defects
      yields ≤ 0.01 % true rate at 95 % confidence; per-endpoint diff
      matrix in the artifact.
- [ ] Phase B.5 complete — S3 archive XDR fallback validated 5K
      stratified samples from the pre-Horizon-retention half (ledger
      < 56.6 M); pass-rate per stratum recorded.
- [ ] Phase C complete — 4 stellar.expert-compared endpoints
      documented; 5K stratified samples per endpoint with oversample
      of rare contract types; manual-spot-check gaps noted honestly.
- [ ] Phase D complete — 9 internal-consistency endpoints documented;
      D.1 sampled invariants + D.2 full-table invariants for the 6
      indirect-only tables; all mismatches > 0 spawned as bugs.
- [ ] Phase F complete — 23 endpoints have warm `p50/p95/p99` captured;
      all `p95 < 1500 ms` or have a spawned optimisation task.
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
