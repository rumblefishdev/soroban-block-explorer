---
id: '0207'
title: 'ClickHouse endpoint queries reference set (parallel to PG endpoint-queries/)'
type: FEATURE
status: active
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

- [ ] 23 `.sql` files exist in `endpoint-queries-clickhouse/`, naming matches PG set 1:1
- [ ] All 23 pass Tier 1 in CI (parse vs live init.sql schema)
- [ ] All 23 pass Tier 2 (row count within documented tolerance) vs local CH mirror + audit PG
- [ ] 100% files have header with `ADR 0044 §:` line citing every applicable rule
- [ ] Compliance matrix (above): 0 violations at review
- [ ] `README.md` covers FINAL discipline, dictGet, §5 divergences, validation workflow
- [ ] `run_endpoint_ch.sh` + `compare_pg_ch.sh` runnable, documented
- [ ] `git diff --name-only develop...HEAD` shows zero files outside new folder + 1 line in parent README
- [ ] No changes to `crates/`, no changes to existing PG `endpoint-queries/`, no ADR changes
- [ ] **Docs updated** — new folder + parent `database-schema/README.md` 1-line link added (per ADR 0032)
- [ ] **API types regenerated** — N/A — no API surface change

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
