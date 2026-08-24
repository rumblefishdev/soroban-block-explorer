# backfill-runner

Production-grade backfill of the Soroban-era Stellar pubnet archive into
ClickHouse (ADR 0044). Syncs 64k-ledger partitions from the public
`aws-public-blockchain` bucket via `aws s3 sync`, decompresses +
deserializes each ledger, and persists via the `db_clickhouse::persist`
partition-writer lifecycle — reusing the shared `indexer` parse path. No
reimplementation of the write path.

Real rows are written into the CH tables via partition-aligned streaming
inserts — see [Writes](#writes) below.

## Prerequisites

- The `aws` CLI on `PATH` (subprocess driver — no native SDK dependency).
  Startup fails fast if `aws --version` can't run.
- A reachable ClickHouse with the schema applied (`db-clickhouse-init` /
  `init.sql`). Startup fails fast if `SELECT 1` fails. ClickHouse
  partitions declaratively (`PARTITION BY intDiv(ledger_sequence, 500000)`),
  so there is no partition-provisioning step.
- `CLICKHOUSE_URL` exported (or `--clickhouse-url`); `CLICKHOUSE_USER` /
  `CLICKHOUSE_PASSWORD` / `CLICKHOUSE_DATABASE` default via
  `db_clickhouse::Config::from_env`. For the mTLS Caddy endpoint pass
  `--ch-cert` / `--ch-key` / `--ch-ca` (task 0307).
- Run from `us-east-1` (same region as the public archive) to avoid
  cross-region ingress costs.
- Local scratch disk: **under ~2 × partition_size**. The runner keeps at
  most the partition being indexed plus the prefetched N+1 on disk, and
  releases each ledger file as it is indexed, so the indexing partition
  shrinks as it goes. Scratch must live on a filesystem the database
  cannot allocate from (task 0488).

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
| `--clickhouse-url`  | env `CLICKHOUSE_URL`     | ClickHouse HTTP endpoint.                            |
| `--ch-cert`/`--ch-key`/`--ch-ca` | env `CLICKHOUSE_CERT`/`_KEY`/`_CA` | mTLS PEMs for the Caddy endpoint (all three or none). |
| `--temp-dir`        | `.temp/backfill-runner`  | Local scratch dir (env `BACKFILL_TEMP_DIR`).        |
| `--keep-partitions` | off                      | Don't delete each partition's local folder after a successful index. Iteration / debug flag — see [Iteration](#iteration). |
| `--verbose`/`-v`    | off                      | Enable per-ledger + per-partition info logs. Without it only warnings print during the run. |

## Subcommands — the rule for one-off passes

Beyond `run` / `status`, this crate hosts maintenance subcommands that operate on
an already-ingested dataset. **Operating** them — which to run, in what order,
indexer stopped or not — is [`docs/backfills.md`](../../docs/backfills.md). This
section is the authoring rule: when a new one is allowed to exist, and when it
must be deleted. Established by lore task 0425.

A one-off pass fixes history that the live indexer already handles going forward.
Four clauses, in order:

1. **Never re-implement the ingest path — that is the only forbidden shape.** A
   subcommand is the *right* home for a one-off: `--dry-run`, counters, tests, and
   a diff a reviewer can read all beat SQL pasted into a prod client at 2am. What
   is forbidden is narrower: a binary that re-walks S3, re-parses ledgers, and
   cherry-picks which rows to keep. That is a third copy of the ingest path, on top
   of the parser and the live writer. The removed `metadata-backfill` (0304) and
   `pool-ids-backfill` (0266) were exactly that — each parsed **every ledger in the
   range in full**, discarded all but one table's rows, and carried its own
   partition loop, watermark file and resume logic to do it.

   So: data already in ClickHouse → a subcommand driving SQL is fine and good.
   Data only in XDR → `run --reindex`, which re-parses the range through the one
   shared path. Never your own re-parser.

   > **The usual objection, and why it does not hold.** `docs/backfills.md` rule 4
   > warns that re-parsing with a *different parser build* is unsafe on the 15 RMT
   > tables that carry no version column, because ClickHouse could keep the stale
   > row. Measured on CH 26.3 (lore 0425): it keeps the **last row inserted**, so a
   > re-parse — which by construction lands after the data it replaces — wins in
   > every shape tried, including after `OPTIMIZE`, across parallel inserts, and
   > when read through `FINAL`. Every version-less table is also either keyed by
   > ledger (a re-parse only competes with its own earlier parse) or holds a pure
   > function of an immutable input. The real hazard is narrower and lives in the
   > parser, not the engine: **two rows for one key inside a single insert**, where
   > "last" is whatever order the code emitted (lore 0356, pool reserves). Emit one
   > row per key and `--reindex` is safe.

2. **Reuse the live code path — never reimplement it.** Call the same function
   the indexer calls. The removed passes that did (`nft-reparse` → the parser's
   own `detect_nft_events`; `assets-id-backfill` → `ids::asset_id`, the same fn
   as `AssetRow::staged`; `metadata-backfill` → the same `PartitionWriter`) never
   drifted — that discipline is what made them safe to delete. The two that reimplemented their logic in SQL — `repair-tier1` and
   `contract-type-rebuild` — are the ones the 0388 → 0392 → 0394 → 0404 bug
   family circles. A hand-written column list or an inlined verdict rule is a
   second copy of something the codebase already owns, and second copies drift
   silently.

3. **If it cannot be expressed as "replay live logic over old data", live has a
   hole.** That is a finding, not an inconvenience: open a task on the write path
   first, and the one-off becomes catch-up rather than recurring maintenance.
   This clause is a detector — it is what sorted the table below.

4. **Delete it once it has run.** Git keeps it. A spent one-shot left in `--help`
   reads as an available tool. Move it to `.trash/` (`rm` is forbidden
   repo-wide) and record the removal in the task.

### What survives, and why

| Command | Why it stays |
|---|---|
| `run`, `status` | the backfill itself |
| `bootstrap` | ⚠ **recurring mop — reclassified 2026-07-21 by measurement.** It reads as a one-off ("top up accounts the ingest window never observed"), but live re-creates the gap: the account writer stamps `sequence_number = 0` whenever it has no account-state override (`persist/stage.rs:699`), and the RMT version (`last_seen_ledger`) is bumped by that same write, so the zeroed row wins. Measured: **61.7% of accounts that sent a transaction** in a recent window carry `sequence_number = 0`, and skeletons are twice as common among *active* accounts (14.7%) as among dormant ones (6.75%). The live indexer has no bootstrap of any kind. Retire via lore 0421 |
| `balance-seed` | RPC snapshot of holders who have not transacted since the parser shipped. Live writes a balance only when it **observes** a `ContractData Balance(Address)` change, so this is not a hole in live logic — it is a state read live cannot express. **Not yet measured the way `bootstrap` was**; treat the classification as provisional |
| `repair-tier1` | ⚠ **recurring mop.** `ReplacingMergeTree` cannot express MIN, so the 6 Tier-1 columns re-drift under live ingest. Retire via lore 0232 / 0421 (`AggregatingMergeTree` + `SimpleAggregateFunction(min)`) |
| `nft-reclassify` | ⚠ **recurring mop.** No continuous `pending → hot` promotion in live. Retire via lore 0392 |
| `contract-type-rebuild` | ⚠ **partly covered.** Live has the G1 / G9 cross-ledger verdicts (`persist/stage.rs`); contracts the classifier cannot name still default to `Other`. Lore 0309 |

The four marked ⚠ each fail clause 3 — which is exactly why they are still here.

**The invariant that decides whether a defaulted write corrupts.** A whole-row
write that fills missing fields with defaults is safe **only if it also carries
the lowest version**. `soroban_contracts`' stub writer (`stage.rs:1761`) emits an
all-NULL row but stamps `wasm_uploaded_at_ledger = 0` — the lowest possible
version — so it always loses to the real deploy row. `accounts` breaks the rule:
its version is `last_seen_ledger`, which the defaulting write *bumps to the
current ledger*, so the emptied row wins. Tables with **no** version column
(`assets`, `wasm_interface_metadata`) are exposed by default, because there the
last write always wins. Check any new state-table writer against this.

### Removed (lore 0425)

Spent one-shots whose logic the live indexer now performs itself, each verified
against the live write path before removal:

| Removed | Live equivalent |
|---|---|
| `wasm-upgrade-backfill` (0320) | `build_wasm_upgrade_rows`, off the `executable_update` event |
| `upgradeable-backfill` (0327) | parser writes `metadata.upgradeable` on every new WASM |
| `nft-reparse` (0296) | fixed `detect_nft_events` in the parser |
| `soroban-token-flow-backfill` (0383) | `stage.rs` registers token-event participants + SAC asset presence |
| `pool-ids-backfill` (0266) | `pool_ids` + `gross_volume_a` computed live |
| `assets-id-backfill` (0331) | `AssetRow::staged` computes `id` with the same Rust fn |
| `metadata-backfill` (0304) | parser writes `soroban_contract_metadata` since 0297 |

Live coverage was verified on prod before each removal, not assumed — e.g. at
deletion time `soroban_contract_metadata` carried a write from 4 ledgers behind
the chain tip, and `operations_appearances.pool_ids` from the tip itself. The
shell wrappers that drove `pool-ids-backfill` (`scripts/0266/`) went with it.

Recoverable from git history if a comparable pass is ever needed — but read
clauses 1–4 first, because the answer is usually `run --reindex` or a live fix.

### Checkpoint-snapshot subcommands (task 0463) — read clause 4 carefully

`snapshot-seed` alone — its dry-run IS the four-way comparison (research probes
`snapshot-tally` / `snapshot-dedup`, the `snapshot-export-sql` helper and the
hand-exported-TSV transport were removed in the 2026-08-20 review — compare
prints the distinct-entry report itself, and both compare and seed read our
side straight from ClickHouse like every other corrective command here). All
read-only except the seed's explicit `--execute` (note the inversion of this
crate's usual `--dry-run` default: writing here is opt-in, so a forgotten
flag fails safe). The freshest checkpoint is complete by construction — the
archive's `.well-known` manifest is stellar-core's atomic commit point,
written last — so `--execute` simply takes it; `--pinned-manifest` exists for
exact reproduction of an earlier run (the ADR 0056 LP merge re-derives the
seed's snapshot from `artifacts/manifest.json`).

**They pass clause 1** — nothing re-implements the ingest path. The source is
the SDF history archive's checkpoint bucket list, a full STATE snapshot of
pubnet. Our ingest is a stream of CHANGES since the ledger floor, so a
`run --reindex` cannot reach what these read: 78.85% of chain history predates
the floor, and an entry that never changed since then has no row here to
re-parse.

**Clause 4 ("delete it once it has run") applies to the SUBCOMMANDS, not to
`snapshot.rs`.** The decoder underneath them is a reusable capability with two
filed consumers — task 0499 re-derives this exact snapshot from the seed's
`manifest.json` for the liquidity-pool merge, and task 0503 re-runs the
comparison as a recurring correctness monitor. Task 0502 extracts it into its
own crate for that reason. Retiring the seed after it runs is correct; taking
the decoder with it is not.

`snapshot-seed` is the one-off. The comparison is deliberately NOT: post-seed
it becomes the check that answers "do we still index correctly?", because a
discrepancy whose entry changed inside our window can no longer be explained by
coverage.

## Writes

The runner writes to ClickHouse (ADR 0044); Postgres was retired (task
0244). Env / flags: `--clickhouse-url` / `CLICKHOUSE_URL`, plus
`CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` / `CLICKHOUSE_DATABASE`
(defaults from `db_clickhouse::Config::from_env`); mTLS via
`--ch-cert`/`--ch-key`/`--ch-ca`.

The write path drives a **partition-writer lifecycle** (task 0206):
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
# ClickHouse run.
CLICKHOUSE_URL=http://localhost:8123 \
    cargo run -p backfill-runner -- \
    run --start 62016000 --end 62016099
```

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

For tight iteration loops — when you want to re-run the same range
many times — pass `--keep-partitions`:

```bash
CLICKHOUSE_URL=http://localhost:8123 \
    cargo run -p backfill-runner -- \
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

Each ledger file is deleted as soon as it is indexed (or skipped as
already-in-DB), and partition N's folder is removed wholesale before
awaiting the N+1 prefetch. Disk is therefore bounded by the prefetched
N+1 plus N's shrinking remainder — under ~2 × partition_size and falling
through each partition, regardless of range width.

## Resume & idempotency

The DB `ledgers` table is the sole source of truth — no state file, no
manifest, no marker.

Two resume filters run against the `HashSet<u32>` of completed sequences
built at startup:

1. **Pre-sync partition skip** — partitions whose clamped range
   (`max(start, p.start)..=min(end, p.end)`) is fully present in the set
   are filtered out and neither synced nor indexed. Re-running a
   fully-done range does zero S3 work and zero `write_ledger` calls.
2. **Per-ledger skip (inside a partition)** — for partitions that survive
   the pre-sync filter, `write_ledger` is skipped for any sequence
   already in the set. Handles mid-partition crashes where the partition
   is only partially in DB.

`aws s3 sync` itself is idempotent — a call against a fully-synced dir is
a LIST with no GETs — so there is no Stage A marker or file-count check.
A partial dir from a crashed run gets filled in on the next sync.

## Recent partitions and AWS S3 archive lag

The public Stellar archive on
`s3://aws-public-blockchain/v1.1/stellar/ledgers/pubnet/` lags real-time
mainnet by a window of hours. Partitions covering the live tail of the
chain may be partially uploaded at sync time: `aws s3 sync` exits 0,
the local folder is incomplete, and indexing would panic on the first
missing file.

Task 0225 added a post-sync validation pass to `sync_partition`:

| Local count          | S3 count (via `aws s3 ls`) | Behaviour                                      |
|----------------------|----------------------------|------------------------------------------------|
| `== PARTITION_SIZE`  | (not probed)               | proceed to index (`SyncOutcome::Complete`)     |
| `< PARTITION_SIZE`   | `< PARTITION_SIZE`         | **skip + warn** (`SyncOutcome::S3Incomplete`)  |
| `< PARTITION_SIZE`   | `== PARTITION_SIZE`        | retry sync once; still partial → hard error    |

`SyncOutcome::S3Incomplete` is operator-visible via a `WARN` log line
carrying `partition_start`, `local_files`, `s3_files`, and the partition
count is reflected in the final run summary
(`partitions skipped (S3)`). The run continues to the next partition;
no panic.

Operator action when archive lag is in effect: rerun the same
`--start … --end …` window after the archive catches up (typically
within a few hours). `resume.rs` picks up exactly the skipped ledgers
on the next run.

If the runner crashes mid-partition for a non-sync reason (parse
error, OOM, etc.), follow
[`docs/runbooks/0225_backfill_crash_recovery.md`](../../docs/runbooks/0225_backfill_crash_recovery.md).

## Retry policy

- **`aws s3 sync`** — 3 attempts, 2s base delay, ×2 multiplier, 30s cap.
  Hardcoded constants in `sync.rs`; change them if the numbers drift.
- **Parse / persist errors** — not retried. Parse failures indicate a
  data-shape bug; schema / constraint violations are write-path bugs.
  Both surface immediately.
- **Local partial sync despite full S3** — one fresh `sync_partition`
  retry (above the internal `run_sync_with_retry`'s 3 attempts). Still
  partial → `BackfillError::PartitionSyncFailed`. See [Recent partitions
  and AWS S3 archive lag](#recent-partitions-and-aws-s3-archive-lag).
- **Missing file post-sync** — still panics via `assert!(path.exists(),
  …)` in `ingest.rs`, but in practice unreachable after task 0225 —
  the validation pass converts the previous panic into either
  `SyncOutcome::S3Incomplete` (graceful skip) or
  `BackfillError::PartitionSyncFailed` (hard error). The assert stays
  as a last-line invariant for genuine filesystem bugs.

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

Bounded **under** ~2 × partition_size by per-ledger release plus
mandatory cleanup-after-index: the partition being indexed shrinks file
by file while the N+1 prefetch fills, so the peak is one full partition
plus a remainder rather than two full ones (task 0488 — a backfill filled
the production box and cost ClickHouse its write space). A crash leaves
at most that much on disk; it is reclaimed on the next successful
iteration, and `aws s3 sync` patches up any partial folder.

On error, folder cleanup is **deliberately skipped**, but per-ledger
release means the surviving folder holds only the ledgers not yet indexed
— the failing ledger's own file included, since it is unlinked only after
a successful ingest. Retry re-syncs the already-consumed files instead of
re-downloading from zero. Pass `--keep-partitions` when a run needs the
whole folder intact for forensics or repeated re-parses. If neither is
needed after a failure, `rm -rf .temp/backfill-runner/` is the recovery.

## Throughput

Reference throughput per partition is **to be measured** on a `us-east-1`
instance against the production DB. Update this section after the first
dry-run.

## Nx targets

```bash
pnpm nx build rust     # cargo build --workspace
pnpm nx test rust      # cargo test --workspace
pnpm nx lint rust      # cargo clippy --workspace -- -D warnings
```
