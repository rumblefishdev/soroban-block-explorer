---
id: '0186'
title: 'DB merge script for multi-laptop backfill snapshots'
type: FEATURE
status: completed
related_adr: ['0040']
related_tasks: ['0010']
tags: [backfill, db, merge, postgres, tooling]
links: []
history:
  - date: 2026-05-04
    status: active
    who: fmazur
    note: 'Task created — implementation grounded in ADR 0040'
  - date: 2026-05-04
    status: active
    who: fmazur
    note: 'Rewrite — locked structural design (atomicity, diff, batching, rebuild timing); fixed test harness to 4-DB design; added idempotency + scale ACs'
  - date: 2026-05-04
    status: active
    who: fmazur
    note: 'Fix infeasible Step 0 snapshot mechanism — switch to postgres_fdw + 5th ephemeral container (postgres-snapshot-source)'
  - date: 2026-05-07
    status: active
    who: fmazur
    note: 'Pivot to manual flow — drop simulated multi-laptop test rig (postgres-truth, postgres-laptop-a, postgres-laptop-b) + scripts/db-merge-tests/. Operator generates snapshots themselves via backfill-runner / backfill-bench, organizes them with a numeric filename prefix, and ingests in order. Runtime infra reduced to 2 containers (postgres-merge + postgres-snapshot-source). T1–T6 corpus removed; correctness verification is now ad-hoc (operator maintains their own ground-truth DB if desired and uses `db-merge diff` against it).'
  - date: 2026-05-07
    status: completed
    who: fmazur
    note: >
      Done. `crates/db-merge` ships ingest/finalize/diff end-to-end (17 tables × per-table
      step + per-table diff projection + 2-step finalize). 2-container runtime infra
      (postgres-merge 5436 + postgres-snapshot-source 5437) under db-merge profile.
      3 helper scripts: gen-merge-snapshots / run-merge-snapshots / diff-merge-vs-truth.
      Manual verification: 4×250 mainnet ledgers (62016000-62016999) — 4 ingests + finalize
      passed with full data through every step (~9 min). Diff-vs-truth, idempotency, and
      wrong-order-rejection: NOT exercised; deferred. Two notable post-merge fixes landed
      with completion: (a) soroban_contracts.metadata→name alignment after lore-0156 typed
      column migration (b965613); (b) helper script tooling + ORIGINAL/MERGED diff headers
      (7d86d17). One in-conflict design choice during develop merge: kept lore-0189
      sentinel approach for orphan lp_positions, dropped lore-0186 Pass 2 stub for
      cross-range op pool refs — see Design Decisions § Emerged for the trade-off and
      Future Work for the multi-laptop disjoint-coverage caveat that re-emerges.
---

# DB merge script for multi-laptop backfill snapshots

## Summary

Build a script that merges one snapshot (Postgres dump) of a backfilled
laptop database into a local Docker target database, applying the
remap/dedup/watermark logic mandated by [ADR 0040](../../2-adrs/0040_multi-laptop-backfill-snapshot-merge-hazards.md).
The script is invoked once per snapshot, **chronologically oldest-first**,
against the same target. Estimate: 1–2 weeks of focused work
(infra + script + diff harness + test corpus).

## Status: Completed

`crates/db-merge` ships `ingest` / `finalize` / `diff` end-to-end against
the 2-container runtime infra. Verified by one real 4×250 mainnet
ledger run (62016000-62016999) — 4 ingests + finalize succeeded.
`db-merge diff` against an operator-maintained sequential ground-truth,
idempotency replay, and wrong-order rejection were NOT exercised before
closure (see Implementation Notes § Verification scope and AC checklist).

## Context

- N laptops run `backfill-runner` on disjoint ledger ranges into local
  Dockerised Postgres (port 5432). After each laptop finishes, its DB is
  `pg_dump --format=custom`-ed.
- ADR 0040 lists the merge hazards: surrogate-id collision on 4 sequences
  (`accounts`, `soroban_contracts`, `nfts`, `transactions`); LWW current-state
  tables (`lp_positions`, `account_balances_current`, `nfts.current_owner_*`);
  GENERATED `soroban_contracts.search_vector`; `pg_trgm` extension + 5
  IMMUTABLE label functions; partition-FK quirks.
