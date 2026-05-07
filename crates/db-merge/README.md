# db-merge

Multi-laptop backfill snapshot merge tool. Implements the playbook from
[ADR 0040](../../lore/2-adrs/0040_multi-laptop-backfill-snapshot-merge-hazards.md)
and the implementation plan in task
[0186](../../lore/1-tasks/active/0186_FEATURE_db-merge-multi-laptop-snapshots.md).

## Manual flow

You generate snapshots yourself — one per laptop range — and ingest them
in chronological order. Typical 1000-ledger run, split 4×250:

1. On a laptop slot, run `backfill-runner` (or `backfill-bench`) for
   ledgers `[L₀..L₀+249]` against a local Postgres.
2. `pg_dump --format=custom -f snapshots/01_a.dump <url>`.
3. Reset the laptop DB, repeat for `[L₀+250..L₀+499]` → `02_b.dump`,
   `03_c.dump`, `04_d.dump`. Filename prefix `01_` / `02_` / … is your
   convention for chronological order — the tool itself doesn't read
   filenames.
4. Spin up the two runtime containers (`postgres-merge`,
   `postgres-snapshot-source`) and migrate the merge target.
5. `db-merge ingest snapshots/01_a.dump …` then `02_b.dump`, `03_c.dump`,
   `04_d.dump` — one invocation per file, oldest-first.
6. `db-merge finalize --target-url <merge>` once at the end.

## Helper scripts (`scripts/`)

Three bash wrappers automate the manual flow above end-to-end. Run in
order. All accept env-var overrides; defaults reproduce the reference
4×250-ledger run starting at mainnet ledger 62016000.

### 1. `scripts/gen-merge-snapshots.sh` — generate snapshots

Loops `ITERATIONS` times (default 4); each iteration:
`docker compose down -v --rmi local` → `up -d` → `npm run db:migrate` →
`backfill-bench --start LO --end HI` → `pg_dump --compress=zstd:19`
into `.temp/merge-snapshots/0N_ledgers-LO-HI.dump`. Each iteration
covers `COUNT` consecutive ledgers (default 250) starting at
`START + (N-1)*COUNT`. **Wipes 5432 every iteration** — runs in the
preparation phase, before any merge work.

Override: `START=… COUNT=… ITERATIONS=… OUT_DIR=… bash scripts/gen-merge-snapshots.sh`.

### 2. `scripts/run-merge-snapshots.sh` — merge them

Discovers `*.dump` files in `$SNAPSHOTS_DIR` (default
`.temp/merge-snapshots`), sorts lexically (so `0N_` prefix maps to
chronological order), brings up `postgres-merge` + `postgres-snapshot-source`,
migrates the merge target, creates the seven `*_default` partitions to
match the snapshot layout, then loops `db-merge ingest` per file
followed by one `db-merge finalize`. Pre-merge backups land in
`.temp/db-merge-backups/`.

Run with **`RESET=1`** for a fresh merge target — required when
`pgdata-merge` already has data, otherwise the chronological-only
preflight rejects the first ingest:

```bash
RESET=1 bash scripts/run-merge-snapshots.sh
```

### 3. `scripts/diff-merge-vs-truth.sh` — verify

Builds a single-laptop sequential ground-truth backfill on `postgres`
(5432) covering the same range, runs `db-merge finalize` on both sides
for parity (so `nfts.current_owner_*` is rebuilt the same way on
truth as on merge), then `db-merge diff --left <truth> --right <merge>`.
Output: 17-row table `TABLE | ROWS_ORIGINAL | ROWS_MERGED |
HASH_ORIGINAL | HASH_MERGED | MATCH`. Exit 0 = all 17 match → merge
logically identical to the sequential backfill. Convention: `--left`
is ORIGINAL (truth), `--right` is MERGED.

**Wipes 5432** to build the truth from scratch (~12 min for 1000
ledgers). Skip the rebuild and reuse whatever's already there:

```bash
SKIP_TRUTH=1 bash scripts/diff-merge-vs-truth.sh
```

Override range with `START=… END=…`.

## Snapshot creation (`pg_dump`)

`db-merge ingest` consumes `pg_dump --format=custom` files via
`pg_restore` into `postgres-snapshot-source`. To minimise snapshot size:

```bash
pg_dump \
  --format=custom \
  --compress=zstd:19 \
  --no-owner --no-privileges \
  --file snapshots/01_a.dump \
  "postgresql://postgres:postgres@localhost:5432/soroban_block_explorer"
```

If `pg_dump` isn't on your `$PATH`, run it inside the source container
(it ships with `postgres:16-alpine`):

```bash
docker compose exec -T <source-service> pg_dump \
  --format=custom --compress=zstd:19 --no-owner --no-privileges \
  -U postgres soroban_block_explorer \
  > snapshots/01_a.dump
```

