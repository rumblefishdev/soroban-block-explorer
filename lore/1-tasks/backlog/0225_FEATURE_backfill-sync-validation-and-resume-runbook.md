---
id: '0225'
title: 'FEATURE: backfill-runner sync validation pre-parse + crash-recovery runbook'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0204', '0205', '0214', '0220']
tags:
  [
    'clickhouse',
    'backfill-runner',
    'reliability',
    'phase-pre-backfill',
    'effort-small',
    'priority-high',
  ]
links:
  - 'crates/db-clickhouse/src/persist/writer.rs'
  - 'crates/backfill-runner/src/ingest.rs'
history:
  - date: 2026-05-14
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned after diagnosing the 512k pilot panic at
      `ingest.rs:176 — ledger file missing post-sync: seq=62555733`.
      Initial diagnosis ("writer ordering bug, 7.34M orphan tx") was
      wrong. Root cause: AWS S3 archive lag for recent partitions —
      partition 9 (62528000-62591999) had 35,795 / 64,000 files on S3
      at sync time (file `FC4579AA--62555733.xdr.zst` uploaded
      2026-05-14 00:46:12, hours after sync). `aws s3 sync` does not
      error on missing files, silently downloads partial set; indexer
      then panics on first missing local file. Per `writer.rs:40-48`,
      this is **by-design recovery** — orphan rows dedupe via
      ReplacingMergeTree on partition re-run. But: panic-then-no-resume
      leaves orphan tx until operator manually re-runs. Better to
      prevent the panic entirely + document operator recovery.
---

# FEATURE: backfill-runner sync validation pre-parse + crash-recovery runbook

## Summary

Two complementary changes to handle recent-partition S3 archive lag:

1. **Pre-parse sync validation** — before parsing a partition, verify
   local file count matches `PARTITION_SIZE`. If incomplete, check S3
   directly: if S3 itself is incomplete (archive lag), skip the
   partition gracefully (log + return SkipReason). If S3 is complete
   but local is missing, retry sync. Only panic on a genuine
   filesystem error.
2. **Crash-recovery runbook** — operator-facing doc for the case where
   a panic still happens (e.g. mid-parse error unrelated to sync).
   Steps: clean orphan rows via `ALTER DELETE` per ledger-keyed table,
   re-run the failed partition. RMT dedupes the second-run rows.

## Context

`crates/db-clickhouse/src/persist/writer.rs:40-48` documents the
commit-marker pattern: 18 streaming tables (transactions, ops, events,
etc.) flush rows to CH continuously throughout a partition; `ledgers`
table is buffered in-memory and flushed once at `commit()` as an
atomic "partition complete" marker. Mid-partition panic → streaming
tables have already persisted parts server-side, `ledgers` buffer is
lost → orphan rows visible in queries until operator re-runs the
partition. Per the writer comment:

> "Mid-partition failure ⇒ no ledgers rows for that range ⇒ resume
> re-does the whole partition. The orphan rows (if any) on the
> partial first attempt dedupe under ReplacingMergeTree on the next
> background merge."

This design is **architecturally optimal for CH** at 11M+ ledgers —
per-ledger atomic commit would create ~200M parts and trip
`parts_to_throw_insert=3000`. Trade-off: recovery requires operator
action.

## Implementation Plan

### Part 1 — Sync validation in `ingest.rs`

**File:** `crates/backfill-runner/src/ingest.rs` (around line 176 where
panic currently fires).

Before `parse_partition`:

```rust
let synced_count = std::fs::read_dir(&partition_local)?
    .filter(|e| e.as_ref().map_or(false, |entry|
        entry.path().extension().is_some_and(|ext| ext == "zst")))
    .count();

if synced_count < PARTITION_SIZE as usize {
    // Check S3 directly — is it the archive lag, or a partial local sync?
    let s3_count = aws_s3_ls_count(&p.s3_folder()).await?;
    if s3_count < PARTITION_SIZE as usize {
        warn!(
            partition_start = p.start,
            local_files = synced_count,
            s3_files = s3_count,
            need = PARTITION_SIZE,
            "S3 archive lag — partition incomplete on S3, skipping"
        );
        return Ok(IngestOutcome::SkippedS3Incomplete);
    }
    // S3 is complete; local sync was partial (network glitch). Retry.
    info!(partition_start = p.start, "local sync partial despite full S3 — retrying sync");
    aws_s3_sync(&p, &partition_local).await?;
    // Re-validate
    let recount = std::fs::read_dir(&partition_local)?.count();
    if recount < PARTITION_SIZE as usize {
        return Err(BackfillError::PartitionSyncFailed { /* ... */ });
    }
}
```