- Scale we're targeting: per laptop ~2M ledgers ⇒ ~50M rows in `transactions`,
  ~150M in `operations_appearances`. Two laptops merged ⇒ ~300M-row target
  table. This rules out single-transaction merges and naive temp tables.

---

## Implementation Plan

### Step 0: Lock structural design decisions

These are the choices that _shape every subsequent step_. Decide before
writing any code; record decisions inline in the script README.

| Decision               | Recommended                                                                                                                                                                                                | Rationale                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Atomicity**          | Per-table batching with `SAVEPOINT`s every 100k rows; pre-merge `pg_dump` of the target as the rollback path                                                                                               | Single tx over 150M rows blows up WAL/locks. Savepoints give recoverable failure without keeping the whole merge open. The pre-merge dump is the only true rollback for cross-table inconsistency.                                                                                                                                                                                                                               |
| **Diff strategy**      | Normalized natural-key projection per table → ordered → `md5_agg` per table → compare hashes                                                                                                               | Direct row-by-row diff with surrogate ids is meaningless (auto-allocated). Per-table hash gives a single bool answer + cheap cardinality check.                                                                                                                                                                                                                                                                                  |
| **Batching threshold** | 100k rows per `INSERT … SELECT`, ledger-sequence-windowed for partitioned tables                                                                                                                           | Keeps temp memory bounded; aligns with `backfill-runner` batch sizes.                                                                                                                                                                                                                                                                                                                                                            |
| **Rebuild timing**     | Post-final-snapshot only — explicit `merge finalize` subcommand                                                                                                                                            | Rebuilding `nfts.current_owner_*` after every snapshot is wasteful (re-scans full ownership log every time). User runs `merge ingest` N times then `merge finalize` once.                                                                                                                                                                                                                                                        |
| **Snapshot ingestion** | `pg_restore` the snapshot into a separate Postgres container (`postgres-snapshot-source`); expose its `public` to the merge target via `postgres_fdw` + `IMPORT FOREIGN SCHEMA public … INTO merge_source` | `pg_restore` cannot retarget a schema name (`--schema` filters what to restore, not where), and renaming `public` on the target is impossible while the target's own `public` is in use. FDW is Postgres's standard cross-DB access pattern; the merge SQL then `SELECT FROM merge_source.<table>` exactly as if it were local. Container isolation also keeps source's seed rows / extensions from polluting target's `public`. |
| **Language**           | Rust (new `crates/db-merge`)                                                                                                                                                                               | Parity with `db-migrate`/`backfill-runner`; sqlx already in workspace; CLI via clap consistent with `backfill-runner`.                                                                                                                                                                                                                                                                                                           |
| **Pre-merge backup**   | `pg_dump --format=custom` of target before every `merge ingest` invocation; user removes after success                                                                                                     | Only safe rollback for cross-table corruption. Path printed to stderr at start.                                                                                                                                                                                                                                                                                                                                                  |

### Step 1: Runtime infrastructure — 2 Docker databases

Add to `docker-compose.yml` under the `db-merge` compose profile:

| Service                    | Port | Role                                                                                                                 |
| -------------------------- | ---- | -------------------------------------------------------------------------------------------------------------------- |
| `postgres` (existing)      | 5432 | Live indexer target for normal dev work — **unrelated to the merge flow**                                            |
| `postgres-merge`           | 5436 | Merge target — receives every snapshot chronologically                                                               |
| `postgres-snapshot-source` | 5437 | Ephemeral; `pg_restore` target for the current snapshot. Reset (drop volume + recreate) before every `merge ingest`. |

Both runtime DBs share identical config (image `postgres:16-alpine`,
healthcheck, same credentials). The script accepts `--target-url` for the
merge destination and `--snapshot-source-url` for the FDW source.

