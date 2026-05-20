---
id: '0241'
title: 'FEATURE: Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation'
type: FEATURE
status: backlog
related_adr: ['0044', '0045']
related_tasks: ['0206', '0228', '0233', '0239', '0240', '0242']
blocked_by: ['0228', '0239']
tags:
  [
    priority-high,
    effort-large,
    layer-indexer,
    layer-data,
    clickhouse,
    hetzner,
    live-ingest,
    cutover,
    hard-swap,
  ]
milestone: 1
links:
  - crates/indexer/src/handler/persist/mod.rs
  - crates/db-clickhouse/src/persist.rs
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned z M1-M3 sequencing planu (2026-05-20). Zamyka lukę D1 AC #2
      ("ledgers table no gaps through current tip") po pivocie PG→CH. Aktualnie
      crates/indexer/Cargo.toml nie depend-uje od db-clickhouse — indexer Lambda
      pisze tylko do PG. Decision: hard swap CH-only (single PR cutover, brak
      dual-write transition). Task obejmuje code change w indexer crate +
      operator runbook + empirical validation cutover.
---

# Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation

## Summary

Po pivocie na Hetzner ClickHouse jako prod data store (ADR 0044/0045 + ADR 0047),
indexer Lambda musi pisać do CH zamiast PG. Decision team (2026-05-20): hard swap,
brak dual-write. Task obejmuje (A) code change w `crates/indexer/`, (B) operator
runbook dla cutover, (C) empirical validation na żywym pipeline.

## Context

D1 AC #2 wymaga "ledgers table no gaps through current tip". Aktualnie:

- `crates/indexer/Cargo.toml` nie ma `db-clickhouse` dep → indexer Lambda pisze
  tylko do PG.
- `crates/indexer/src/handler/persist/mod.rs:88` `persist_ledger()` — 15-krokowy
  PG flow w single BEGIN/COMMIT transaction.
- `crates/db-clickhouse/src/persist.rs` ma `PartitionWriter` (production-grade)
  oraz `persist_ledger_clickhouse` (legacy/test wrapper).
- 0228 (parallel-backfill merge) kończy historical na `L_last_closed` —
  brak mechanizmu dla ledgerów `[L_last_closed + 1, current_tip]`.

API nadal czyta PG (sqlx) po deploy 0241 — to akceptowalne. API "stale window"
trwa do M2 task 0243 (API feature flag). M1 = write-path correctness, D2 =
read-path correctness.

## Implementation Plan

### Część A — kod (`crates/indexer/`)

1. **Cargo.toml**: dodać `db-clickhouse = { path = "../db-clickhouse" }`,
   `clickhouse = { workspace = true }`.
2. **`crates/indexer/src/handler/persist/mod.rs:88` `persist_ledger()`** —
   zastąpić 15-step PG flow wywołaniem `db_clickhouse::persist::PartitionWriter`
   (production interface). Gap analysis w PR: czy `PartitionWriter` pokrywa
   wszystkie 15 PG writers (`upsert_accounts`, `insert_ledger`, ...,
   `recompute_asset_aggregates` per `persist/mod.rs:223`).
3. **Idempotency**: dla CH retry strategy na HTTP errors (timeout / 5xx) —
   backoff schema [50, 200, 800] ms (analogiczny do PG retry). Replay safety
   via `ReplacingMergeTree(version)` semantics (version = `(ledger_seq, ingest_ts)`).
4. **Error handling**: CH unreachable = **fail loud** (Lambda zwraca error,
   S3 retry obsługuje re-delivery). Brak PG fallback.
5. **mTLS client config**: read cert + key + ca z Secrets Manager (zależność
   od 0239 Phase 2). Env var `CH_PROD_DOMAIN` + mounted secret bundle.
6. **Cleanup**: usunąć `sqlx` dep z `crates/indexer/Cargo.toml` jeśli żaden
   inny moduł go nie używa po refactorze.

### Część B — runbook (`docs/runbooks/live-tail-cutover.md`)

Step-by-step operator instructions:

