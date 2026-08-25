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
  (re-running an overlap is a no-op); with identical code the rows are
  byte-identical anyway, and where they are not, the later insert wins. See
  rule 4.
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

**Unless the run writes one table that has no such column.** A re-parse whose
only purpose is to populate a NEW derived table does not need to re-emit the
other twenty-odd — and if it does, it re-arms this trap for nothing. Task 0266
did this with a bespoke harness ("targeted write only — do NOT run the full
persist pipeline"); task 0279 turned it into a flag:

```bash
backfill-runner run --start <A> --end <B> --lp-amounts-only
```

It parses exactly as a normal run does and persists only
`lp_operation_amounts`, so **no Tier-1 column is touched and no `repair-tier1`
is owed**. The trade is that it writes no `ledgers` commit marker (the marker
means "fully ingested", which a targeted pass has not done), so resume cannot
read progress from the DB: on a crash, restart with a narrowed `--start`.
Re-running a range is harmless — the rows are deterministic and the RMT
collapses the duplicates.

Adding a second such mode is a one-line branch beside it in
`sink.rs::write_ledger`; the pattern generalises to any future
one-new-table re-parse.

Two nuances worth knowing:

- This is **not parallel-specific**. Per task 0232 the same columns also drift in
  **live ingest**, because the CH writer cannot afford the read-before-write the
  old PG writer used. Parallelism widens an existing defect rather than creating
  one.
- Backfill commits per **64k**-ledger unit while CH partitions are **500k** — so
  ~8 writers land in one CH partition. This mismatch is why partition-swap
  atomicity was rejected.

### 4. Version-less RMT keeps the LAST ROW INSERTED — so emit one row per key

15 of the RMT tables carry no version column. The rule that matters there is
about **one insert**, not about re-parsing.

**Re-parsing is safe.** A re-parse lands after the data it replaces, and
version-less RMT keeps the last row inserted, so the newer parse wins. Measured
on a ClickHouse 26.3 server (lore 0425), in the shapes that actually occur:

| Shape                                                                     | Result                                               |
| ------------------------------------------------------------------------- | ---------------------------------------------------- |
| 40 unmerged old parts, then a 4-way concurrent re-parse, read via `FINAL` | new value wins, no survivors                         |
| background merges only, never `OPTIMIZE`                                  | new value wins                                       |
| old data already `OPTIMIZE FINAL`-collapsed, then re-parsed               | new value wins                                       |
| partial re-parse (half the keys)                                          | re-parsed keys update, the rest keep their old value |

This also holds structurally: every version-less table is either **keyed by
ledger** — so a re-parse of ledger N only ever competes with its own earlier
parse of ledger N — or a **pure function of an immutable input**
(`wasm_interface_metadata` by `wasm_hash`; `assets`, whose mutable columns are
DEAD and now live in `balance_aggregates` / `asset_enrichment`).

**The real hazard is two rows for one key inside a single insert.** Then "last"
means whatever order the code emitted, which carries no meaning. That is
task 0356: the parser emitted the before- and after-image of a pool per op, so
one `(pool, ledger)` key got two rows with different reserves and ClickHouse kept
one at random — wrong data, not duplicates.

→ **A parse must emit at most one row per key per insert.** Get that right and
`run --reindex` needs no further ceremony. (Tables that DO carry a version — 11
of them, all keyed by an entity rather than a ledger — need it for a different
reason: a re-parse of old ledgers inserts old state last, and only the version
stops it from rolling current state backwards.)

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

| Flag                | Reality                                                                                                                                                                                                  |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--start` / `--end` | u32, inclusive. This is also how you parallelise (disjoint ranges).                                                                                                                                      |
| `--reindex`         | Bypasses the resume-skip so an already-ingested range is re-parsed. Without it, re-parsing history is a silent **0-row no-op** — `run` skips whatever is already in `ledgers`.                           |
| `--lp-amounts-only` | Persists **only** `lp_operation_amounts` (task 0279). Implies `--reindex`. Writes no `ledgers` marker, so resume is manual — narrow `--start`; re-running a range is a no-op. See the rule-3 note below. |
| `--keep-partitions` | **Debug only.** "Do not pass this for a real backfill — disk grows linearly."                                                                                                                            |
| `--target`          | **Does not exist.** Survives only in stale doc comments; PG was retired (0244), CH is the sole target.                                                                                                   |
| `--workers`         | **Does not exist.** Run K processes instead.                                                                                                                                                             |

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

## Checkpoint-snapshot passes (task 0463 / ADR 0055)

The SDF history archive publishes the **complete state of pubnet** at each
checkpoint as a bucket list (~4.5 GB gzipped, 21 files). This is a different
kind of source from everything above: backfills replay **changes** we already
hold, while the snapshot answers **"what does the network have that we do
not?"** — the only question a change-stream can never answer (78.85% of chain
history predates our ledger floor).

One subcommand, read-only except the seed's explicit `--execute`.
There are NO manual exports: each command reads our side straight from
ClickHouse through the same mTLS connection `--execute` inserts through, like
every other corrective command in this crate. (The research-phase probes
`snapshot-tally`/`snapshot-dedup`, the `snapshot-export-sql` helper and the
hand-exported-TSV transport were removed in the 2026-08-20 review;
the seed's dry-run IS the four-way comparison — a separate `snapshot-compare`
carried the same decode and the same verdict behind its own counting shell.)

| Subcommand                                      | What it does                                                                                                                                                                                                                                             | Writes                                                                          |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `snapshot-seed [--artifacts <dir>] [--execute]` | build ALL corrections (missing holdings, closure stamps, ghost zeroing, signers, dimension stubs); dry-run by default; always decodes the freshest checkpoint, writing into `<artifacts>/<checkpoint_ledger>/` (default root `.artifacts/snapshot-seed`) | `balances`, `account_entry_state`, `assets`, `accounts` — only with `--execute` |

**The decision table.** Every one of our rows falls into exactly one verdict,
and the verdict alone decides what (if anything) is written. Read the report's
buckets against this:

| our row          | network at the checkpoint | relation                  | verdict                       | what is written                                             |
| ---------------- | ------------------------- | ------------------------- | ----------------------------- | ----------------------------------------------------------- |
| —                | live                      |                           | `missing`                     | live row @ the entry's own ledger                           |
| open             | absent                    | ours ≥ checkpoint         | `newer than checkpoint`       | nothing — the snapshot is the stale side                    |
| open, amount 0   | absent                    | ours < checkpoint         | `closure`                     | amount 0, closed @ checkpoint                               |
| open, amount > 0 | absent                    | ours < checkpoint         | **`GHOST`**                   | amount 0, closed @ checkpoint, **+ a line in `ghosts.tsv`** |
| closed           | absent                    |                           | `already closed`              | nothing                                                     |
| closed           | live                      | network newer             | **`CLOSED BUT LIVE`**         | live row @ the entry's ledger — re-opened                   |
| closed           | live                      | network not newer         | **`CLOSED vs LIVE conflict`** | nothing — defect signal, see below                          |
| open             | live                      | amounts equal, ours older | `stale`                       | nothing                                                     |
| open             | live                      | amounts equal             | `agree`                       | nothing — the positive control                              |
| open             | live                      | differ, network newer     | `heal`                        | the network's amount @ its ledger                           |
| open             | live                      | differ, ours newer        | `divergent ours newer`        | nothing — the live parser saw more                          |
| open             | live                      | differ, SAME ledger       | **`divergent SAME ledger`**   | nothing — defect signal                                     |

**Version discipline:** a live fact versions on the entry's own
`lastModifiedLedgerSeq`; an absence fact (closure, ghost) on the run's
checkpoint ledger, meaning "true at or before". Never a synthetic stamp. The
`≥ checkpoint` guard is deliberate, not an off-by-one: a checkpoint-versioned
correction written against a row already AT that ledger would be a
same-version ReplacingMergeTree tie, resolved arbitrarily — the exact
nondeterminism this tool exists to remove.

The `missing` split is reported for BOTH populations — trustlines and native
accounts — each with its own below/above-floor counts, 2M-ledger bands and
sample dump. An above-floor missing ACCOUNT is the stronger signal of the two:
native is the population the RPC bootstrap already seeded, so dormancy
explains it less well than it explains a dormant trustline.

Both defect signals get their own sample dump (`divergent_same_ledger.tsv`,
`closed_but_live_conflict.tsv`) — they write nothing, so the dump is the only
way to look at one.

**The two defect signals never auto-heal.** `divergent SAME ledger` means one
of two parsers misread that ledger; `CLOSED vs LIVE conflict` means something
closed a holding the network still has, at a ledger no honest version can
supersede. Both are reported and left alone: adopting a side, or inventing a
version, would bury the only evidence. Expect both at zero on the first run —
`CLOSED vs LIVE conflict` is structurally unreachable until something has
stamped a closure, so it is the alarm for the reconciliation runs below, where
the closures under test are the seed's own previous output or the live
writer's.

**`--execute` needs a write-capable ClickHouse identity.** The laptop mTLS cert
maps to user `dev_read`, whose profile sets `readonly = 1` and
`max_execution_time = 30` — an INSERT is refused on the readonly setting before
grants are consulted, and the row volume would exceed the execution ceiling
regardless. The dry-run is unaffected: it only reads, in bounded slices. Confirm
the identity before the run, not at the prompt (`SELECT currentUser()` and
`system.settings` answer it read-only; never test a write by writing).

**The seed's ordering contract (do not reorder):**

1. Deploy the lifecycle writer (the indexer that stamps `closed_at_ledger`)
   FIRST.
2. Run `snapshot-seed --execute` against a checkpoint taken
   AFTER the deploy. Reversed, every removal between checkpoint and deploy is
   written by the old writer as a plain zero with a HIGHER version than the
   seed's closure — the ghost resurrects. The tool reads our rows itself at
   run time, so input freshness — measured as the dominant lever on correction
   volume — is no longer an operator concern. Churn between the dry-run read
   and the execute read is absorbed by the `>= checkpoint` guard (such rows
   are classified newer-than-checkpoint and left alone), so the dry-run's
   `summary.txt` is a close estimate of the execute's counts, never a
   contradiction of them.

**No indexer stop is needed.** Every seeded row versions on a real ledger:
live data on the entry's own `lastModifiedLedgerSeq`, closures on the run's
checkpoint ledger (semantics: "closed at or before"). ReplacingMergeTree keeps
the higher version, so the live writer's newer rows always win regardless of
load order.

**After the seed (standing requirement, not advice):** measure the coverage
achieved for trustlines and accounts separately, AND cross-check a sample
against the RPC route regardless of the result. The 200-account probe from
task 0463's notes is the repeatable check: zero accounts where the chain holds
more live zero trustlines than we do, and zero where we show more than the
chain.

**Provenance:** the run writes `manifest.json` (checkpoint ledger + the 21
bucket hashes) into `<artifacts>/<checkpoint_ledger>/`. The archive is
content-addressed, so that manifest alone IDENTIFIES the snapshot a run
decoded — the planned LP merge (ADR 0056) reads it to know which checkpoint
this seed used. `ghosts.tsv` records every positive-amount
row the seed zeroed.

## After any historical re-parse: reconcile against the snapshot (MANDATORY)

A re-parse of already-ingested ledgers with CHANGED writer code writes rows at
the SAME ReplacingMergeTree versions the old code used. Where the new code
emits different content, the table holds two rows at one version and the merge
picks a winner **arbitrarily** — and `argMax` reads flip the same coin. This is
not hypothetical: the 2026-06-23 merge-tombstone fix plus a re-parse of
54M–63.04M left **1,238,583** such keys in `balances`, every one a merged
account randomly showing 0 or its stale pre-merge balance.

There is no in-schema defence. A "later insert wins" tiebreaker would make a
REGRESSED re-parse deterministically overwrite good data — worse than a
detectable tie. The arbiter is the network:

1. After the re-parse, run the standing tie query (task 0503 carries it per
   table): keys with more than one distinct content at one version.
2. Run `snapshot-seed` (dry-run) against a fresh checkpoint and read its report.
   Between-runs ties surface as `divergent SAME ledger` (live entities) and as
   closure/ghost corrections (dead ones).
3. Run `snapshot-seed` (dry-run → review → `--execute`): dead-entity ties are
   repaired outright at the checkpoint version; same-ledger divergences on
   LIVE entities are reported for a human call — one of two parser versions is
   wrong, and auto-adopting either would bury the evidence.

This is the same tooling as the one-off 0463 seed; the seed is one-off, the
reconciliation is not.

## Superseded — do not follow

- [`lore/3-wiki/backfill-execution-plan.md`](../lore/3-wiki/backfill-execution-plan.md)
  — the RDS `pg_restore` staging cutover. Carries a SUPERSEDED banner (2026-05-20).
  Historical only.
- **ADR 0040** is Postgres-era (BIGSERIAL surrogate-key remapping), superseded in
  practice by the deterministic-cityhash CH design but **not marked as such** — a
  trap for anyone reading it as backfill guidance.