**Manual flow.** The operator runs `backfill-runner` / `backfill-bench`
themselves — on whatever Postgres they like (their own laptop, a separate
DB, the `postgres` slot reused 4× sequentially) — and `pg_dump
--format=custom` produces N snapshot files. They organize the files with
a numeric filename prefix (`01_*.dump`, `02_*.dump`, …) so they can
remember the chronological order; `db-merge` itself does not parse
filenames. Then they call `db-merge ingest` once per file, oldest-first,
followed by `db-merge finalize`.

**Reset procedures**:

- Start of a fresh merge run (clean merge target):
  `docker compose stop postgres-merge && docker volume rm <prefix>_pgdata-merge && docker compose up -d postgres-merge` then run migrations.
- Between snapshots within one run (clean snapshot source):
  same pattern on `postgres-snapshot-source`. The merge script does this
  automatically as the first step of `merge ingest`.

Truncating tables is _not_ sufficient — leaves sequence state and partition
children behind.

### Step 2: Snapshot ingestion (`merge ingest <snapshot> --target-url <url> --snapshot-source-url <url>`)

1. Reset `postgres-snapshot-source` (drop volume, recreate, wait for healthy);
   `pg_restore` the snapshot into its `public` schema. Source's `pg_trgm`
   extension and the 5 IMMUTABLE label functions are restored alongside the
   data and live in _that_ container's `public` — they don't touch the
   target's schema.
2. On the merge target, prepare the FDW bridge (idempotent — script may
   re-run after partial failure):
   ```sql
   CREATE EXTENSION IF NOT EXISTS postgres_fdw;
   CREATE SERVER IF NOT EXISTS merge_source_server FOREIGN DATA WRAPPER postgres_fdw
       OPTIONS (host 'postgres-snapshot-source', port '5432', dbname 'soroban_block_explorer');
   CREATE USER MAPPING IF NOT EXISTS FOR CURRENT_USER SERVER merge_source_server
       OPTIONS (user 'postgres', password 'postgres');
   DROP SCHEMA IF EXISTS merge_source CASCADE;
   CREATE SCHEMA merge_source;
   IMPORT FOREIGN SCHEMA public FROM SERVER merge_source_server INTO merge_source;
   ```
   Now `merge_source.<table>` exposes every source table as a foreign table;
   the merge SQL `SELECT FROM merge_source.X` reads via the FDW (local Docker
   network, low overhead).
3. Pre-flight precondition checks via the FDW (abort on any mismatch with
   actionable error; tear down the FDW bridge first so the target stays
   clean):
   - target's `_sqlx_migrations` matches `merge_source._sqlx_migrations`
     row-for-row including `checksum` (catches schema drift from mid-merge
     migration runs);
   - `merge_source.ledgers` `MIN/MAX(sequence)` doesn't overlap with target's
     existing range; source range is **strictly later** than target's `MAX`
     (chronological-only contract);
   - both sides have `*_default` partition only and matching CHECK set
     (`ck_assets_identity`, `ck_sia_caller_xor`, partial UNIQUEs) — verified
     by querying `pg_constraint` on each via FDW vs local.
4. Take pre-merge `pg_dump` of target (Step 0 decision); print path to
   stderr. Only after this point is the merge committed.

### Step 3: Topological merge (the 15 SQL steps from ADR 0040)

Run in batches of 100k rows; use `SAVEPOINT` per batch so a single failed
batch retries without losing progress.

