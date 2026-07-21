# Backfill Guide

Single entry point for backfilling ClickHouse. **Read
[§ Correctness rules](#correctness-rules--read-before-you-run-anything) before
running anything** — most of the ways a backfill goes wrong are silent.

Deep-dive runbooks are linked, not duplicated.

---

## Which situation are you in?

| Situation                                                                 | What to run                                                                                                        |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| **Gap** — ledgers missing (after a restore, an outage, a stalled indexer) | `backfill-runner run --start <gap> --end <tip>` — `run` is a **gap-filler**, it skips ledgers already in `ledgers` |
| **New derived table** over history — the data exists only in XDR          | from-S3 re-parse: `run --reindex` (see [§ On the box](#path-a--directly-on-the-hetzner-box-current-default))       |
| **New derived table** computable from columns already in CH               | cheap in-DB `INSERT … SELECT` (no re-parse)                                                                        |
| **Bad data in place** (range already in `ledgers`)                        | `run --reindex` — a plain `run` would **no-op**                                                                    |
| Tier-1 columns wrong after any of the above                               | `repair-tier1` (**mandatory** — see below)                                                                         |

---

## Correctness rules — read before you run anything

### 1. Writes are idempotent by construction

Backfill and the live indexer share one parse path
(`backfill-runner/src/ingest.rs` → `indexer::handler::process::process_ledger`).
Three properties make replay safe:

- **Absolute state, never delta.** Rows are post-images read straight from the
  XDR `TransactionMeta` — no read-modify-write, no carried in-memory balance. A
  removed trustline emits an explicit `balance: 0` row, not a decrement.
  **There is no double-apply.**
- **Deterministic surrogate ids** (`db-clickhouse/src/persist/ids.rs`) — the file
  names "Parallel-writer safety" as an explicit design goal: K runners compute
  the same `id` for the same StrKey with no shared counter, no coordination.
- **Commit marker written last.** The `ledgers` row is opened strictly after the
  18 entity tables. A marker-less ledger is re-processed in full.

`insert_deduplicate = 0` — ReplacingMergeTree (RMT) is the dedup layer, not the
insert path.

**Engine inventory:** 25 of 28 tables are RMT. The 3 that are not are all
duplicate-proof, so they are **not** a hazard: `accounts_recent` and
`balance_aggregates` are refreshable-MV targets (full recompute + atomic
`EXCHANGE`, nothing writes to them directly), and `asset_sac` is
AggregatingMergeTree over `max` (`max(x,x) = x`). There is no SummingMergeTree
and no append-only counter anywhere.

### 2. Parallelism — K processes on disjoint ranges

There is **no `--workers` flag**. The runner is sequential per-ledger, one
partition at a time. Parallelism is external: run K processes with disjoint
`--start`/`--end`.

- **Safe:** concurrent inserts on the same table need no coordination — RMT
  dedups merge-side keyed by `ORDER BY`. Overlapping ranges are _tolerated_
  (re-running an overlap is a no-op) — **but only because identical code
  produces byte-identical rows**. See rule 4.
- **K ≥ 8:** raise CH `max_concurrent_queries` from 100 to ~200 (each writer
  holds ~14 long-lived inserts). Performance, not correctness.
- **Empirically (2026-07, 24 cores):** more than **6 workers was no faster**, and
  going past 6 only worsened disk pressure.

> **Disjointness is necessary but NOT sufficient.** See rule 3.

### 3. The MIN-semantics trap — 12 Tier-1 columns corrupt silently

RMT keeps the **highest-version whole row**, so it **cannot express MIN
semantics**. Worker N stamps `first_seen_ledger` with the first ledger of _its_
range, with no visibility into earlier workers' ranges — so the surviving value
reflects the latest-touching worker, not the true minimum. **Twelve Tier-1
columns corrupt this way, silently.**

Scale, measured: the 0228 repair moved `first_seen_ledger` for **10.13M
accounts**.

→ **Budget a `repair-tier1` pass after every parallel or `--reindex` backfill.**
`repair-tier1` itself requires the indexer stopped (see the table below).

Two nuances worth knowing:

- This is **not parallel-specific**. Per task 0232 the same columns also drift in
  **live ingest**, because the CH writer cannot afford the read-before-write the
  old PG writer used. Parallelism widens an existing defect rather than creating
  one.
- Backfill commits per **64k**-ledger unit while CH partitions are **500k** — so
  ~8 writers land in one CH partition. This mismatch is why partition-swap
  atomicity was rejected.

### 4. Re-parsing with a _different_ parser build is unsafe on version-less RMT

RMT with **no version column** picks an **arbitrary** row among equal-version
duplicates. That is harmless when the rows are byte-identical (same code), and
**wrong data** when they are not.

- **Same build → safe.** Re-running produces identical rows; the arbitrary winner
  is irrelevant.
- **Different build → unsafe** on version-less tables: `liquidity_pool_snapshots`,
  `assets`, `transactions`, and the 9 event-log tables. At equal version with a
  changed value, RMT may keep the **stale** row from the earlier attempt.

→ Re-parsing history that was ingested by an older parser build is a real hazard
on those tables. Plan for it (or accept a full re-write of the affected range).

### 5. A version-less RMT row replaces the WHOLE row — emit complete rows

A sparse/partial row does not patch a column; it **replaces the row**, silently
nulling or zeroing everything you left out. Recorded the hard way in 0266 for
`liquidity_pool_snapshots` reserves.

**Live example (task 0356):** the parser emitted both the `state` (before) and
`updated` (after) image per op, so one `(pool, ledger)` got two rows with
different reserves and version-less RMT picked one **at random** — wrong data,
not duplicates. The parser fix (`dedup_final_pool_snapshots`, "keep the last
image") **has landed** and applies to backfill too via the shared path — but the
lesson generalises: **any** version-less RMT table where a parse can emit more
than one row per key is exposed to the same silent corruption, and the bad rows
survive until someone rewrites them.

---

## Indexer: stop it, or not?

The dividing line is **`EXCHANGE TABLES`**. A subcommand that builds a staging
table and swaps it will **lose any live write** that lands between build and
swap.

| Must **STOP** the indexer (staging + `EXCHANGE TABLES`) | No stop needed (RMT, idempotent)                          |
| ------------------------------------------------------- | --------------------------------------------------------- |
| `contract-type-rebuild`, **`repair-tier1`**             | `run` (disjoint ranges), `balance-seed`, `nft-reclassify` |

**Grey zone:**

- **`bootstrap`** — computes its watermark **once** at start to win the RMT race.
  A concurrently-running indexer that advances `last_seen_ledger` past that stamp
  out-versions the bootstrap rows, leaving `seq=0` skeletons. Degraded, not
  corrupt; re-runnable.
- **`nft-reclassify`** — uses `ALTER TABLE … DELETE` (not `EXCHANGE`), so a live
  indexer re-inserts rows the DELETE just removed.

**How to stop it:** see
[`docs/deployment.md` § Backend Lambdas](deployment.md#backend-lambdas--compute-stack).
Short version — quick/temporary: disable the SQS event-source-mapping
(`aws lambda update-event-source-mapping --no-enabled`), **undone by the next
Compute deploy**. Durable: `indexerLambdaConcurrency: 0` in
`infra/envs/production.json` + redeploy Compute.

Either way **nothing is lost** — the S3→SNS notification is always wired
(not gated on concurrency), so a paused indexer still captures events durably in
the queue. Note prod runs `indexerLambdaConcurrency: 1` — a deliberate **single
writer**.

---

## Path A — directly on the Hetzner box (current default)

Build the binary on your laptop, ship it to the box, run it there against
`localhost:8123`. No transfer step, so no schema-drift or ATTACH concerns.

➡️ **Runbook: [`docs/runbooks/backfill_derived_table_reparse_hetzner.md`](runbooks/backfill_derived_table_reparse_hetzner.md)**
— covers both flavours end to end:

- **Flavour A — cheap in-DB backfill**, no re-parse (`arrayJoin` an existing
  column, RMT-dedup). Minutes.
- **Flavour B — from-S3 re-parse** (`s5cmd` pre-fetch + `run --reindex`), when
  the new grain exists only in the ledger XDR. ~1 TB of XDR streamed, ~a day,
  **disk-governed**.

Points worth knowing before you open it:

- The box has **no repo, no cargo, no `aws`/`s5cmd` by default** — the binary is
  cross-compiled on the laptop (zigbuild → glibc 2.31 x86_64) and `scp`'d.
- **Disk is the binding constraint** — root `/dev/md1` ≈ 1.8 TB; the 2026-07 run
  peaked at **92%**. Do not start under ~300 GB free. `--reindex` rewrites _every_
  table, so parts outrun merges — governed by TRUNCATE-ing system logs and
  `OPTIMIZE … PARTITION FINAL`.
- **The target table must be created by hand** — `init.sql` is fresh-install only
  and never re-runs against the live DB.
- **Take the pre-op snapshot first** and drop it **only after validation passes**
  — see [`docs/backups.md`](backups.md).

## Path B — backfill on a separate machine, then transfer to Hetzner

Used when the box itself cannot do the work (CPU/disk), historically with the
range split across machines. **Two transports — they are alternatives for
different situations, not competitors:**

|              | **Direct INSERT** (task 0266, June 2026)                                                  | **FREEZE + rsync + ATTACH PART** ([ADR 0045](../lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md), task 0228, May 2026) |
| ------------ | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Schema**   | **Tolerates drift** — maps columns by name                                                | **Requires byte-identical schema** — ATTACH rejects a structurally different part                                                                                   |
| **Best for** | incremental-on-live (backfill and live ingest are RMT-key-disjoint → **no pause needed**) | bulk / cold-build — moves immutable parts, no re-insert cost                                                                                                        |
| **Cost**     | serialize + zstd + ssh + INSERT (goes through the CH write path)                          | bandwidth/CPU-efficient                                                                                                                                             |
| **Status**   | the **default** since 0266                                                                | kept as the **bandwidth fallback**                                                                                                                                  |

**Decision rule: schema identical + bulk → ATTACH. Schema drifted, or
incremental-on-live → INSERT.**

### B1 — Direct INSERT (default)

Stream from the worker straight into prod over an ssh pipe:

```bash
# WORKER → PROD, one table, a bounded range
clickhouse-client -q "SELECT * FROM <table> WHERE ledger_sequence BETWEEN <a> AND <b> FORMAT Native" \
  | zstd -3 \
  | ssh sorban-prod "zstd -d | docker exec -i app-clickhouse-1 \
      clickhouse-client -q 'INSERT INTO <table> FORMAT Native'"
```

Why 0266 chose this over ADR 0045: prod had a scalar `pool_id` while the worker's
parts had only `pool_ids` — **ATTACH would have rejected the structurally
different part**, INSERT maps by name. The recorded conclusion: _"Transport
default flips to direct INSERT; ADR 0045 stays as bandwidth fallback."_

0266 also recorded that backfill and live ingest are **RMT-key-disjoint**, so no
ingestion pause was needed — _"the 0228 pattern was a cold-build design; this is
incremental-on-live, a different shape."_

### B2 — FREEZE + rsync + ATTACH PART (ADR 0045)

The cold-build path. Executed once (0228, validated GREEN: 980/980 Horizon
hash-set compare, 0.0000% mismatch).

```
worker:  backfill locally
         → scripts/merge-freeze-worker.sh      # FREEZE → /var/lib/clickhouse/shadow/<snap>/ (hardlinks)
         → rsync -avP --partial  <snap>/  hetzner:/var/lib/clickhouse/detached_inbox/<worker>/<snap>/
hetzner: → scripts/merge-attach-hetzner.sh     # mv → detached/ → ATTACH PART → OPTIMIZE FINAL → RELOAD DICT
         → backfill-runner repair-tier1        # mandatory (rule 3) — indexer STOPPED
worker:  → SYSTEM UNFREEZE WITH NAME '<snap>'
```

Gotchas, all recorded:

- **The rsync step has no script.** `merge-freeze-worker.sh` prints it as a manual
  instruction ("Next steps (NOT executed by this script)"). Automation stops on
  both sides of the wire.
- **Atomic-engine UUIDs differ per machine** — hence the `uuid_table_map.json`
  manifest, generated on the worker from `system.tables`; ADR 0045 calls this
  "easy to script but easy to fumble".
- **ATTACH in worker order** (m1 → m2 → m3) so the downstream repair pass has
  deterministic input.
- **Per-partition `OPTIMIZE … FINAL`** after each partition attaches (RMT collapse
  - straddle merge). Pass `optimize_throw_if_noop = 0` and a generous
    `max_execution_time` — the 60 s default is far too tight (0266: partitions
    100–126 took ~37 min).
- **Rollback:** before rsync → `UNFREEZE`. After ATTACH → `DETACH PART`. After
  `OPTIMIZE FINAL` → destructive; recovery means re-rsync from the worker's
  `/shadow/`, which is why workers keep `/shadow/` until the post-attach OPTIMIZE
  is confirmed.

> The operator runbook for this path (`merge-parallel-backfills.md`) was planned
> as task 0233 and **canceled as obsolete** in 2026-05 ("no future parallel
> backfill is planned"). The canceled task still carries the full section-by-section
> spec and is the best historical reference; the scripts' header links to that
> runbook are dangling.

---

## `backfill-runner` reference

**Subcommands:** `run`, `status`, `bootstrap`, `repair-tier1`,
`contract-type-rebuild`, `balance-seed`, `nft-reclassify`. Most one-shot ops
subcommands take `--dry-run`. No separate bins remain.

Seven spent one-shots were removed in lore 0425 — `wasm-upgrade-backfill` (0320),
`upgradeable-backfill` (0327), `nft-reparse` (0296), `soroban-token-flow-backfill`
(0383), `pool-ids-backfill` (0266) with its `scripts/0266/` wrappers,
`assets-id-backfill` (0331), and `metadata-backfill` (0304) — because the live
indexer now does each of them itself, verified on prod per command. Before writing a new one-shot, read the
authoring rule in
[`crates/backfill-runner/README.md`](../crates/backfill-runner/README.md#subcommands--the-rule-for-one-off-passes):
if the signal is not already in ClickHouse, the answer is `run --reindex`, not a
new binary.

**Flags — with the traps:**

| Flag                | Reality                                                                                                                                                                        |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--start` / `--end` | u32, inclusive. This is also how you parallelise (disjoint ranges).                                                                                                            |
| `--reindex`         | Bypasses the resume-skip so an already-ingested range is re-parsed. Without it, re-parsing history is a silent **0-row no-op** — `run` skips whatever is already in `ledgers`. |
| `--keep-partitions` | **Debug only.** "Do not pass this for a real backfill — disk grows linearly."                                                                                                  |
| `--target`          | **Does not exist.** Survives only in stale doc comments; PG was retired (0244), CH is the sole target.                                                                         |
| `--workers`         | **Does not exist.** Run K processes instead.                                                                                                                                   |

**Config** (flag-or-env): `CLICKHOUSE_URL`, `CLICKHOUSE_USER`,
`CLICKHOUSE_PASSWORD`, `CLICKHOUSE_DATABASE`; `CLICKHOUSE_CERT` / `_KEY` / `_CA`
for mTLS via Caddy (all three or none — the CN maps to a CH user);
`SOROBAN_RPC_URL` (optional — unset ⇒ bootstrap skipped, accounts stay skeleton
rows); `BACKFILL_TEMP_DIR` (default `.temp/backfill-runner`).

---

## After any backfill

1. **`repair-tier1`** — mandatory after parallel or `--reindex` runs (rule 3).
   Stop the indexer first.
2. **Validate** — coverage gap-scan + a Horizon / stellar.expert sample compare.
   Re-run the same slice: the RMT-deduped count must stay identical.
3. **Only then** drop the pre-op snapshot ([`docs/backups.md`](backups.md)).

> Steps 1–2 are not bookkeeping. A re-parse that skips `repair-tier1` leaves the
> 12 Tier-1 columns wrong (rule 3), and the damage is invisible until someone
> reads `first_seen_ledger` — which is why "the write finished" is not the same
> as "the backfill is done".

## Superseded — do not follow

- [`lore/3-wiki/backfill-execution-plan.md`](../lore/3-wiki/backfill-execution-plan.md)
  — the RDS `pg_restore` staging cutover. Carries a SUPERSEDED banner (2026-05-20).
  Historical only.
- **ADR 0040** is Postgres-era (BIGSERIAL surrogate-key remapping), superseded in
  practice by the deterministic-cityhash CH design but **not marked as such** — a
  trap for anyone reading it as backfill guidance.
