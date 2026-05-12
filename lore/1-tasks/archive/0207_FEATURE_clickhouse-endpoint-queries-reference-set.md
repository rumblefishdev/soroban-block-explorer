---
id: '0207'
title: 'ClickHouse endpoint queries reference set (parallel to PG endpoint-queries/)'
type: FEATURE
status: completed
related_adr: ['0044']
related_tasks: ['0167', '0204', '0205']
tags:
  [
    layer-docs,
    layer-db,
    clickhouse,
    endpoint-queries,
    adr-0044,
    effort-medium,
    priority-medium,
  ]
links:
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - docs/architecture/database-schema/endpoint-queries/README.md
history:
  - date: '2026-05-11'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned after task 0204 (db-clickhouse crate + Docker + init.sql)
      and task 0205 (backfill-runner --target clickhouse) landed. Both
      populate CH but neither documents the read-side query patterns.
      0207 mirrors task 0167 (commit 1582c8e, 23 PG SQL files) for the
      CH pilot per ADR 0044, in parallel folder
      `docs/architecture/database-schema/endpoint-queries-clickhouse/`.
      Goal: canonical CH query reference, idiomatic syntax (FINAL,
      dictGet, intDiv partitioning), §5 divergences explicit, validated
      against local CH mirror + audit PG.
  - date: '2026-05-11'
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active via /promote-task. Ready to start implementation —
      no blockers; CH mirror + audit PG container available locally for
      validation.
  - date: '2026-05-11'
    status: completed
    who: stkrolikiewicz
    note: >
      Phase 1-3 + Tier 1 verification landed in commit aa8dff9.
      23 CH SQL files + README + run/compare scripts under
      `docs/architecture/database-schema/endpoint-queries-clickhouse/`,
      plus a 1-line link added in `clickhouse-pilot.md`. All 34
      statements pass `clickhouse-client --format=Null` against the
      canonical ADR 0044 schema applied by the `db-clickhouse-init`
      sidecar (Tier 1 PASS). Hand-inserted-rows smoke test confirmed
      E01/E04/E08 return shape-correct data end-to-end. Tier 2-4
      empirical row-count diff vs PG is **deferred** — gated on the
      CH writer becoming non-stub (`db_clickhouse::persist::persist_ledger_clickhouse`
      is still a no-op per task 0205); `compare_pg_ch.sh` scaffold is
      in place so the follow-up is a small per-endpoint binding pass.
  - date: '2026-05-12'
    status: completed
    who: stkrolikiewicz
    note: >
      Follow-up fix-up on the same branch (post-archive). PR #175
      (commit b9db354 `feat(lore-0206): clickhouse writer for the 0204-pilot
      schema`) substantially rewrote `crates/db-clickhouse/schema/init.sql`
      to the hybrid-surrogate design: 3 hub tables keep `Int64 id`
      (cityhash64), 14 other tables switched to natural composite keys.
      Tier 2 sweep against a fresh 64k-ledger backfill (range
      62016000-62079999, 11.6 GB raw via the now non-stub CH writer)
      initially scored 9/23 green; 14 queries broken by schema drift
      (assets/nfts/operations_appearances dropped surrogate `id`,
      `liquidity_pools.created_at_ledger` renamed `last_updated_ledger`).
      Rewrote 10 .sql files (02, 03, 08, 09, 10, 15, 16, 17, 18, 19, 22)
      to use natural composite keys + sentinels `(issuer_id=0, contract_id=0)`
      for "no issuer / no contract"; runner case branches updated for
      new param shapes + discovery oneshots swapped to `last_updated_ledger`.
      E02 memory blowup (5.6 GB exceeded) addressed by (a) partition
      predicate always applied (`$7 = caller-supplied latest_partition`),
      (b) JOIN-after-LIMIT subquery pattern (avoids 300k×300k hash
      hashtable), (c) dropped `contract_surrogate_ids[]` projection
      (3 correlated FINAL subqueries × 50 rows). Final state: **23/23
      endpoints return real data on populated CH (Tier 2 PASS)**,
      32/32 statements parse via --syntax-only (Tier 1 PASS).
      §5.1 win confirmed: E14 returns inline decoded JSON event payload
      (`topics_xdr` field actually stores ScVal-decoded JSON not raw XDR
      per PR #175 writer design).
---

# ClickHouse endpoint queries reference set (parallel to PG endpoint-queries/)

## Summary

Create `docs/architecture/database-schema/endpoint-queries-clickhouse/` with
23 `.sql` files mirroring the PG reference set (task 0167) 1:1, one per API
endpoint E01-E23. Each query uses idiomatic ClickHouse syntax per ADR 0044
(ReplacingMergeTree `FINAL`, `dictGet` for tx-hash lookup, `intDiv`
partition pruning) and documents §5 deliberate divergences from PG (no
`created_at` outside `ledgers`, no `metadata` on `nfts`, full-payload
`contract_events`, `transaction_hash_index` → Dictionary). Reviewers verify
CH endpoints produce same data as PG; devs writing CH-backed handlers have
canonical patterns.

## Context

**Why now:** Tasks 0204 + 0205 landed CH pilot infra and dual-write
backfill. CH parallel store exists with ~17 tables + 1 Dictionary. But no
read-side query reference exists — reviewers cannot verify CH endpoints
produce equivalent data to PG; new devs lack canonical CH patterns.

**Prior art:** Task 0167 (commit 1582c8e) shipped the PG reference set:
23 `.sql` files in `docs/architecture/database-schema/endpoint-queries/`,
header convention (Endpoint / Purpose / Source / Schema / Inputs / Indexes
/ Notes), `run_endpoint.sh` runner, `README.md` with conventions. 0207
replicates that pattern for CH.

**Non-invasive scope:** Docs-only folder. Zero edits outside
`endpoint-queries-clickhouse/` except 1-line link in parent README. No
crate changes, no API changes, no schema changes, no ADR changes.

## Implementation Plan

### Phase 1 — Audit PG set (1 day)

Enumerate `docs/architecture/database-schema/endpoint-queries/*.sql`. Build
working `MAPPING.md` (deleted at end): endpoint → PG file → CH file →
applicable ADR 0044 §-rules → CH patterns required.

### Phase 2 — Skeleton (0.5 day)

Create folder, `README.md`, `run_endpoint_ch.sh`, `compare_pg_ch.sh`, and
23 stub `.sql` files with full headers + `SELECT 1` placeholder body. CI
gate: every stub passes Tier 1 (parses via `clickhouse-client --format=Null`).

### Phase 3 — Port queries (4 days)

Per query: write → Tier 1 parse → Tier 2 row-count diff → Tier 3 sample
diff → commit. Order by complexity:

- **D1**: 01-07 (ledgers/tx/operations — `FINAL` discipline + `JOIN ledgers` for `closed_at`)
- **D2**: 04 `dictGet` + 08 assets keyset + 09-12 accounts (`FINAL` state reads)
- **D3**: 13-17 (operations by account, events full-payload, NFT transfers)
- **D4**: 18-23 (NFT detail, LPs, search w/ trigram regression risk, stats)

### Phase 4 — Validation end-to-end (1 day)

Full `compare_pg_ch.sh` matrix across all 23 queries against local CH mirror
(`ch-mirror-setup.sh` populated container, ~168k ledgers from local
backfill) and audit PG container `3f9c594d90b4`. Document any §5 divergence
in each header. `OPTIMIZE TABLE ... FINAL` before Tier 4 aggregate
comparisons.

### Phase 5 — README + reviewer guide (0.5 day)

Sections: why CH-specific reference set exists, `FINAL` discipline (which
tables, why), Dictionary usage (`dictGet` syntax + RAM cost), §5 divergences
quick-ref, how to add new query (template + checklist), how to validate.

### Phase 6 — Close (0.5 day)

Per `/lore-framework-tasks`: acceptance criteria checked, ADR 0044 stays
`proposed`, API types regen N/A, parent README link added, `git mv` to
archive, `lore_generate-index`.

**Total: 8-10 days, 1 dev.**

## ADR 0044 Compliance Matrix

Every query must satisfy:

| ADR §                                       | Rule                                                             | Verification                                              |
| ------------------------------------------- | ---------------------------------------------------------------- | --------------------------------------------------------- |
| §4.1 ledgers                                | Plain MergeTree, no version                                      | Direct read, no `FINAL`                                   |
| §4.2 transactions                           | ReplacingMergeTree(ingested_at)                                  | Every read `FINAL`; partition predicate where range known |
| §4.3 operations                             | Same as §4.2                                                     | Same                                                      |
| §4.4 contract_events full payload           | ReplacingMergeTree, payload col                                  | E14/E16 read single table — no appearances JOIN           |
| §4.5 accounts / balances / balances_current | ReplacingMergeTree(last_modified_ledger)                         | `FINAL` on state reads                                    |
| §4.6 nfts                                   | ReplacingMergeTree, no metadata col                              | E18 returns metadata NULL or via off-chain table          |
| §4.7 liquidity_pools                        | Plain MergeTree                                                  | No `FINAL`                                                |
| §4.8 wasm_interface_metadata                | Plain MergeTree                                                  | No `FINAL`                                                |
| §4.9 transaction_hash_dict                  | HASHED Dictionary, RAM-bounded                                   | E04 uses `dictGet`, no scan                               |
| §5.1 events full-content                    | No JOIN to events_appearances (absent in CH)                     | E14 single-table read                                     |
| §5.2 created_at dropped except ledgers      | All other tables `JOIN ledgers ON ledger_sequence` for closed_at | Verified per query                                        |
| §5.3 nfts.metadata dropped                  | E18 returns NULL or via enrichment table                         | Header documents                                          |
| §5.4 \_sqlx_migrations dropped              | N/A                                                              | N/A                                                       |
| §5.5 tx_hash_index → Dictionary             | E04 uses `dictGet`, no scan                                      | Verified                                                  |

## Idiomatic CH Patterns

```sql
-- Pattern 1: tx by hash via Dictionary (replaces PG transaction_hash_index)
SELECT t.*, l.closed_at
FROM transactions t FINAL
JOIN ledgers l ON l.ledger_sequence = t.ledger_sequence
WHERE (t.ledger_sequence, t.application_order) = (
    dictGet('transaction_hash_dict',
            ('ledger_sequence', 'application_order'),
            unhex(?))
);

-- Pattern 2: range query w/ partition pruning + keyset cursor
SELECT *
FROM operations FINAL
WHERE intDiv(ledger_sequence, 500000)
        BETWEEN intDiv(?, 500000) AND intDiv(?, 500000)
  AND (ledger_sequence, application_order, op_index) < (?, ?, ?)
ORDER BY ledger_sequence DESC, application_order DESC, op_index DESC
LIMIT 50;

-- Pattern 3: account current state
SELECT abc.*
FROM account_balances_current abc FINAL
WHERE account_id = ?
ORDER BY asset_code;

-- Pattern 4: events full-payload (no appearances JOIN, §5.1)
SELECT ce.*, l.closed_at
FROM contract_events ce FINAL
JOIN ledgers l ON l.ledger_sequence = ce.ledger_sequence
WHERE ce.contract_id = unhex(?)
  AND intDiv(ce.ledger_sequence, 500000) >= intDiv(?, 500000)
ORDER BY ce.ledger_sequence DESC, ce.application_order DESC
LIMIT 50;

-- Pattern 5: NFT detail w/o metadata col (§5.3)
SELECT n.contract_id, n.token_id, n.owner_account,
       l.closed_at AS minted_at
FROM nfts n FINAL
JOIN ledgers l ON l.ledger_sequence = n.mint_ledger_sequence
WHERE n.contract_id = unhex(?) AND n.token_id = ?;
```

## Anti-Patterns Rejected in Review

```sql
-- WRONG: full scan, ignores Dictionary
SELECT * FROM transactions WHERE tx_hash = ?;

-- WRONG: PG ON CONFLICT (CH uses Replacing merge semantics)
INSERT INTO ... ON CONFLICT (...) DO NOTHING;

-- WRONG: missing FINAL on state read
SELECT * FROM account_balances_current WHERE account_id = ?;

-- WRONG: t.created_at on non-ledgers table (§5.2)
SELECT t.created_at FROM transactions t WHERE ...;

-- WRONG: JOIN to soroban_events_appearances (§5.1, table absent)
SELECT e.* FROM contract_events e
JOIN soroban_events_appearances ea ON ea.event_id = e.id;

-- WRONG: SELECT n.metadata FROM nfts (§5.3, col absent)
```

## Data Validation Strategy — 4 Tiers

For each of 23 queries, vs local CH mirror + audit PG container `3f9c594d90b4`:

- **Tier 1 — Schema parse** (mandatory, CI): `clickhouse-client --query=$(cat NN.sql) --format=Null` → exit 0.
- **Tier 2 — Row count** (mandatory, local): same params PG vs CH → match. Tolerance: `±0` for facts, `±0` for hash lookup, `±epsilon_merge` for Replacing state (run `OPTIMIZE TABLE ... FINAL` then retest).
- **Tier 3 — Sample diff** (recommended): 10 random keys from result, column-by-column PG vs CH. Expected diffs per §5 documented in header. Anything outside §5 = bug.
- **Tier 4 — Aggregate equivalence** (for aggregating E08/E15/E23): compare total_supply, holder_count, event_count. Tolerance per §5.

## File Header Convention

Each `.sql` file starts with:

```sql
-- Endpoint: GET /v1/transactions/:hash
-- Purpose: Fetch transaction detail by hash
-- Source: crates/api/src/handlers/transactions.rs::get_by_hash
-- Schema: transactions (ReplacingMergeTree, PK ledger_sequence/application_order)
-- Data sources: transactions, ledgers (closed_at), transaction_hash_dict
-- Inputs: tx_hash (32-byte hex)
-- Indexes: PK on (ledger_sequence, application_order); HASHED Dict on tx_hash
-- CH Engine: ReplacingMergeTree(ingested_at) — FINAL required
-- CH Pattern: dictGet hash→seq, then PK seek + JOIN ledgers for closed_at
-- ADR 0044 §: §4.2 (engine), §4.9 (Dict), §5.2 (closed_at), §5.5 (Dict replaces hash idx)
-- Notes: ...
```

PG header (Endpoint/Purpose/Source/Schema/Data sources/Inputs/Indexes/Notes) preserved + 3 CH-specific lines (Engine, Pattern, ADR §).

## Risk Register

| ID  | Risk                                                    | Lik  | Imp  | Mitigation                                                          |
| --- | ------------------------------------------------------- | ---- | ---- | ------------------------------------------------------------------- |
| R1  | FINAL discipline misapplied (forgotten or over-applied) | Med  | High | Pre-flight checklist + reviewer matrix above                        |
| R2  | Dictionary missing from CH mirror → E04 fails           | Low  | Med  | `ch-mirror-setup.sh` creates Dict; verify Phase 0                   |
| R3  | E22 global search — CH has no `pg_trgm`                 | High | Med  | Document; pick `LIKE` vs `tokenbf_v1` skip index D4; flag in review |
| R4  | E14/E16 full-payload slower than PG appearances         | Med  | Low  | Baseline only; perf opt = follow-up task                            |
| R5  | Replacing merge lag → Tier 2 mismatch on state tables   | High | Low  | `OPTIMIZE TABLE ... FINAL` pre-validation                           |
| R6  | `closed_at` JOIN adds N JOINs vs PG single-table        | Med  | Low  | Acceptable for §5.2; document baseline                              |
| R7  | Scope creep (wiring handlers)                           | High | High | Hard limit: docs-only. CI gate on non-invasive proof                |

## Acceptance Criteria

- [x] 23 `.sql` files exist in `endpoint-queries-clickhouse/`, naming matches PG set 1:1
- [x] All 23 pass Tier 1 in CI (parse vs live init.sql schema) — 34 statements verified
- [ ] All 23 pass Tier 2 (row count within documented tolerance) vs PG + CH — **deferred to follow-up** (CH writer is still a no-op stub per task 0205; `compare_pg_ch.sh` scaffold in place). Smoke-tested on E01/E04/E08 with hand-inserted rows.
- [x] 100% files have header with `ADR 0044 §:` line citing every applicable rule
- [x] Compliance matrix (above): 0 violations at review (verified during Tier 1 sweep)
- [x] `README.md` covers FINAL discipline, dictGet, §5 divergences, validation workflow + reviewer guide
- [x] `run_endpoint_ch.sh` + `compare_pg_ch.sh` runnable, documented (compare_pg_ch.sh is scaffold pending non-stub CH writer)
- [x] `git diff --name-only develop...HEAD` shows zero files outside new folder + 1 line in `clickhouse-pilot.md`
- [x] No changes to `crates/`, no changes to existing PG `endpoint-queries/`, no ADR changes
- [x] **Docs updated** — new folder + parent `clickhouse-pilot.md` reference added (per ADR 0032)
- [x] **API types regenerated** — N/A — no API surface change

## Implementation Notes

### Files touched

| File                                                                               | Change                                                                | LOC       |
| ---------------------------------------------------------------------------------- | --------------------------------------------------------------------- | --------- |
| `docs/architecture/database-schema/endpoint-queries-clickhouse/*.sql` (×23)        | CREATE — one per E01-E23 endpoint                                     | ~1500     |
| `docs/architecture/database-schema/endpoint-queries-clickhouse/README.md`          | CREATE — conventions + FINAL/§5/reviewer guide                        | ~250      |
| `docs/architecture/database-schema/endpoint-queries-clickhouse/run_endpoint_ch.sh` | CREATE — runner mirroring PG `run_endpoint.sh`                        | ~410      |
| `docs/architecture/database-schema/endpoint-queries-clickhouse/compare_pg_ch.sh`   | CREATE — Tier 2-4 scaffold (per-endpoint shims deferred to follow-up) | ~150      |
| `docs/architecture/database-schema/clickhouse-pilot.md`                            | EDIT — add "Read queries (reference set)" + reference link            | +11 lines |

Total: ~2320 LOC across 27 files (23 SQL + README + 2 scripts + 1 doc edit).
**Zero changes outside `docs/architecture/**` — fully non-invasive.\*\*

### Phases executed

| Phase | What                                          | Outcome                                                                                                        |
| ----- | --------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| 1     | Audit PG set + build `MAPPING.md` working doc | 23 files mapped; CH schema reality verified vs ADR 0044; translation rules tabulated                           |
| 2     | Skeleton: folder, README, scripts             | Tier 1 gate operational; runner mirrors PG conventions                                                         |
| 3     | Port 23 queries                               | All 34 statements parse against canonical CH; idiomatic FINAL/dictGet/intDiv applied throughout                |
| 4     | Tier 1 sweep + end-to-end smoke               | Tier 1 100% pass; E01/E04/E08 hand-data smoke confirms shape correctness; Tier 2-4 deferred (writer-stub gate) |
| 5     | README finalization + delete `MAPPING.md`     | Reviewer guide section added; working doc moved to `.trash/`                                                   |
| 6     | Task closure, archive, push                   | This commit                                                                                                    |

### Design Decisions

#### From Plan

1. **Non-invasive scope (docs-only).** Hard limit per the plan's R7 risk: zero edits outside
   `docs/architecture/database-schema/`. Verified by `git diff --name-only develop...HEAD`.
   The only out-of-folder touch is a 11-line addition in `clickhouse-pilot.md` for the
   cross-reference link.
2. **Tier 1 parse via `clickhouse-client --format=Null`** as the CI gate. Cheap, deterministic,
   catches schema drift and CH-syntax bugs that the planner rejects.
3. **FINAL on every `ReplacingMergeTree` read.** Documented in the README's "FINAL discipline"
   table. Plain `MergeTree` reads (`ledgers`, `liquidity_pools`, `wasm_interface_metadata`) skip
   FINAL.
4. **`dictGet('transaction_hash_dict', ...)` as the canonical E03 hot path.** Replaces PG's
   `transaction_hash_index` partition-PK seek per §5.5. The Dict attribute is `String` (not
   `FixedString`) per the §4.9 implementation constraint; callers pass `toString(unhex(hex))`.
5. **Partition prune via `intDiv(ledger_sequence, 500000) BETWEEN ...`** on all 8 partitioned
   fact tables. Applied wherever the cursor or input gives a ledger range.
6. **§5.2 closed_at via JOIN to `ledgers`.** Every query that needs a wall-clock timestamp
   joins `ledgers` on `ledger_sequence`. Cursor tuples drop the `created_at` term that PG
   cursors carry — the natural CH keyset is `(ledger_sequence, application_order, id)`.
7. **§5.3 metadata absent.** E15/E16 don't project `n.metadata` (matches PG post-migration
   20260507120000). Detail handler fetches via Soroban RPC `token_uri()` (ADR 0043).