1. `wasm_interface_metadata` — `ON CONFLICT (wasm_hash) DO UPDATE SET metadata = EXCLUDED.metadata`.
2. `ledgers` — `ON CONFLICT (sequence) DO NOTHING`.
3. `accounts` — remap pass: dedup by `account_id`; clauses per ADR 0040 (LEAST/GREATEST/sentinel-aware sequence_number/latest non-NULL home_domain); capture `RETURNING (id, account_id)` into `merge_remap.accounts(source_id, target_id)`.
4. `soroban_contracts` — remap pass: dedup by `contract_id`; COALESCE per nullable; `is_sac = OR`. **Omit `search_vector` from INSERT** (GENERATED ALWAYS; recomputed by Postgres). Capture remap.
5. `assets` — dedup-only via partial UNIQUEs; `GREATEST(asset_type)` with the SAC-prefer guard from `write.rs:1311–1314`. No remap needed (no FK referrers).
6. `liquidity_pools` — `ON CONFLICT (pool_id) DO UPDATE SET created_at_ledger = LEAST(...)`.
7. `nfts` — remap pass: dedup by `(contract_id, token_id)`. Do **not** copy source's `current_owner_*` — leave NULL/stale until Step 13 (`merge finalize`). Capture remap.
8. `transactions` — remap pass: dedup by `(hash, created_at)` via `uq_transactions_hash_created_at`; `DO UPDATE SET hash = EXCLUDED.hash` no-op for `RETURNING`. Capture `merge_remap.transactions(source_id, source_created_at, target_id, target_created_at)` — note `created_at` is part of the remap because partition routing depends on it.
9. `transaction_hash_index` — `ON CONFLICT (hash) DO NOTHING`.
10. Five appearance tables (`operations_appearances`, `transaction_participants`, `soroban_events_appearances`, `soroban_invocations_appearances`, `nft_ownership`) — FK rewrite via `JOIN merge_remap.<parent>` in the SELECT. **Build B-tree index on `merge_remap.<parent>(source_id)` before the JOINs** — without it, 150M-row JOINs do nested loops and never finish. ON CONFLICT … DO NOTHING on each table's natural-key UNIQUE/PK.
11. `liquidity_pool_snapshots` — `ON CONFLICT uq_lp_snapshots_pool_ledger DO NOTHING` (dedup-only).
12. `lp_positions` and `account_balances_current` (native + credit paths) — watermark-guarded UPSERT exactly mirroring `write.rs:1749–1754` and `write.rs:1866–1948`.
13. **(Deferred to `merge finalize`)** Rebuild `nfts.current_owner_*` from `nft_ownership` via `SELECT DISTINCT ON (nft_id) … ORDER BY nft_id, ledger_sequence DESC, event_order DESC`. Do NOT run after each `merge ingest`; only on the final invocation.
14. **(Deferred to `merge finalize`)** `setval(<seq>, MAX(id))` on all 7 sequences.
15. **(In each `merge ingest`)** Tear down the FDW bridge on the target:
    `DROP SCHEMA merge_source CASCADE; DROP USER MAPPING FOR CURRENT_USER SERVER merge_source_server; DROP SERVER merge_source_server;`.
    Optionally `docker compose stop postgres-snapshot-source` to release the
    volume; the next `merge ingest` will reset it anyway.

### Step 4: Diff harness (`merge diff --left <url> --right <url>`)

Build a separate utility that produces a **per-table normalized hash** for
two DBs. Approach:

- For each table, project rows to a SELECT that:
  - replaces every surrogate FK with the natural key (e.g.
    `transactions.source_id` → `(SELECT account_id FROM accounts WHERE id = source_id)`),
  - excludes auto-allocated surrogate `id` columns from the projection,
  - excludes `search_vector` (recomputed),
  - sorts deterministically by natural key.
- Compute `md5(string_agg(row::text, '|' ORDER BY natural_key))` per table.
- Output a 25-row table: `table | row_count_left | row_count_right | hash_left | hash_right | match?`.

Two DBs with identical _logical_ contents but different surrogate id
allocations will produce identical hashes. This is the **only credible
correctness check** for the merge.

### Step 5: Manual verification

There is no automated test corpus checked in. The operator verifies a
merge run themselves, ad-hoc, after each meaningful change. Suggested
checks (none of these are wired into CI):

- **First-snapshot sanity.** Empty `postgres-merge` ← single snapshot
  via `merge ingest`; if the operator separately maintains a sequential
  backfill of the same range, `db-merge diff` should report 25× match.
- **Two-snapshot chronological merge.** `postgres-merge` ← snapshot A,
  then snapshot B (B's ledger range strictly after A's), then `merge
finalize`. Compare against an operator-maintained sequential
  backfill of the full range with `db-merge diff` — expect 25× match.