Add new `IngestOutcome` variant `SkippedS3Incomplete`. Runner emits
log + continues to next partition (or stops if it's the tail).

**Helper `aws_s3_ls_count`** — `aws s3 ls --recursive` via tokio
process, count lines. Anonymous credentials (`--no-sign-request`)
since archive is public.

### Part 2 — Tests

`crates/backfill-runner/tests/sync_validation.rs`:

- Test: full local + full S3 → proceeds to parse.
- Test: empty local + partial S3 → returns `SkippedS3Incomplete` (no
  network call to actual S3; mock via temp dir + custom S3 mock client
  if helper supports it, else manual integration with public bucket
  for a known incomplete partition).
- Test: partial local + full S3 → retries sync, succeeds.
- Test: partial local + partial S3 → returns `SkippedS3Incomplete`.

### Part 3 — Runbook

New file `docs/runbooks/0225_backfill_crash_recovery.md`:

````markdown
# Runbook: backfill-runner crash recovery

## When to use

Backfill-runner panicked mid-partition. Symptoms:

- `panic at ingest.rs:NNN: ledger file missing post-sync: ...`
- Or any other parse-time / persist-time panic.

CH state post-crash: streaming tables (transactions, operations, events,
etc.) have partial rows from the failed partition; `ledgers` table has
no rows for the failed partition (commit-marker pattern).

## Recovery steps

### Step 1 — identify failed partition

```sql
SELECT
    max(sequence) AS last_complete_ledger,
    max(sequence) + 1 AS first_failed_ledger
FROM ledgers;
```
````

If `first_failed_ledger > backfill_range_end`, no recovery needed (run
completed). Otherwise compute the failed partition's start/end via
`intDiv(first_failed_ledger, 64000) * 64000`.

### Step 2 — clean orphan rows from streaming tables

```sql
-- Delete rows from each streaming table that reference ledgers > last_complete_ledger
ALTER TABLE transactions DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE operations_appearances DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE soroban_events DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE soroban_invocations_appearances DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE transaction_participants DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE transaction_hash_index DELETE WHERE ledger_sequence > <last_complete_ledger>;
-- ... plus all other ledger-keyed tables (consult schema)
```

These are async on CH. Monitor via:

```sql
SELECT command, is_done, latest_fail_reason
FROM system.mutations
WHERE table IN ('transactions','operations_appearances','soroban_events',…)
  AND create_time > now() - INTERVAL 1 HOUR
ORDER BY create_time DESC;
```

Wait until all `is_done=1`.

### Step 3 — re-run failed partition

```bash
backfill-runner --target clickhouse --clickhouse-url <url> run \
    --start <failed_partition_start> --end <failed_partition_end>
```

RMT dedupes any rows the panic-truncated-but-still-persisted streaming
inserts might have left.

### Step 4 — verify

```sql
SELECT
    (SELECT max(sequence) FROM ledgers) AS max_ledgers,
    (SELECT max(ledger_sequence) FROM transactions FINAL) AS max_tx,
    (SELECT count() FROM transactions FINAL
       WHERE ledger_sequence > (SELECT max(sequence) FROM ledgers)) AS orphan_tx;
```

Expect: `max_ledgers >= max_tx` and `orphan_tx = 0`.

## Why this is safe

- `ALTER DELETE` is idempotent — running twice deletes nothing the
  second time.
- RMT (`ReplacingMergeTree`) dedupes on `ORDER BY` key — replay rows
  collapse to one on next merge.
- Streaming inserts' parts are persisted server-side independently of
  HTTP request lifecycle; abort just stops the stream, never corrupts
  parts.

```

## Acceptance Criteria

- [ ] `aws_s3_ls_count` helper added to `crates/backfill-runner`
      (anonymous S3 ls via tokio).
- [ ] Pre-parse sync validation in `ingest.rs` per Part 1; new
      `IngestOutcome::SkippedS3Incomplete` variant.
- [ ] Unit/integration tests per Part 2.
- [ ] Runbook `docs/runbooks/0225_backfill_crash_recovery.md` committed.
- [ ] Empirical test: re-run partition 9 (62528000-62591999) after S3
      catches up (file `FC4579AA--62555733.xdr.zst` now exists at
      2026-05-14 00:46:12). Verify resume succeeds and RMT dedupes
      previous-attempt orphan rows.
- [ ] **Docs updated** — `crates/backfill-runner/README.md` (or module
      doc) gains a "Recent partitions / archive lag" section linking
      to the runbook.
- [ ] **API types regenerated** — N/A (no `crates/api/**` change).

## Out of Scope

- **Writer architecture refactor** — current commit-marker pattern is
  optimal for CH part economy. Documented in `writer.rs:40-48`.
- **Two-phase `partitions_complete` audit table** — clean design but
  requires schema migration + endpoint query rewrite. Defer to post-
  pilot.
- **Sub-partition checkpointing (commit every 8k ledgers)** — would
  reduce replay cost but 8× narzut on merger. Skip unless recovery
  cost becomes a bottleneck.

## Notes

- Archive lag affects only the **most recent** partitions (last hours
  / days). Full Soroban-era backfill (50.5M → ~62.5M) hits this only
  at the tail. Mitigation: stop backfill ~50k ledgers before mainnet
  tip; resume in follow-up window once S3 catches up.
- Sync validation is also a guard against accidental `aws s3 sync`
  network glitches that leave a partition partially synced locally
  without ever being incomplete on S3.
```