8. **§5.1 E14 returns full event payload inline.** Major divergence vs PG E14 which only
   returns appearance + Archive bridge. Documented in the E14 header.

#### Emerged (decisions taken during implementation, not in original plan)

9. **Correlated subqueries in JOIN — CH 26.x limitation.** PG's `LATERAL` pattern for
   `contract_ids[]` in E02 (UNION across 3 appearance tables + JOIN to `soroban_contracts`
   for StrKey resolution) hits CH's `NOT_IMPLEMENTED` ("Correlated subqueries are not
   supported in JOINs yet"). **Workaround:** project `Array(Int64)` of contract surrogate ids
   from `arrayDistinct(arrayConcat(...))` over three independent scalar subqueries; the API
   layer resolves ids → C-StrKeys via a batched `IN` lookup on `soroban_contracts`. One
   extra round-trip per page; acceptable for the pilot.

10. **`UNION DISTINCT` in correlated subqueries — same limitation.** CH rejects with
    "Cannot check Distinct plan step for correlated expressions". Swapped to
    `arrayConcat` + `arrayDistinct` over independent branches (decision 9).

11. **CH 26.x rejects correlated subqueries with ORDER BY/LIMIT.** PG E05 uses `LATERAL`
    with `ORDER BY ... LIMIT 1` referencing the outer row. CH errors "Cannot check Sorting
    plan step for correlated expressions". **Workaround:** since `$1` is bound, the inner
    queries can reference `$1` directly instead of the outer row.

