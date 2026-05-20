---
id: '0241'
title: 'FEATURE: Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation'
type: FEATURE
status: backlog
related_adr: ['0044', '0045', '0047']
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
      Spawned from M1-M3 sequencing plan (2026-05-20). Closes the D1 AC #2 gap
      ("ledgers table no gaps through current tip") after the PG→CH pivot.
      Currently `crates/indexer/Cargo.toml` does not depend on `db-clickhouse`
      — the indexer Lambda writes to PG only. Decision: hard swap CH-only
      (single PR cutover, no dual-write transition). Task covers code change
      in the indexer crate + operator runbook + empirical cutover validation.
---

# Indexer Lambda hard swap PG→CH + live-tail cutover runbook + empirical validation

## Summary

After the pivot to Hetzner ClickHouse as the prod data store (ADR 0044/0045 +
ADR 0047), the indexer Lambda must write to CH instead of PG. Team decision
(2026-05-20): hard swap, no dual-write. Task covers (A) code change in
`crates/indexer/`, (B) operator runbook for the cutover, (C) empirical
validation against the live pipeline.

## Context

D1 AC #2 requires "ledgers table no gaps through current tip". Currently:

- `crates/indexer/Cargo.toml` does not include a `db-clickhouse` dep → the
  indexer Lambda writes to PG only.
- `crates/indexer/src/handler/persist/mod.rs:88` `persist_ledger()` is a
  15-step PG flow inside a single BEGIN/COMMIT transaction.
- `crates/db-clickhouse/src/persist.rs` exposes `PartitionWriter`
  (production-grade) and `persist_ledger_clickhouse` (legacy/test wrapper).
- 0228 (parallel-backfill merge) ends historical at `L_last_closed` — no
  mechanism in place for ledgers `[L_last_closed + 1, current_tip]`.

The API continues to read PG (sqlx) after deploying 0241 — that is acceptable.
The API "stale window" lasts until M2 task 0243 (API feature flag). M1 =
write-path correctness, D2 = read-path correctness.

## Implementation Plan

### Part A — code (`crates/indexer/`)

1. **Cargo.toml**: add `db-clickhouse = { path = "../db-clickhouse" }` and
   `clickhouse = { workspace = true }`.
2. **`crates/indexer/src/handler/persist/mod.rs:88` `persist_ledger()`** —
   replace the 15-step PG flow with a call into
   `db_clickhouse::persist::PartitionWriter` (production interface). Gap
   analysis in the PR: does `PartitionWriter` cover all 15 PG writers
   (`upsert_accounts`, `insert_ledger`, …, `recompute_asset_aggregates` per
   `persist/mod.rs:223`).
3. **Idempotency**: CH retry strategy for HTTP errors (timeout / 5xx) —
   backoff schedule [50, 200, 800] ms (mirrors the PG retry shape). Replay
   safety via `ReplacingMergeTree(version)` semantics
   (version = `(ledger_seq, ingest_ts)`).
4. **Error handling**: CH unreachable = **fail loud** (Lambda returns an
   error, S3 retry handles re-delivery). No PG fallback.
5. **mTLS client config**: read cert + key + ca from Secrets Manager
   (depends on 0239 Phase 2). Env var `CH_PROD_DOMAIN` + mounted secret
   bundle.
6. **Cleanup**: remove the `sqlx` dep from `crates/indexer/Cargo.toml` if no
   other module in the crate needs it after the refactor.

### Part B — runbook (`docs/runbooks/live-tail-cutover.md`)

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
3. **Verification (T+30 min)**:
   - Gap check: `SELECT count(*) FROM ledgers WHERE sequence BETWEEN ... AND ...`
   - Dedup check: 1 row per sequence
   - `MAX(sequence)` matches stellarchain.io tip within 30 s
4. **Rollback (if needed)**:
   - Disable Lambda trigger
   - Roll back to pre-0241 Lambda version
   - Document manual re-replay from the S3 backlog before the next attempt
5. **Monitoring 24 h post-cutover**:
   - CloudWatch alarms (GalexieLagAlarm, custom CH-write-error alarm)
   - CH disk usage growth rate — expect ~linear
   - Ledger lag metric — expect <30 s steady state

### Part C — empirical validation

- Execute the cutover on staging-Hetzner (or directly on prod if single-shot).
- Capture observations in the runbook (timings, surprises, edge cases) —
  mirrors the task 0233 pattern of "best executed alongside the first real
  cutover".
- Write lessons learned back into the runbook post-execution.

## Acceptance Criteria

- [ ] `cargo check -p indexer` clean with no `sqlx` dep
- [ ] Lambda deploy with mTLS connection to Hetzner CH (env var `CH_PROD_DOMAIN` + mounted secret)
- [ ] Smoke test: ledger N writes to CH, query `SELECT * FROM ledgers WHERE sequence = N` returns the row
- [ ] 39 existing indexer tests rewritten or gated (CH-only test path)
- [ ] Replay safety: re-delivering an S3 event = no duplicates in CH (`ReplacingMergeTree(version)` verified)
- [ ] Error path: CH unreachable → Lambda fails, CloudWatch logs "ClickHouse unreachable", S3 retry kicks in (verified via toxiproxy or manual CH stop)
- [ ] `docs/runbooks/live-tail-cutover.md` authored and reviewed
- [ ] Cutover executed empirically: no ledger gap, no double-write corruption
- [ ] Monitoring: CloudWatch metric "ledger lag" <30 s post-cutover (matches D3 AC #1)
- [ ] Rollback path documented and test-runed
- [ ] Lessons learned written into the runbook (post-execution edit)
- [ ] **Docs updated** — `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
      reflects the CH write path (replaces the PG write path description)
- [ ] **API types regenerated** — N/A — task does not touch `crates/api/**`,
      `Cargo.{toml,lock}` (root), or `libs/api-types/**`

## Depends on

- **0239 Phase 2** (mTLS connection layer for AWS Lambdas → Hetzner CH) — technical blocker
- **0228** (historical CH ready as a baseline; cutover without a working merge has no value) — technical blocker
- **0233** (merge runbook — pairs with the live-tail runbook, complementary docs) — soft dependency
- **0242** — NOT a blocker. ADR ratification is post-factum documentation per
  the lore convention (`lore/2-adrs/CLAUDE.md`: "Written post-factum after
  implementation."). 0241 code can ship before 0242's ADR.

## Open questions

- **`PartitionWriter` vs `persist_ledger_clickhouse` wrapper**: the wrapper is
  "for legacy/test single-ledger callers"; production should drive
  `PartitionWriter`. May require a small refactor of the `db-clickhouse`
  interface.
- **CH replay semantics**: if `ReplacingMergeTree(version)` is not enough for
  idempotency (e.g. version collision on re-delivery), a sentinel design is
  needed: `INSERT IGNORE`, dedup table, or query-time dedup. Decision in PR.

## Notes

After deploying 0241, PG no longer receives new ledgers — by design ("hard
swap"). The API still reads PG until M2 (task 0243 feature flag). The "stale
window" is accepted by the team as a trade-off for skipping the dual-write
transition.