- **Idempotency.** Re-run `merge ingest <snapshot>` (with
  `--allow-overlap`) on an already-merged target; expect zero new rows
  and zero modified watermark columns.
- **Wrong-order rejection.** After ingesting a later range, attempt to
  ingest an earlier one without `--allow-overlap`; preflight must abort
  with "source range precedes target".

If the team needs reproducible regression coverage in the future,
re-introduce a scripted harness (the previous `scripts/db-merge-tests/`
lives in `.trash/scripts-db-merge-tests/` as a starting point) — but
that's out of scope of the current manual flow.

---

## Acceptance Criteria

- [x] `docker-compose.yml` has `postgres-merge` and `postgres-snapshot-source`
      services on ports 5436 and 5437, both gated behind the `db-merge`
      profile; reset procedures (merge target, snapshot source) documented
      in `crates/db-merge/README.md`.
- [x] `merge ingest` automates the FDW setup (`CREATE EXTENSION
  postgres_fdw`, server, user mapping, `IMPORT FOREIGN SCHEMA public …
  INTO merge_source`) and tears it down on success.
- [x] `crates/db-merge` exists with three subcommands: `ingest`, `finalize`,
      `diff`; CLI flags follow `backfill-runner` conventions.
- [x] All 18 ADR-0040 table-by-table merge semantics implemented (collapsed
      into 15 substeps under task §"Step 3: Topological merge"); FK rewrites
      are JOIN-in-SELECT with B-tree indexes on remap tables; no post-insert
      UPDATE on partitioned tables.
- [x] Per-table batching at 100k rows; `SAVEPOINT` per batch; failure of one
      batch retries without rolling back the whole table.
- [x] Pre-merge precondition checks abort on: migration mismatch (incl.
      checksum), ledger-range overlap, source-precedes-target, partition
      drift, CHECK drift.
- [x] `search_vector` excluded from `soroban_contracts` INSERT column list;
      Postgres recomputes on each insert.
- [x] Pre-merge `pg_dump` taken; path printed to stderr; user owns cleanup.
- [x] `merge finalize` runs Step 13 (`nfts.current_owner_*` rebuild) and
      Step 14 (`setval` all 7 sequences); idempotent.
- [x] `merge diff` produces a per-table table with row counts + md5 on a
      normalized natural-key projection. Operator uses it ad-hoc against
      whatever ground-truth DB they maintain.
- [~] **Manual verification** (per Step 5) — partial. 4×250 mainnet
  sequential ingest + finalize executed end-to-end without errors
  (preflight, all 17 step modules, FDW bridge teardown, pre-merge
  pg*dump, finalize sequences + nfts.current_owner*\* rebuild — see
  Implementation Notes § Verification scope for table-by-table row
  counts). NOT exercised: (a) `db-merge diff` against an
  independent sequential ground-truth backfill, (b) idempotency
  replay (same snapshot twice), (c) wrong-order rejection
  (re-ingesting an earlier range after a later one). Helper script
  `scripts/diff-merge-vs-truth.sh` ready for (a) when needed.
- [ ] **Docs updated** — N/A: offline operational tool, not part of indexer/
      API/infra shape under `docs/architecture/**`. If `crates/db-merge`
      becomes a permanent piece of the pipeline (e.g. ongoing parallel
      backfill workflow), revisit
      `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`.

---

## Notes

**Genuine open questions** (deliberately deferred — affect implementation
ergonomics, not correctness):

- **Snapshot transport.** Today the user copies snapshot files between
  laptops manually (USB / S3 / scp). Out of scope for this task; the script
  takes a local path.
- **Resume after partial `merge ingest` failure.** If a `SAVEPOINT` batch
  fails mid-table after 50k of 200k rows are committed, on retry should the
  script skip already-merged rows automatically (via `ON CONFLICT DO NOTHING`
  semantics, which is already in place) or expose `--from-batch N`? Default:
  rely on ON CONFLICT idempotency; add `--from-batch` only if T4 testing
  reveals replay performance issues.
