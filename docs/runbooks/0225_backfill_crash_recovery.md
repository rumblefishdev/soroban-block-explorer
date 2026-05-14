# Runbook: 0225 — backfill-runner crash recovery

**Task:** [0225 — backfill-runner sync validation + crash recovery](../../lore/1-tasks/backlog/0225_FEATURE_backfill-sync-validation-and-resume-runbook.md)
**Target:** PostgreSQL or ClickHouse backfill (CH-focused below; PG is structurally simpler)
**Idempotent:** yes — `ALTER DELETE … WHERE` matches 0 rows on second pass; CH `ReplacingMergeTree` dedupes resume rows on next merge
**Frequency:** as needed — fires when `backfill-runner` panics mid-partition

---

## When to use

`backfill-runner` panicked mid-partition. Symptoms:

```
panic at ingest.rs:NNN: ledger file missing post-sync: partition=NN seq=NN path=…
```

Or any other panic during parse / persist that aborts the runner before
`writer.commit()` is reached.

CH state post-crash, per `db_clickhouse::persist::writer.rs:40-48`
(commit-marker pattern):

- **18 streaming tables** (`transactions`, `operations_appearances`,
  `soroban_events`, `transaction_participants`, `transaction_hash_index`,
  `soroban_invocations_appearances`, `assets`, `nfts`, `nft_ownership`,
  `nfts_pending`, `nft_ownership_pending`, `account_balances_current`,
  `accounts`, `soroban_contracts`, `wasm_interface_metadata`,
  `liquidity_pools`, `liquidity_pool_snapshots`, `lp_positions`) —
  may have partial rows from the failed partition. Streaming inserts
  flush server-side parts continuously, so anything that streamed before
  the panic is persisted.
- **`ledgers` table** — no rows for the failed partition. The
  commit-marker pattern buffers `LedgerRow`s in RAM and flushes in
  `commit()` after every streaming table acks. Panic before `commit()`
  → buffer lost → `ledgers` has nothing for the partition.

Net effect: orphan rows visible in queries against streaming tables
(rows reference `ledger_sequence` not present in `ledgers`).

---

## Root causes commonly seen

### S3 archive lag (most common)

Recent partitions on `s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/`
may not be fully uploaded yet — Stellar's archive writer lags real-time
mainnet by hours. `aws s3 sync` **does not error on missing objects** —
it silently downloads what is there. Indexer then panics on the first
missing local `.xdr.zst`.

Example seen on 2026-05-14: partition 9 (62528000-62591999) was 56 %
uploaded (35 795 / 64 000 files) at sync time. File
`FC4579AA--62555733.xdr.zst` was uploaded to S3 at `2026-05-14 00:46:12`,
**hours after** the backfill sync ran. Backfill crashed at ledger
62 555 733; once S3 caught up, the file existed and resume succeeded.

### Filesystem corruption / accidental deletion

Local `.xdr.zst` file disappears between `aws s3 sync` and `parse`.
Rare; usually a separate cleanup script or disk error.

### Parse / persist panic on a malformed XDR

Indexer hits an unhandled XDR shape and panics. Same recovery path —
the panic kills the writer's `commit()` step regardless of the cause.

---

## Recovery steps

### Step 1 — identify the failed partition

```sql
-- ClickHouse
SELECT
    max(sequence)     AS last_complete_ledger,
    max(sequence) + 1 AS first_failed_ledger,
    intDiv(max(sequence) + 1, 64000) * 64000 AS failed_partition_start,
    intDiv(max(sequence) + 1, 64000) * 64000 + 63999 AS failed_partition_end
FROM ledgers;
```

If `first_failed_ledger > <backfill range end>`, the run completed
cleanly — no recovery needed. Otherwise note `failed_partition_start`
and `failed_partition_end` for Step 3.

### Step 2 — clean orphan rows from streaming tables

For each ledger-keyed streaming table, delete rows referencing ledgers
the partition never committed:

```sql
ALTER TABLE transactions                       DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE operations_appearances             DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE soroban_events                     DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE soroban_invocations_appearances    DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE transaction_participants           DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE transaction_hash_index             DELETE WHERE ledger_sequence > <last_complete_ledger>;
-- accounts / soroban_contracts / assets / nfts / nft_ownership rows
-- carry last_seen_ledger / wasm_uploaded_at_ledger / current_owner_ledger
-- — adjust column per table; consult crates/db-clickhouse/schema/init.sql.
ALTER TABLE accounts                           DELETE WHERE last_seen_ledger > <last_complete_ledger>;
ALTER TABLE soroban_contracts                  DELETE WHERE wasm_uploaded_at_ledger > <last_complete_ledger>;
ALTER TABLE account_balances_current           DELETE WHERE last_updated_ledger > <last_complete_ledger>;
ALTER TABLE nfts                               DELETE WHERE current_owner_ledger > <last_complete_ledger>;
ALTER TABLE nft_ownership                      DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE nfts_pending                       DELETE WHERE current_owner_ledger > <last_complete_ledger>;
ALTER TABLE nft_ownership_pending              DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE lp_positions                       DELETE WHERE last_updated_ledger > <last_complete_ledger>;
ALTER TABLE liquidity_pools                    DELETE WHERE ledger_sequence > <last_complete_ledger>;
ALTER TABLE liquidity_pool_snapshots           DELETE WHERE last_updated_ledger > <last_complete_ledger>;
```