1. **Pre-flight checks (T-0)**:
   - Verify 0228 merge complete: `clickhouse-client -q "SELECT max(sequence) FROM ledgers"`
     → expect = `L_last_closed`
   - Verify CH endpoint reachable: `curl -k https://ch-prod.../ping` → 200
   - Verify Lambda 0241 deployed: `aws lambda get-function ...` → expect post-0241 version
   - Verify mTLS cert in Secrets Manager: `aws secretsmanager get-secret-value ...`
2. **Cutover (T+0)**:
   - Enable indexer Lambda S3 trigger
   - Watch CloudWatch metric `ledger_processed_count` for 5 min — expect monotonic
3. **Verification (T+30min)**:
   - Gap check: `SELECT count(*) FROM ledgers WHERE sequence BETWEEN ... AND ...`
   - Dedup check: 1 row per sequence
   - `MAX(sequence)` matches stellarchain.io tip within 30s
4. **Rollback (jeśli potrzebne)**:
   - Disable Lambda trigger
   - Roll back to pre-0241 Lambda version
   - Document manual re-replay z S3 backlogu po next-attempt
5. **Monitoring 24h post-cutover**:
   - CloudWatch alarms (GalexieLagAlarm, custom CH-write-error alarm)
   - CH disk usage growth rate — expect ~linear
   - Ledger lag metric — expect <30s steady state

### Część C — walidacja empiryczna

- Wykonać cutover na staging-Hetzner (lub directly prod jeśli single-shot).
- Captured observations w runbook (timings, surprises, edge cases) — analogicznie
  do task 0233 pattern "best executed alongside first real cutover".
- Lessons learned wpisane do runbook post-execution.

## Acceptance Criteria

- [ ] `cargo check -p indexer` clean bez `sqlx` dep
- [ ] Lambda deploy z mTLS connection do Hetzner CH (env var `CH_PROD_DOMAIN` + mounted secret)
- [ ] Smoke test: ledger N writes do CH, query `SELECT * FROM ledgers WHERE sequence = N` zwraca row
- [ ] 39 existing indexer tests rewritten lub gated (CH-only test path)
- [ ] Replay safety: re-deliver S3 eventu = no duplicates w CH (`ReplacingMergeTree(version)` verified)
- [ ] Error path: CH unreachable → Lambda fails, CloudWatch log "ClickHouse unreachable", S3 retry kicks in (verified via toxiproxy lub manual CH stop)
- [ ] `docs/runbooks/live-tail-cutover.md` authored + reviewed
- [ ] Empirycznie wykonany cutover: no ledger gap, no double-write corruption
- [ ] Monitoring: CloudWatch metric "ledger lag" <30s post-cutover (zgodnie z D3 AC #1)
- [ ] Rollback path udokumentowany + test-runed
- [ ] Lessons learned wpisane do runbook (post-execution edit)
- [ ] **Docs updated** — `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
      reflects CH write path (replaces PG write path description)
- [ ] **API types regenerated** — N/A — task does not touch `crates/api/**`,
      `Cargo.{toml,lock}` (root), `libs/api-types/**`

## Depends on

- **0239 Phase 2** (mTLS connection layer dla AWS Lambdas → Hetzner CH) — technical blocker
- **0228** (historical CH ready jako baseline; cutover bez działającego merge'a nie ma sensu) — technical blocker
- **0233** (merge runbook — para z live-tail runbook, complementary docs) — soft dependency
- **0242** — NOT a blocker. ADR ratification jest post-factum dokumentacją per lore convention (`lore/2-adrs/CLAUDE.md`: "Written post-factum after implementation."). 0241 code może lecieć przed 0242 ADR.

## Open questions

- **`PartitionWriter` vs `persist_ledger_clickhouse` wrapper**: wrapper jest
  "for legacy/test single-ledger callers"; production powinno driver'ować
  `PartitionWriter`. Może wymagać małej refaktoryzacji db-clickhouse interfejsu.
- **CH replay semantics**: jeśli `ReplacingMergeTree(version)` nie wystarczy do
  idempotency (np. version collision on re-delivery), projekt sentinelowy:
  `INSERT IGNORE`, dedup table, lub query-time dedup. Decyzja w PR.

## Notes

Po deploy 0241 PG nie dostaje nowych ledgerów — to po decyzji "hard swap". API
nadal czyta PG do M2 (task 0243 feature flag). "Stale window" akceptowalny przez
zespół jako trade-off przy braku dual-write transition.