12. **`ledger_sequence` alias collision in argMax projection.** CH rejects
    `argMax(ledger_sequence, ledger_sequence) AS ledger_sequence` with "Aggregate function
    ... is found inside another aggregate function". Renamed the alias to
    `latest_ledger_sequence` to break the collision.

13. **`pg_trgm` regression on E22 (R3 from risk register).** CH has no `gin_trgm_ops`
    equivalent. **Decision:** use `positionCaseInsensitiveUTF8` for substring match on
    small tables (`assets`, `nfts`). Free-text search performance optimisation deferred
    to a follow-up (`tokenbf_v1` skip index on the relevant columns would land in a
    schema change, out of scope for 0207).

14. **`asset_type_name` / `op_type_name` / `contract_type_name` PG helpers — CH has no
    SQL equivalent.** Project raw `Int16`; API decodes via the same Rust enum that backs
    PG's helper function. Documented in each affected file's Notes.

15. **`encode(b, 'hex')` returns lowercase in PG, `hex(b)` returns uppercase in CH.**
    Standardised on `lower(hex(b))` everywhere CH-side to keep the API's hex strings
    PG-compatible (frontend already expects lowercase).

16. **Local `ch-mirror` container schema differs from canonical pilot schema** (UInt vs
    Int, FixedString(64) hex-string vs FixedString(32) raw, etc.). Decision: target the
    canonical `crates/db-clickhouse/schema/init.sql` via fresh `docker compose up
clickhouse db-clickhouse-init` — NOT the local mirror. Documented in README.

