---
id: '0419'
title: 'OPS: net-settled value column — prod rollout (ALTER → deploy → S3 re-ingest → assets.id → bloom) + post-reingest verification'
type: OPS
status: backlog
related_adr: ['0044']
related_tasks: ['0393', '0417', '0383']
tags:
  [
    'clickhouse',
    'indexer',
    'ops',
    'deployment',
    'phase-future',
    'effort-medium',
    'priority-high',
  ]
links:
  - crates/db-clickhouse/schema/init.sql
history:
  - date: '2026-07-21'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0393 (done). The value-column CODE is merged + verified on real
      mainnet data, but the prod rollout is a hand-run, ORDER-DEPENDENT DB sequence
      that must not live as prose in an archived task. Carries every operational
      step + an explicit post-S3-reingest verification (re-cross-validate historical
      net_settled against external sources, not just trust the re-run).
---

# OPS: net-settled value column — prod rollout + post-reingest verification

## Summary

Task 0393 shipped the code that computes `net_settled` per (tx, asset) from the
**ledger** at ingest. Turning it on in prod is a hand-run ClickHouse sequence
where **ordering matters** — the indexer INSERTs the new column, so the column
must exist BEFORE the new indexer deploys or every `operation_asset_appearances`
insert fails `Code 16: No such column` and ingestion halts. History (classic +
Soroban) only materialises via a full **S3 re-ingest** (there is no CH-local value
backfill — `TransactionMeta` is S3-only). This task tracks the rollout to done and
**verifies the historical values after the re-ingest** against external truth.

## Context

- Parent: [[0393]] (archived, done — code + real-data cross-validation).
- Release gate for the READ path is owned by [[0417]] (the `(ledger,tx)` companion);
  this task is the WRITE/backfill rollout up to that gate.
- Engine is version-less `ReplacingMergeTree` — the column add is a plain additive
  `ALTER`, no rebuild. `init.sql` already reflects the desired schema; prod is
  migrated by hand.

## Implementation — ordered prod steps

1. **Add the column (additive, before the new indexer deploys):**
   ```sql
   ALTER TABLE operation_asset_appearances ADD COLUMN net_settled Nullable(Int128);
   ```
   Existing rows read `NULL` (= "not computed"); the read's `HAVING … IS NOT NULL`
   hides them, so no wrong value shows pre-backfill.
2. **Deploy the new indexer** — live-forward values start writing.
3. **Full S3 re-ingest** to fold value into history (classic + Soroban) by re-running
   `stage.rs` over every ledger. **Do NOT run the 0383 token-flow backfill after the
   column exists** — it is presence-only (`net_settled: NULL`) and could win the
   version-less RMT merge and blank a live-computed value.
4. **Confirm `assets.id` backfilled** for the range — the value read does
   `INNER JOIN assets ON a.id = asset_id`; un-backfilled rows have `id = 0`, join
   nothing, and those values silently vanish.
5. **Add + `MATERIALIZE` the bloom skip index:**
   ```sql
   ALTER TABLE operation_asset_appearances
     ADD INDEX idx_oaa_transaction_id transaction_id TYPE bloom_filter(0.001) GRANULARITY 1;
   ALTER TABLE operation_asset_appearances MATERIALIZE INDEX idx_oaa_transaction_id;
   ```
   Until `MATERIALIZE` finishes only new parts are granule-pruned.
6. **RELEASE GATE** — do NOT expose the value read on prod polling until [[0417]]
   (the `(ledger,tx)` companion) lands OR a mature-partition load test clears the
   scan + two `FINAL` joins. `wants_values` scopes the cost to the single global tx
   list — keep it gated there until this passes.

## Post-S3-reingest verification (REQUIRED — do not just trust the re-run)

The re-ingest recomputes history; verify it the same adversarial way 0393 verified
the live path — historical values must match external truth, not just "the code ran".

7. **Re-run the gated corpus + E2E against prod-shaped data.** The 8-fixture corpus
   (`net_settled_real_corpus.rs`) + prod-resolver E2E already pin the reducer; after
   re-ingest, spot-check that the SAME transactions now carry the expected stored
   `net_settled` in prod CH (not just in the test harness).
8. **Cross-validate a sample of RE-INGESTED historical txs** (spanning eras: pre-Soroban
   classic, SAC-wrapped, bespoke type-3) against **two independent sources** — Horizon
   `/effects` + stellar CLI XDR decode (stellar.expert `meta` for unlimited retention)
   — 1:1 on at least the deltas, per the 0393 method. Pull tx hashes from prod, decode
   their meta, compare stored value vs recomputed.
9. **Confirm no NULL-blanking / no stale-`max` inflation:** verify (a) no presence-only
   NULL rows overwrote a computed value (0383 backfill ordering), and (b) where a value
   is known-wrong from a later reducer fix, budget `OPTIMIZE TABLE
operation_asset_appearances FINAL` over the affected partitions — `max(net_settled)`
   is a one-way ratchet, so a smaller corrected value stays invisible until old rows
   physically merge away.

## Acceptance Criteria

- [ ] Column added on prod before the new indexer deploy (no `Code 16` halt)
- [ ] New indexer deployed; live-forward values landing
- [ ] Full S3 re-ingest completed over the target range; 0383 backfill NOT re-run afterward
- [ ] `assets.id` confirmed backfilled for the range (no `id = 0` drop)
- [ ] Bloom index added + `MATERIALIZE`d
- [ ] Read path kept gated until 0417 or a mature-partition load test clears it
- [ ] Post-reingest: sample of historical txs (classic + SAC + bespoke, multiple eras)
      cross-validated 1:1 vs Horizon + stellar CLI
- [ ] Verified no NULL-blanking and stale-`max` handled (OPTIMIZE FINAL budgeted if needed)