`assets` and `wasm_interface_metadata` don't carry a per-row ledger
watermark; their rows are global state and the resume re-emits dedupes
via RMT ORDER BY key without an explicit DELETE.

These `ALTER … DELETE` mutations are **async on CH**. Monitor:

```sql
SELECT command, is_done, latest_fail_reason, create_time
  FROM system.mutations
 WHERE table IN ('transactions','operations_appearances','soroban_events',
                 'soroban_invocations_appearances','transaction_participants',
                 'transaction_hash_index','accounts','soroban_contracts',
                 'account_balances_current','nfts','nft_ownership',
                 'nfts_pending','nft_ownership_pending','lp_positions',
                 'liquidity_pools','liquidity_pool_snapshots')
   AND create_time > now() - INTERVAL 1 HOUR
 ORDER BY create_time DESC;
```

Wait for every row to show `is_done = 1` before proceeding.

### Step 3 — verify the cleanup

```sql
SELECT
    (SELECT max(sequence) FROM ledgers)                 AS max_ledgers,
    (SELECT max(ledger_sequence) FROM transactions FINAL) AS max_tx,
    (SELECT count() FROM transactions FINAL
       WHERE ledger_sequence > (SELECT max(sequence) FROM ledgers)) AS orphan_tx;
```

Expect: `max_tx <= max_ledgers` and `orphan_tx = 0`.

### Step 4 — re-run the failed partition

For **S3 archive lag** causes, verify the missing files now exist on
the bucket before resuming:

```bash
aws s3 ls --no-sign-request \
    s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/<HEX>--<start>-<end>/ \
    --recursive | wc -l
# Expect 64000. If less, wait — archive is still uploading.
```

Then resume the run from the failed partition's start:

```bash
target/release/backfill-runner \
    --target clickhouse \
    --clickhouse-url http://localhost:8124 \
    --soroban-rpc-url https://mainnet.sorobanrpc.com \
    --temp-dir <temp-dir> \
    run --start <failed_partition_start> --end <range_end>
```

ReplacingMergeTree dedupes any rows the panic-truncated-but-still-
persisted streaming inserts might have left server-side — the
`ORDER BY` keys (e.g. `(ledger_sequence, application_order, id)` on
`transactions`) match exactly between the failed-attempt rows and the
resume-attempt rows, so RMT keeps one on the next background merge.
Step 2 cleans the rows up-front; the RMT dedupe is the backstop.

### Step 5 — final verification

Re-run the Step 3 verification once the resumed partition completes.
All three values should now match `max_tx == max_ledgers` with
`orphan_tx = 0`.

---

## Why this is safe

- **`ALTER DELETE` is idempotent** — running twice deletes nothing the
  second time.
- **ReplacingMergeTree dedupes on ORDER BY key** — replay rows from
  the failed-then-resumed partition collapse to one on next merge.
- **Streaming insert parts are persisted server-side independently of
  the HTTP request lifecycle** — abort just stops the stream, never
  corrupts existing parts. CH does not have a "transactional rollback"
  for parts; the merger does the equivalent via RMT.
- **The `ledgers` commit marker is the only "is this partition
  complete" signal** — its absence guarantees the partition is a
  legitimate resume target.

## Why this happens at all

Per `writer.rs:14-26`, per-ledger atomic commit would create ~200 M
parts across 11 M ledgers × 18 tables, blowing past CH's
`parts_to_throw_insert=3000` per `(table, partition)` limit. The
commit-marker pattern keeps part counts under control at the cost of
making mid-partition crashes leave streaming-table orphans — a
documented design trade-off, recoverable via this runbook.

## See also

- `crates/db-clickhouse/src/persist/writer.rs:1-56` — commit-marker
  pattern rationale + memory budget
- `crates/backfill-runner/src/sync.rs` — `aws s3 sync` driver
- `crates/backfill-runner/src/ingest.rs:170-200` — per-ledger ingest
  loop where panics typically surface
- [task 0225](../../lore/1-tasks/backlog/0225_FEATURE_backfill-sync-validation-and-resume-runbook.md)
  — pre-emptive sync validation work (reduces frequency of this runbook
  firing)