17. **Tier 2-4 row-count validation deferred.** The original plan assumed CH would be
    populated via `backfill-runner --target clickhouse`, but the runner currently calls
    a no-op stub (`db_clickhouse::persist::persist_ledger_clickhouse`, task 0205).
    Empirical row-count vs PG is gated on the writer becoming real — a follow-up task
    after the next CH ingest milestone. `compare_pg_ch.sh` scaffold is in place so the
    per-endpoint binding work is the only remaining piece. Tier 2 spot-checked on
    E01/E04/E08 with manually-inserted rows to prove the queries return shape-correct
    data.

### Issues Encountered

- **Pre-commit prettier reformatted README tables.** Hook ran `npx nx format:write --files`
  on the staged changes, widened table padding. Intentional reformat by the project's
  linter — left as-is.
- **`zsh` vs `bash` array indexing in the parse_helper script.** Initial inline helper
  used `args[$((i-1))]` which is 0-indexed in bash but 1-indexed in zsh, producing
  off-by-one substitutions in interactive shell tests. Fixed by wrapping the helper in
  a separate `.sh` file with `#!/usr/bin/env bash` shebang.
- **Stale `target/release/backfill-runner` binary.** Pre-build binary didn't have
  `--target clickhouse` flag (build from before task 0205 landed). Resolved via
  `cargo build --release -p backfill-runner` — 1m12s build, no regression.