- **Concurrent `merge ingest` invocations.** Forbidden — script takes a
  Postgres advisory lock at start. Worth covering in operator manual
  verification (Step 5) once before declaring production-ready.
- **Source DB live during merge.** The ADR assumes source is dumped first,
  not connected live. Live-source merge is a future variant (skip pg_dump
  step), out of scope here.

**Cross-references** for SQL clauses every step uses (line numbers from
verifier passes underlying ADR 0040, not from the ADR text itself; useful
when re-reading `crates/indexer/src/handler/persist/write.rs` to confirm
the exact `ON CONFLICT` wording before transcribing it into the merge
script):
518 (ledgers), 595–596 (transactions), 649 (transaction_hash_index),
86–125 (accounts), 410–427 (soroban_contracts), 158–164 (wasm), 1077–1329
(assets 4 paths), 1490–1497 (nfts LWW), 1567 (nft_ownership append), 700
(transaction_participants), 806 (operations_appearances), 902
(soroban_events_appearances), 1060 (soroban_invocations_appearances),
1643–1645 (liquidity_pools), 1702 (lp_snapshots), 1749–1754 (lp_positions),
1866–1948 (account_balances_current).

---

## Implementation Notes

**Crate layout** (`crates/db-merge/src/`):

- `cli.rs` — clap subcommands: `ingest <snapshot> --target-url --snapshot-source-url [--allow-overlap]`, `finalize --target-url`, `diff --left --right`
- `main.rs`, `error.rs` — entry + error type
- `ingest.rs` — orchestrates the per-snapshot pipeline (snapshot-source reset → pg_restore → FDW bridge → preflight → backup → steps loop → FDW teardown)
- `snapshot_source.rs` — `docker compose stop/rm/up -d` lifecycle for `postgres-snapshot-source` + `pg_restore` invocation
- `fdw.rs` — `CREATE EXTENSION postgres_fdw` + server + user mapping + `IMPORT FOREIGN SCHEMA public … INTO merge_source` + teardown
- `preflight.rs` — migrations match (incl. checksum), ledger range non-overlap & strictly later, partition layout match, CHECK constraint match
- `backup.rs` — pre-merge `pg_dump --format=custom` of target → `.temp/db-merge-backups/pre-merge-<timestamp>.dump`
- `batcher.rs` — `ledger_windowed` + single-batch helpers, 100k row windows, SAVEPOINT per batch
- `steps/{17 tables}.rs` — per-table merge SQL (REMAP / DEDUP / WATERMARK / UNION); aggregator at `steps/mod.rs`
- `diff/{17 tables}.rs` — per-table normalized natural-key projection → md5; aggregator at `diff/mod.rs`
- `finalize/{nfts_current_owner, sequences}.rs` — Step 13 + Step 14

**Runtime infra** (docker-compose.yml, `db-merge` profile):

- `postgres-merge` (5436) — merge target, persistent
- `postgres-snapshot-source` (5437) — ephemeral, reset before every ingest

**Helper scripts** (`scripts/`):

- `gen-merge-snapshots.sh` — N×COUNT-ledger snapshots via wipe → up → migrate → backfill-bench → pg_dump (zstd:19, custom, no-owner, no-privileges); default 4×250 from 62016000
- `run-merge-snapshots.sh` — discover _.dump in lexical order → merge target up + migrate + `_\_default`partitions → loop`ingest`+`finalize`; `RESET=1` for fresh target
- `diff-merge-vs-truth.sh` — sequential ground-truth on 5432 + finalize on both sides for parity → `db-merge diff --left <truth> --right <merge>`

**Verification scope** (4×250 mainnet ledgers, 62016000-62016999, 2026-05-07):