Why these flags:

- `--format=custom` is required — `db-merge ingest` calls
  `pg_restore -F custom` against the file.
- `--compress=zstd:19` — Postgres 16's custom format embeds zstd; level 19
  is near-max, typically 15–30 % smaller than the default gzip-6 at
  comparable CPU. The matching `pg_restore` in `postgres:16-alpine`
  decompresses it natively. On a Postgres < 16 source, fall back to
  `--compress=9` (level-9 gzip).
- `--no-owner --no-privileges` — drops `ALTER … OWNER` / `GRANT` lines
  that are noise under the local `postgres` superuser; tiny saving and
  zero permission warnings on restore.

Deliberately **not** used:

- `--data-only` / `--schema-only` — the merger reads from
  `merge_source.<table>` via FDW, so `postgres-snapshot-source` needs
  both schema and data.
- `--exclude-table=_sqlx_migrations` (or any other table exclusion) —
  the preflight gate requires `_sqlx_migrations` to match the target
  row-for-row including checksum. Same goes for every other table
  referenced by the merge plan.
- `--jobs=N` — parallel dump only works with `--format=directory`,
  which produces a tree instead of a single file. If wall-clock time
  matters more than ease of shipping, use
  `--format=directory --jobs=8 --compress=zstd:19` and zip / tar the
  resulting directory; otherwise stick with custom + zstd:19.

## Usage

```
db-merge ingest <snapshot> --target-url <url> --snapshot-source-url <url>
db-merge finalize --target-url <url>
db-merge diff --left <url> --right <url>
```

Run `ingest` once per snapshot, **chronologically oldest-first**, against
the same target. The preflight will reject a snapshot whose ledger range
precedes-or-overlaps the target's existing range (override with
`--allow-overlap` only when intentionally replaying for idempotency
inspection). Run `finalize` once after the last `ingest`. `diff` is a
generic per-table normalized-hash comparator between any two DBs — useful
if you maintain your own ground-truth DB elsewhere and want to verify the
merge against it.

## Locked structural decisions (task 0186, Step 0)

These are decided. Do not relitigate without amending the task.

| Decision           | Choice                                                                                          |
| ------------------ | ----------------------------------------------------------------------------------------------- |
| Atomicity          | Per-table batching with `SAVEPOINT`s every 100k rows; pre-merge `pg_dump` is the rollback path  |
| Diff strategy      | Normalized natural-key projection per table → ordered → `md5_agg` → compare hashes              |
| Batching threshold | 100k rows per `INSERT … SELECT`, ledger-sequence-windowed for partitioned tables                |
| Rebuild timing     | Post-final-snapshot only — explicit `merge finalize` subcommand                                 |
| Snapshot ingestion | `pg_restore` into `postgres-snapshot-source` container; expose to target via `postgres_fdw`     |
| Language           | Rust (`crates/db-merge`); sqlx + clap; parity with `backfill-runner`                            |
| Pre-merge backup   | `pg_dump --format=custom` of target before every `merge ingest`; user removes after success    |

## Containers

Two Postgres containers in `docker-compose.yml`, both gated behind the
`db-merge` profile:

| Service                    | Port | Role                                                            |
| -------------------------- | ---- | --------------------------------------------------------------- |
| `postgres-merge`           | 5436 | Merge target — receives every snapshot chronologically          |
| `postgres-snapshot-source` | 5437 | Ephemeral; `pg_restore` target. Reset before every `ingest`.    |

The existing `postgres` (5432) is the live backfill target for normal
dev work — **unrelated to the merge flow**.

### Reset procedures

Truncating tables is not sufficient — leaves sequence state and partition
children behind. Always drop the volume.

**Clean merge target** (start of a fresh merge run):

```bash
docker compose stop postgres-merge
docker volume rm <prefix>_pgdata-merge
docker compose up -d postgres-merge
# then run migrations against postgres-merge
```

**Clean snapshot source** (between snapshots within one run): same pattern
on `postgres-snapshot-source`. `merge ingest` does this automatically as
its first step.

**Full teardown — DO NOT use `docker compose down -v`.** The compose
project mixes the live `postgres` (no profile) with the db-merge runtime
DBs (profile `db-merge`); `down -v` removes every project volume
including the live one regardless of which profile triggered the
command. Scope the teardown explicitly:

```bash
docker compose --profile db-merge stop postgres-merge postgres-snapshot-source
docker compose --profile db-merge rm -f  postgres-merge postgres-snapshot-source
docker volume rm \
  sorban-block-explorer_pgdata-merge \
  sorban-block-explorer_pgdata-snapshot-source
```

## Implementation status

Subcommands `ingest`, `finalize`, `diff` are implemented end-to-end —
see `src/steps/`, `src/finalize/`, `src/diff/` for per-table coverage.