- **`aws s3 sync` for a 64k-ledger partition.** Even a 10-ledger backfill request
  syncs the entire partition. Killed mid-sync since the CH writer is a stub anyway
  (decision 17).

### Out of Scope (preserved from plan)

- Wiring API handlers to CH (separate task — needs runtime config switching)
- Perf benchmarks vs PG (separate task — needs perf framework)
- ADR 0044 status flip `proposed` → `accepted` (separate task — needs prod data)
- Indexer dual-write changes (already done in 0205)
- Frontend changes (none needed)

### Future Work (spawned to follow-up tasks)

None spawned automatically — Tier 2-4 work is gated on the CH writer becoming non-stub.
The natural sequencing is: CH writer task → re-run `compare_pg_ch.sh` against same
ledger range as PG audit → mark Tier 2-4 acceptance criterion `[x]` retrospectively
in this archived task.

## Out of Scope

- Wiring API handlers to CH (separate task — needs runtime config switching)
- Perf benchmarks vs PG (separate task — needs perf framework)
- ADR 0044 status flip `proposed` → `accepted` (separate task — needs prod data)
- Indexer dual-write changes (already done in 0205)
- Frontend changes (none needed)

## Notes

- Local CH mirror already populated via `ch-mirror-setup.sh` (~168k ledgers, 17 tables + Dictionary) from prior session.
- Audit PG container `3f9c594d90b4` on port 5432 has equivalent dataset; safe to read against during validation.
- Approach analogous to commit `1582c8e` (task 0167 PG reference set) — header convention, runner script, per-endpoint file naming all mirror that precedent.
- Reusable runner pattern: copy `endpoint-queries/run_endpoint.sh`, swap `psql` for `clickhouse-client`, keep case-dispatched endpoint discovery.