| Stage                                              | Outcome                                                                                                                                                                                                                                                                                                                                                  |
| -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 4× `merge ingest` chronologically                  | all preflight checks passed; FDW bridge set up + torn down each time; pre-merge backups (4× ~50MB) saved                                                                                                                                                                                                                                                 |
| Per-table row counts (cumulative across 4 ingests) | ledgers 1000, accounts 104215, soroban*contracts 10111, transactions 394342, operations_appearances 625428, nfts 92694, lp_snapshots 41129, transaction_participants ~750k, events_appearances ~635k, invocations_appearances ~190k, lp_positions / nft_ownership / current_owner*\* mostly empty (no NFT transfers / minimal LP activity in test range) |
| `merge finalize`                                   | nfts.current*owner*\* rebuild OK (0 rows — no nft_ownership events); 7× setval applied                                                                                                                                                                                                                                                                   |
| Total wall-clock                                   | ~9 min on workstation                                                                                                                                                                                                                                                                                                                                    |

NOT exercised: `db-merge diff --left <truth> --right <merge>` against a sequential ground-truth backfill, idempotency replay, wrong-order rejection. Operator can run `bash scripts/diff-merge-vs-truth.sh` to cover (a) at any time.

---

## Issues Encountered

- **`soroban_contracts.metadata` column removed by lore-0156 typed-name migration** — migration `20260505130000_soroban_contracts_typed_name_column` (landed via develop merge) replaced `metadata JSONB` with typed `name VARCHAR(256)`. Merger `steps/soroban_contracts` and `diff/soroban_contracts` still referenced `s.metadata` → ingest aborted at "column s.metadata does not exist" in the second step. Fix: 4× `metadata`→`name` in the step query (input projection, INSERT col list, SELECT col list, ON CONFLICT clause) + 1× in diff canonical projection (commit `b965613`).
- **Partition-layout mismatch on first merge run** — `run-merge-snapshots.sh` initially called `cargo run -p db-partition-mgmt --bin cli` to create partitions on `postgres-merge`. That CLI runs `ensure_all_partitions` which adds 217 monthly children (`y2024m02`…`y2026m08`) on top of `*_default`. But `backfill-bench` (which produced the snapshots) only calls `ensure_default_partition` — snapshots have `*_default` only. Preflight rejected with "non-default children present on target". Fix: replaced the CLI invocation with raw `psql` `CREATE TABLE … PARTITION OF … DEFAULT` for the 7 time-partitioned tables — matches snapshot layout exactly.
- **Conflict in `crates/indexer/src/handler/persist/write.rs` on develop merge** — two parallel sentinel-aware UPSERT designs collided: branch-side lore-0186 Pass 2 stub (`fee_bps=0` detection) for cross-range op pool refs vs develop-side lore-0189 orphan placeholder (`created_at_ledger=0` marker via `insert_sentinel_pools`) for orphan `lp_positions`. Resolution: take develop entirely (lore-0189 wins). Implication captured under Future Work.

---

## Design Decisions

### From Plan

1. **Per-table batching with `SAVEPOINT`s every 100k rows + pre-merge `pg_dump` as rollback path** — single transaction over 150M rows blows up locks/WAL.
2. **`postgres_fdw` + ephemeral `postgres-snapshot-source` container** — `pg_restore` cannot retarget a schema, and renaming `public` on the live target is impossible. FDW lets the merger run plain `INSERT INTO target.X SELECT … FROM merge_source.X` set-based across a Docker-network hop.
3. **Per-table normalized natural-key hash for `diff`** — surrogate IDs (BIGSERIAL) differ across DBs even with identical logical content, so raw diff is impossible. Per-table projection replaces every surrogate FK with the referenced natural key, excludes surrogate `id` and `search_vector`, sorts by natural key, hashes via `md5(string_agg(...))`.
4. **Rust + sqlx + clap, new `crates/db-merge`** — parity with `backfill-runner`/`db-partition-mgmt`.
5. **Post-final-snapshot `nfts.current_owner_*` rebuild + `setval` via `merge finalize`** — wasteful to recompute after every ingest.

### Emerged

