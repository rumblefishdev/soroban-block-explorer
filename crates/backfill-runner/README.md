# backfill-runner

Production-grade backfill of the Soroban-era Stellar pubnet archive into
Postgres. Syncs 64k-ledger partitions from the public
`aws-public-blockchain` bucket via `aws s3 sync`, decompresses +
deserializes each ledger, and persists to the ADR 0027 schema via
`indexer::handler::process::process_ledger` — the shared parse-and-persist
contract. No reimplementation of the write path.

Also drives the ClickHouse pilot store (ADR 0044) behind
`--target clickhouse`. As of task 0206 that path writes real rows
into the 17 mirrored CH tables via partition-aligned streaming
inserts — see [Targets](#targets) below.

## Prerequisites

- The `aws` CLI on `PATH` (subprocess driver — no native SDK dependency).
  Startup fails fast if `aws --version` can't run.
- A reachable Postgres with the project schema migrated (ADR 0027).
  Startup fails fast if `SELECT 1` fails.
- **Monthly partitions exist on every partitioned parent.** The runner
  itself does **not** create partitions — assumes they're already
  provisioned (in production, by the EventBridge-triggered partition-mgmt
  Lambda; locally, by the CLI below). Without them, every ingested row
  lands in `_default`, defeating partition pruning and forcing a costly
  detach-and-migrate later. Run once before the backfill:
  ```bash
  cargo run -p db-partition-mgmt --bin cli   # uses $DATABASE_URL
  ```
  Idempotent — re-runs are a no-op once monthly children exist for the
  Soroban era. See task **0130** + `lore/3-wiki/backfill-execution-plan.md`
  for context.
- `DATABASE_URL` exported, or passed via `--database-url`.
- Run from `us-east-1` (same region as the public archive) to avoid
  cross-region ingress costs.
- Local scratch disk: **~2 × partition_size** (a couple of GB). The runner
  keeps at most the partition being indexed plus the prefetched N+1 on
  disk; each partition's folder is deleted after it fully indexes.

## Usage

```bash
# Backfill a sequence range.
cargo run -p backfill-runner -- run --start 50457424 --end 50460000

# Report per-partition progress for a range.
cargo run -p backfill-runner -- status --start 50457424 --end 50460000
```

### Flags

| Flag                | Default                  | Notes                                               |
|---------------------|--------------------------|-----------------------------------------------------|
| `--start`           | required                 | First ledger sequence (inclusive).                  |
| `--end`             | required                 | Last ledger sequence (inclusive).                   |
| `--target`          | `postgres`               | One of `postgres` \| `clickhouse`. See [Targets](#targets). |
| `--database-url`    | env `DATABASE_URL`       | Postgres DSN (required when `--target postgres`).   |
| `--clickhouse-url`  | env `CLICKHOUSE_URL`     | ClickHouse HTTP endpoint (used when `--target clickhouse`). |
| `--temp-dir`        | `.temp/backfill-runner`  | Local scratch dir (env `BACKFILL_TEMP_DIR`).        |
| `--keep-partitions` | off                      | Don't delete each partition's local folder after a successful index. Iteration / debug flag — see [Iteration](#iteration). |
| `--verbose`/`-v`    | off                      | Enable per-ledger + per-partition info logs. Without it only warnings print during the run. |

## Targets

The runner writes to one of two parallel stores, selected by `--target`.
Default is `postgres` so existing invocations (CI scripts, runbooks, the
aws-public-blockchain workflow) keep working byte-for-byte without edits.

| Target       | Status              | Required env / flag                                                                        |
|--------------|---------------------|--------------------------------------------------------------------------------------------|
| `postgres`   | production          | `--database-url` / `DATABASE_URL`                                                          |
| `clickhouse` | pilot (task 0206)   | `--clickhouse-url` / `CLICKHOUSE_URL`, plus `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` / `CLICKHOUSE_DATABASE` (defaults from `db_clickhouse::Config::from_env`) |

### `--target clickhouse`

The CH path drives a **partition-writer lifecycle** (task 0206):
`open_partition` → `write_ledger × N` → `commit` (or `abort` on
error). The CH writer
(`db_clickhouse::persist::PartitionWriter`) holds one long-lived
`clickhouse::Insert<RowT>` per CH table across the entire backfill
partition — the server only sees one `INSERT` statement per table
per partition, instead of one per ledger.

Per-ledger `INSERT` was an anti-pattern here: `MergeTree` creates
one "part" per `INSERT` statement, and the CH default
`parts_to_throw_insert = 3000` (per `(table, CH-partition)`) would
trip after the first ~3 k ledgers (~0.03 % of an 11 M-ledger
backfill). Partition-aligned streaming pushes this to ~2 400 total
parts created over the entire backfill — well within the background
merger's comfort zone. Full design rationale lives in
[`docs/architecture/database-schema/clickhouse-pilot.md`](../../docs/architecture/database-schema/clickhouse-pilot.md#writers).

```bash
# Real ClickHouse run.
CLICKHOUSE_URL=http://localhost:8123 \
    cargo run -p backfill-runner -- \
    --target clickhouse \
    run --start 62016000 --end 62016099
```

The PG variant of the partition-writer lifecycle is a no-op around
the existing per-ledger transaction — `--target postgres` behaviour
is byte-for-byte equivalent to the pre-task-0206 path.

### Parallel partition runs

The CH writer accepts concurrent inserts on the same table without
coordination — ReplacingMergeTree dedups on the merge side keyed by
`ORDER BY` (the deterministic surrogate id, derived from the
natural key). To parallelize an 11 M-ledger backfill across K
runner processes, invoke each with `--start N --end M` on **disjoint
ranges**:

```bash
# Four parallel runners across four disjoint partition ranges.
for i in 0 1 2 3; do
    START=$((62016000 + i * 64000))
    END=$((START + 63999))
    CLICKHOUSE_URL=http://localhost:8123 \
        cargo run -p backfill-runner --release -- \
        --target clickhouse \
        run --start "$START" --end "$END" &
done
wait
```

CH-side requires no setup. At K ≥ 8 in one process group, raise
`max_concurrent_queries` from the CH default (100) to ~200 — each
writer holds 14 long-lived inserts. K = 4 (14 × 4 = 56) is
comfortable on defaults.

### Iteration

A 64k-ledger partition is ~11.6 GB compressed; `aws s3 sync` against an
empty folder takes ~60 s. Default behaviour deletes each partition's
local folder right after it indexes (`partition local folder cleaned up`
log line) to bound disk at ~2 × partition_size — see [Shape](#shape).
For real backfills that's what you want.

For tight iteration loops — typically against `--target clickhouse`
where the persist path is a stub and you want to re-run the same range
many times — pass `--keep-partitions`:

```bash
CLICKHOUSE_URL=http://localhost:8123 \
    cargo run -p backfill-runner -- \
    --target clickhouse \
    --keep-partitions \
    run --start 62016000 --end 62016099
```

The first run still pays the full sync cost. Subsequent runs find a
fully-populated folder (64 000 `.xdr.zst` files for a closed partition);
the sync stage short-circuits to a sub-second file-count check and
skips the `aws s3 sync` subprocess entirely — `partition local folder
already complete — skipping aws s3 sync` in the verbose log. Public-
archive partitions are immutable once closed, so this is safe; the
"current" (in-progress) partition cannot match the count and falls
back to the normal sync path.

Drop `--keep-partitions` once you're done — long runs with it on grow
disk linearly with partition count.

### Start ledger

First Soroban-era ledger: `50_457_424` (2024-02-20, Protocol 20 go-live,
community-sourced). Cross-verify with SDF opportunistically; a small
leading gap is cheap to re-run.

## Shape

One partition at a time, sequential per-ledger inside. Exactly **one**
background task: a single-slot prefetch of partition N+1 running while
partition N indexes. No worker pool, no `JoinSet` of indexer tasks —
concurrency inside the indexer is explicitly out of scope.

After partition N finishes indexing, its local folder is deleted before
awaiting the N+1 prefetch. This bounds disk at ~2 × partition_size
regardless of range width.

## Resume & idempotency

The DB `ledgers` table is the sole source of truth — no state file, no
manifest, no marker.

Two resume filters run against the `HashSet<u32>` of completed sequences
built at startup:

1. **Pre-sync partition skip** — partitions whose clamped range
   (`max(start, p.start)..=min(end, p.end)`) is fully present in the set
   are filtered out and neither synced nor indexed. Re-running a
   fully-done range does zero S3 work and zero `process_ledger` calls.
2. **Per-ledger skip (inside a partition)** — for partitions that survive
   the pre-sync filter, `process_ledger` is skipped for any sequence
   already in the set. Handles mid-partition crashes where the partition
   is only partially in DB.

`aws s3 sync` itself is idempotent — a call against a fully-synced dir is
a LIST with no GETs — so there is no Stage A marker or file-count check.
A partial dir from a crashed run gets filled in on the next sync.

## Retry policy

- **`aws s3 sync`** — 3 attempts, 2s base delay, ×2 multiplier, 30s cap.
  Hardcoded constants in `sync.rs`; change them if the numbers drift.
- **Parse / persist errors** — not retried. Parse failures indicate a
  data-shape bug; schema / constraint violations are write-path bugs.
  Both surface immediately.
- **Missing file post-sync** — panics (`assert!`). A file absent after a
  successful `aws s3 sync` means either an archive gap or a sync bug;
  both are worth a stack trace, not a silent skip. Debug-first stance
  for the duration of 0149's write-path churn — revisit once stable.

## Observability

The `run` subcommand emits a live human-readable `tracing` stream (default
formatter). `--verbose` / `-v` is the only log-level dial — without it
the filter is `warn`, so a quiet run shows only retry warnings and
panics; with it, per-ledger and per-partition `info` events flow.
Operator-facing — meant for `tail -f` while a long backfill runs. The
`status` subcommand is the structured "how far along" query; the two do
not overlap.

Per-ledger event `ledger ingested` (verbose only):
`seq`, `partition`, `bytes`, `parse_ms`, `persist_ms`. Decompression is
intentionally **not** timed — deterministic zstd work on a fixed input
carried no diagnostic signal relative to parse/persist and was just
noise on the line. Per-partition `partition indexing complete` (verbose
only): aggregate parse / persist totals, **min / max per-ledger
total_ms**, wall clock, throughput (ledgers/s). Sync layer emits
`running aws s3 sync`, `partition sync complete` (duration + file count
+ bytes), and `warn` on each retry.

**Final run summary** is always printed via `println!` regardless of
`--verbose`, so a quiet run still leaves one "what just happened"
block: partitions processed, ledgers indexed / already in DB, parse
total, persist total, ledger time min / max, elapsed seconds.

Exit code `0` on success; unrecoverable failures **panic** (non-zero
exit + stack trace) rather than return a typed error, per the
debug-first stance noted in the Retry section.

### `status` output

```
range: 50457424..=50460000   partitions: 1
   partition       indexed / range    pending   progress
    50425856            2577 / 2577         0     100.0%
----------------------------------------------------------
       total            2577 / 2577         0     100.0%
```

`indexed` / `range` and `pending` are counted against the **clamped**
range per partition — edge partitions that stick out of the requested
window only count the in-window slice. `progress` is `indexed / range`
as a percentage.

## Disk footprint

Bounded at ~2 × partition_size by mandatory cleanup-after-index. A crash
leaves at most two partitions on disk (the one being indexed + the N+1
prefetch); both are reclaimed on the next successful iteration, and
`aws s3 sync` patches up any partial folder. On error, cleanup is
**deliberately skipped** — the broken partition stays on disk for
forensics, and `aws s3 sync` on retry fills in any missing tail instead
of re-downloading from zero. If forensics aren't needed after a failure,
`rm -rf .temp/backfill-runner/` is the recovery.

## Throughput

Reference throughput per partition is **to be measured** on a `us-east-1`
instance against the production DB. Update this section after the first
dry-run.

## Diff vs `crates/backfill-bench`

Both crates target the **same sink** (Postgres, ADR 0027, via
`process_ledger`) and use the **same unit of work** (one 64k-ledger
partition via `aws s3 sync`). `backfill-bench` is the prototype /
benchmark; `backfill-runner` is the operator-facing production tool.
They coexist intentionally.

| Axis                     | backfill-bench             | backfill-runner                      |
|--------------------------|----------------------------|--------------------------------------|
| S3 fetch                 | `aws s3 sync` subprocess   | `aws s3 sync` subprocess             |
| Scratch dir              | `.temp/`                   | `.temp/backfill-runner/` (flag)      |
| Cleanup after index      | no                         | yes (disk bounded at ~2 × partition) |
| Concurrency              | sequential + prefetch      | sequential + prefetch (same)         |
| Retry                    | none                       | 3× exp backoff on `aws s3 sync`      |
| Subcommands              | single run                 | `run`, `status`                      |
| Pre-flight checks        | none                       | `aws --version` + `SELECT 1`         |
| Resume — partition level | none                       | pre-sync skip against DB             |
| Resume — ledger level    | none                       | per-ledger skip against DB           |
| Per-stage timing logs    | minimal                    | every ledger + per-partition totals  |
| `DEFAULT` partition boot | yes (dev shortcut)         | no — assumes provisioned             |

## Nx targets

```bash
pnpm nx build rust     # cargo build --workspace
pnpm nx test rust      # cargo test --workspace
pnpm nx lint rust      # cargo clippy --workspace -- -D warnings
```