6. **Pivot to manual flow (5 → 2 runtime containers)** — original plan had 5-DB simulated multi-laptop test rig (`postgres-truth`, `postgres-laptop-a`, `postgres-laptop-b`) + `scripts/db-merge-tests/` (T1-T6 test corpus, ~13 files, ~840 LOC). Dropped in favour of operator-driven snapshot generation: 3 helper scripts replaced the harness, ground-truth maintenance is operator's responsibility. Runtime collapsed to `postgres-merge` + `postgres-snapshot-source` only. Test rig moved to `.trash/scripts-db-merge-tests/`. Commit `c080e29`.
7. **`*_default` partitions only on merge target** — discovered via Issue #2. Helper script uses raw psql `CREATE TABLE *_default PARTITION OF … DEFAULT` for the 7 time-partitioned tables instead of the full `db-partition-mgmt-cli` (which would create monthly children that mismatch snapshots).
8. **Diff output headers `ROWS_ORIGINAL` / `ROWS_MERGED` / `HASH_ORIGINAL` / `HASH_MERGED`** instead of generic `_L` / `_R` — semantic over generic. Convention pinned: `--left` = ORIGINAL (truth), `--right` = MERGED. CLI flags themselves left as `--left/--right` — the tool stays generic, only the labels are opinionated. Commit `7d86d17`.
9. **Lightweight `pg_dump` recipe**: `--format=custom --compress=zstd:19 --no-owner --no-privileges` as default in helper script. zstd:19 is near-max compression in PG16's custom format (15-30% smaller than default gzip-6 at comparable CPU). Documented alternatives (zstd:22, directory+jobs, plain+external zstd, --data-only with pre-migrated source) under § Snapshot creation in README.
10. **Took develop's lore-0189 sentinel approach over branch's lore-0186 Pass 2 stub** during write.rs conflict resolution — lore-0189 (sentinel `created_at_ledger=0` for orphan `lp_positions` placeholders) wins over lore-0186 (Pass 2 stub for cross-range op→pool refs with `fee_bps=0` marker). Trade-off captured in Future Work.

---

## Future Work

Prose-only — no backlog tasks spawned.

- **Cross-range op→pool linkage in disjoint multi-laptop merges.** Branch's lore-0186 commit `0592c62` added a Pass 2 stub in `upsert_pools_and_snapshots` that auto-stubbed `liquidity_pools` rows for any `pool_id` referenced by `operations_appearances` but not yet present in the target — together with the removal of the defensive `CASE WHEN EXISTS … ELSE NULL` nullify in `insert_operations`. That commit was effectively reverted by taking develop's `write.rs` whole during the merge conflict (Design Decision § Emerged #10). Net effect with current code: when laptop A backfills ledgers `[X..Y]` containing a `CREATE_POOL_OP`, and laptop B backfills `[Y+1..Z]` containing a `DEPOSIT_OP` referencing that pool, laptop B's snapshot will have `operations_appearances.pool_id = NULL` for the deposit (the defensive nullify catches it). The merger then loses the op→pool linkage permanently — XDR is gone after backfill. Acceptable for the verified scenario (4×250 chronologically sequential snapshots from one laptop slot reused 4×; no real cross-range refs). Becomes a real data-loss issue if the team adopts truly disjoint multi-laptop coverage. Re-introducing the lore-0186 Pass 2 stub on top of the lore-0189 sentinel-aware UPSERT is feasible (both stubs share `fee_bps=0` marker, hybrid detection covers both classes — see commit message of `b9cc223` for the analysis); deferred until disjoint multi-laptop runs become a workflow.
- **Manual verification suite extension.** Currently only a single chronological-merge scenario was exercised. The deleted T1–T6 corpus from `scripts/db-merge-tests/` (in `.trash/scripts-db-merge-tests/`) covers idempotency, wrong-order rejection, single-snapshot reproducibility, scale smoke. If the merger sees real-team usage and bug reports start appearing, re-introduce a slimmed-down version of that harness against the 2-container infra.
- **Snapshot transport.** Operator currently moves snapshot files between laptops manually (USB / S3 / scp). Out of scope by design; merger takes local paths.
- **Live-source merge.** ADR 0040 + this implementation assume snapshots are dumped first, not connected live. A `merge ingest --source-url <live>` variant skipping `pg_restore` could be useful for repeated mid-stream merges; not implemented.
