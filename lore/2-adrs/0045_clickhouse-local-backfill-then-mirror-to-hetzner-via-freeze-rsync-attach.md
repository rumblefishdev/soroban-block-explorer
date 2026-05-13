---
id: '0045'
title: 'ClickHouse local-backfill → Hetzner mirror via FREEZE + rsync + ATTACH PART'
status: proposed
deciders: [fmazur]
related_tasks: []
related_adrs: ['0010', '0040', '0044']
tags: [clickhouse, backfill, migration, hetzner, infrastructure]
links: []
history:
  - date: 2026-05-13
    status: proposed
    who: fmazur
    note: 'ADR created — captures the chosen path for getting the 11.5M-ledger ClickHouse backfill from a developer laptop onto the production Hetzner box before any code or schema work begins.'
---

# ADR 0045: ClickHouse local-backfill → Hetzner mirror via FREEZE + rsync + ATTACH PART

**Related:**

- [ADR 0010 — Local backfill over Fargate](./0010_local-backfill-over-fargate.md)
- [ADR 0040 — Multi-laptop backfill snapshot merge — schema hazards and playbook](./0040_multi-laptop-backfill-snapshot-merge-hazards.md)
- [ADR 0044 — ClickHouse pilot parallel store](./0044_clickhouse-pilot-parallel-store.md)

---

## Context

The ClickHouse store stood up in [ADR 0044](./0044_clickhouse-pilot-parallel-store.md)
is now schema-stable and the `backfill-runner` `--target clickhouse` path is
landing real rows (smoke fixture: 128k ledgers, 9 GiB on disk). The next step
is a full pubnet backfill: **~11.5M ledgers**, projected on-disk footprint
**~800 GB** (linear extrapolation from the 9 GiB fixture).

That backfill has to end up on a production ClickHouse instance that will run
on a Hetzner dedicated server (likely under Docker, mirroring the local
compose topology). The question this ADR settles is **how the 800 GB of
populated ClickHouse data physically gets from the developer laptop onto the
Hetzner box**, given the constraints below.

### Constraints

- **Developer uplink ~55 Mb/s** (~6.9 MB/s in bytes). A naive 800 GB upload
  takes ~33 h netto, ~2 d realistically with TCP/TLS overhead.
- **Local backfill takes ~6 d** on the developer laptop (Orange 140/55 Mbps,
  Pavilion 15 class CPU). Backfill is CPU-bound on XDR parsing + CH writer,
  not S3 reads.
- **Laptop disk is finite.** A 1 TB SSD that already holds 800 GB of CH parts
  has no room for a parallel 600–720 GB tarball — any approach that requires
  a second on-disk copy of the data does not fit.
- **`aws-public-blockchain` S3 egress is sponsored** (AWS Open Data
  Program) — pulling from Hetzner costs €0. Cross-region cost mentioned in
  `crates/backfill-runner/README.md` only applies to within-AWS reads from
  a non-`us-east-1` region.
- **Production must run the same schema** as the smoke fixture. No schema
  change can be required by the migration path — otherwise we are no longer
  validating "the thing we will run in prod" with the local fixture.
- **`backfill-runner` already produces partition-aligned writes** (`open_partition`
  → `write_ledger × N` → `commit`) — parts are clean MergeTree parts after
  commit, suitable for cold transport between instances.

---

## Decision

1. **Run the full 11.5M-ledger backfill locally**, against the existing
   Dockerised ClickHouse on the developer laptop, exactly the same way the
   128k-ledger smoke fixture was produced.

2. **After backfill completes**, mirror the populated database onto the
   Hetzner production CH via the three-step CH-native sequence:

   - **`ALTER TABLE … FREEZE`** on every table in `default`. Creates
     hardlinks under `/var/lib/clickhouse/shadow/<N>/` pointing to the
     currently-active parts. Zero data copied, zero additional disk
     consumed (refcount-only).
   - **`rsync -avP --partial`** of `shadow/<N>/` over SSH to the Hetzner
     host. Resumable, streamed over the laptop's ~6.9 MB/s uplink, ~2 d
     wall-clock. Background CH operations on the laptop are unaffected
     because the frozen parts are pinned by hardlinks regardless of any
     merge that supersedes them in `store/`.
   - **`ALTER TABLE … ATTACH PART '...'`** on Hetzner for each part,
     after `mv` from the staging location into the destination table's
     `detached/` subdirectory (the latter is keyed by the table's
     Atomic-engine `<uuid>`, which differs between the two instances and
     must be looked up on Hetzner via `system.tables.uuid`). Parts are
     incorporated atomically, no re-parsing, no re-sorting, no
     re-compression.

3. **Verify** with row counts and aggregate sanity (`SELECT count() FROM …`,
   `SELECT min/max(ledger_sequence) FROM …`) on each table before pointing
   any production traffic at the Hetzner box.

4. **Unfreeze locally** (`ALTER TABLE … UNFREEZE WITH NAME '<N>'`) once
   the Hetzner side passes verification, to release any disk pinned by
   shadow hardlinks.

The local CH instance is **kept intact post-migration** as a debug/reference
copy until at least one production deploy cycle has shipped from Hetzner
without rollback.

---

## Rationale

### Why local-backfill-first rather than backfill-directly-on-Hetzner

A direct-on-Hetzner backfill would be **faster** (single ~3 d run on a
beefier box and faster uplink to S3, vs. 6 d local + 2 d rsync = ~8 d). It
was nonetheless rejected because:

- **We lose the local debug copy.** Every parser regression, schema rough
  edge, and write-path peculiarity discovered after the production data
  lands has to be reproduced either by re-running the backfill (expensive)
  or by sampling from production (risky). A populated local CH is a
  flat-cost asset for the entire post-deploy lifecycle.
- **The local backfill is the validation event** that the smoke fixture
  scales to full pubnet. Running it on a different box defers that
  validation to a place where rollback is more expensive (production
  CH on a paid server vs. local Docker we can wipe freely).
- **Hetzner box specs are not yet fixed.** Choosing the migration path
  before the production box specs are finalised means we cannot lean on
  a "Hetzner is fast" assumption. A laptop-only path is fully under our
  control with known characteristics.

### Why FREEZE + rsync + ATTACH PART rather than a cold tarball

Both move the same bytes over the same uplink in approximately the same
wall-clock time (~2 d for 600–800 GB at ~6.9 MB/s). The differentiator is
**local disk pressure** and **operational resilience at this scale**:

- **Disk:** FREEZE produces hardlinks — refcount-only, no second copy
  of the data. The 800 GB stays as 800 GB. A tarball would require an
  additional ~600–720 GB of free space on the laptop SSD to materialise
  before upload; a 1 TB SSD that already holds the backfill physically
  cannot accommodate it.
- **CH availability:** FREEZE runs in seconds with CH up. The tarball
  path requires `docker compose stop clickhouse` for the duration of
  the tar (~30–60 min at 800 GB).
- **Resumability granularity:** rsync resumes partial files; ATTACH PART
  is atomic per part. A network interruption mid-rsync costs minutes to
  restart the affected file, not the whole transfer. A tarball is one
  ~600 GB file; rsync still resumes byte-level, but a corruption found
  in the middle (e.g. during extract) forces a re-transfer of the
  remainder.
- **Bytes on the wire:** parts are already ZSTD-compressed internally
  (per init.sql, columns use ZSTD codecs; `topics_xdr` / `data_xdr` /
  `wasm_interface_metadata.metadata` use explicit `ZSTD(3)`). External
  tarball compression saves ~10–15% on top. Not enough to outweigh the
  disk-pressure and stop-CH costs.

### Why this beats CH-native logical replication (ReplicatedMergeTree)

ReplicatedMergeTree + ClickHouse Keeper would give real-time replication
during the backfill and be the canonical CH answer for hot replication.
It is rejected for this **one-shot migration** because:

- **Schema rewrite.** All 17 tables in `init.sql` would have to change
  engine from `ReplacingMergeTree` to `ReplicatedReplacingMergeTree` with
  a per-table ZooKeeper path. The smoke fixture has already validated
  the current schema; rewriting it before the production backfill
  invalidates that validation work.
- **Keeper infrastructure.** Either a separate Keeper deployment (third
  machine, or co-located on Hetzner with reliability implications) or
  ClickHouse-embedded Keeper has to be stood up, monitored, secured.
  The cost-benefit only pays off for ongoing HA, which is not part of
  the current production target.
- **Network reachability.** Laptop ↔ Keeper ↔ Hetzner-CH requires
  inbound ports to one of those endpoints. The laptop sits behind a
  consumer ISP; reverse-tunneling that for the duration of a 6-day
  backfill is operationally fragile.

---

## Alternatives Considered

### Alternative 1: Backfill directly on Hetzner from S3 (no local intermediate)

**Description:** Stand up CH on Hetzner first, point `backfill-runner` at the
Hetzner CH endpoint, ingest all 11.5M ledgers from `aws-public-blockchain`
S3 directly on the Hetzner box.

**Pros:**

- Total wall-clock ~1.5–3 d (Hetzner box has Gbps+ uplink to AWS and
  better CPU than a laptop).
- No data leaves the AWS↔Hetzner path — no developer-uplink involvement.
- Zero migration step, zero `ATTACH PART` choreography.
- Production CH is built by the same pipeline that will run in steady
  state (the indexer), guaranteeing reproducibility.

**Cons:**

- No local debug copy of the populated database for post-deploy
  investigations.
- Validates the full backfill against a box whose specs are not yet
  fixed; if Hetzner spec choice goes wrong, the backfill is the
  feedback loop.
- Forces Hetzner production box to be paid-for and provisioned before
  any of the local validation work matures.

**Decision:** REJECTED — losing the local copy and pushing validation
onto paid infrastructure outweighs the wall-clock saving for this one-time
migration. Revisit if local-disk constraints make local backfill
unworkable, or if Hetzner specs are confirmed early.

### Alternative 2: Cold tarball after local backfill, upload, extract on Hetzner

**Description:** After local backfill completes,
`docker compose stop clickhouse` and `tar -cf - -C /data . | zstd -T0 -9 > snapshot.tar.zst`
the entire ClickHouse data volume; upload the resulting ~600–720 GB
tarball over rsync; extract into a fresh CH volume on Hetzner; start CH.

**Pros:**

- Same toolchain that was used for the 9 GiB smoke fixture snapshot —
  team is already familiar.
- Tarball is a portable artifact suitable for archival / rollback storage.
- Slightly smaller on the wire (~10–15% smaller than raw parts).

**Cons:**

- Requires **~600–720 GB of free disk on the laptop** in addition to the
  800 GB CH data already there. A 1 TB SSD physically cannot hold both.
- Requires CH downtime on the laptop for the duration of the tar
  (~30–60 min at 800 GB).
- Restore on Hetzner is byte-for-byte volume copy — if the destination
  volume is not pristine, requires destroy/recreate workflow.
- No per-part atomicity; an interruption late in transfer costs the
  uncompressed remainder.

**Decision:** REJECTED — local disk capacity is the hard blocker. Even on
a 2 TB SSD where it fits, the per-part atomicity and zero-downtime of the
FREEZE path is preferable for this scale.

### Alternative 3: ReplicatedMergeTree + ClickHouse Keeper, replicate during backfill

**Description:** Convert all 17 tables to `ReplicatedReplacingMergeTree`
(and `ReplicatedMergeTree` for `ledgers` / `wasm_interface_metadata`),
stand up ClickHouse Keeper somewhere reachable from both laptop and
Hetzner, and let CH-native replication stream parts to Hetzner as the
backfill runs.

**Pros:**

- Total wall-clock ≈ backfill time only (~6 d) — replication runs
  concurrently with ingest.
- Built-in retry, dedup, atomicity at the replication layer.
- Production-grade pattern that would carry over to a future HA setup.

**Cons:**

- Schema rewrite required across the entire `init.sql`.
- Keeper infrastructure to deploy and operate for a one-time event.
- Network reachability (laptop ↔ Keeper ↔ Hetzner) over a consumer
  ISP for 6 days is fragile.
- Invalidates the smoke-fixture validation done against the current
  (non-replicated) schema.

**Decision:** REJECTED for this migration. Revisit if/when production
gains a real HA requirement — at that point a Replicated rebuild is on
the table, but driven by the HA goal, not by migration tactics.

### Alternative 4: Per-partition FREEZE + rsync + ATTACH **during** backfill (streaming mirror)

**Description:** Same primitives as the chosen Decision, but driven by a
wrapper script that watches for each `commit` event from `backfill-runner`
and ships that partition's parts to Hetzner immediately. Hetzner is
already-populated by the time local backfill finishes.

**Pros:**

- Total wall-clock ≈ 6 d (mirror runs in parallel with backfill). The
  laptop's ~6.9 MB/s uplink can sustain the ~1.5 MB/s average produced
  by the backfill rate with ~4.6× headroom.
- Hetzner is "live" earlier.

**Cons:**

- Requires a wrapper script that hooks into the
  `open_partition`/`commit` lifecycle of `backfill-runner` (no such
  hook exists yet). ~50 lines of bash, but extra moving piece.
- Failure modes multiply: laptop network blip mid-backfill, Hetzner
  unavailable, partial ATTACH on a partition that local has now
  merged — each needs handling.
- Saves ~2 d wall-clock at the cost of a ~6 d window where laptop
  and Hetzner have to both be reachable and healthy.

**Decision:** REJECTED for now. The total elapsed-time saving (~2 d in 8) does not justify the operational complexity for a one-time event.
The chosen Decision (mirror **after** backfill) has the same per-part
mechanics but runs them as a single serial step.

---

## Consequences

### Positive

- **Local CH stays intact** post-migration as a flat-cost debug/reference
  copy, queryable for the entire post-deploy lifecycle.
- **Zero local disk overhead** during the mirror window — hardlinks
  occupy bytes, not gigabytes.
- **No CH downtime** on the laptop during the mirror window — FREEZE
  is online.
- **Resumability** at the rsync level (partial file) and at the ATTACH
  level (per part) — network blips do not force a full restart.
- **Schema is not touched** — the production deploy validates the same
  `init.sql` that the local fixture validates.
- **Deterministic restore** — parts are byte-identical between
  laptop and Hetzner; restore is not a re-parse, it is a hardlink
  rehoming.

### Negative

- **Total elapsed wall-clock ~8 d** (6 d backfill + 2 d rsync) — ~5 d
  longer than a direct-on-Hetzner backfill would have been.
- **Laptop must stay online** for the rsync window (~2 d). rsync
  `--partial -P` handles interruptions, but the laptop has to be
  reachable.
- **Atomic-engine `<uuid>` mapping** has to be handled in the ATTACH
  script — the destination table's `<uuid>` differs from the source's,
  so the staging-to-`detached/` `mv` must look up `system.tables.uuid`
  on Hetzner before each move. Easy to script but easy to fumble.
- **Production CH must have schema applied before ATTACH** — the
  `db-clickhouse-init` sidecar (or equivalent) must run against the
  Hetzner instance first; ATTACH PART against a non-existent table
  fails.
- **Background merger may temporarily inflate local disk** during the
  rsync window (~5–10% worst case) — parts hardlinked in `shadow/`
  are pinned even after newer merged parts supersede them in `store/`.
  Acceptable margin given the laptop's free space after backfill, but
  worth flagging.
- **No automation written yet** — the FREEZE-all-tables and
  remote-ATTACH-loop wrappers are ad-hoc work for whenever backfill
  completes.

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md),
any ADR that changes the shape of the system MUST be landed together with the
corresponding updates to `docs/architecture/**`. Tick each that applies before
marking the ADR `accepted`:

- [ ] `docs/architecture/technical-design-general-overview.md` updated (or N/A)
- [ ] `docs/architecture/database-schema/database-schema-overview.md` updated — N/A: schema is unchanged; this ADR is about how an unchanged schema's data physically reaches Hetzner.
- [ ] `docs/architecture/backend/backend-overview.md` updated — N/A: no API or service-layer change.
- [ ] `docs/architecture/frontend/frontend-overview.md` updated — N/A: no frontend impact.
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md` updated — N/A: pipeline implementation does not change; backfill-runner is invoked the same way locally.
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md` updated — production Hetzner ClickHouse becomes a documented topology element; needs an entry describing how it is populated (this ADR) and its relationship to the local Dockerised CH.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated — N/A: parsing path is unaffected.
- [ ] This ADR is linked from each updated doc at the relevant section.

`technical-design-general-overview.md` and `infrastructure-overview.md`
are the only candidates for updates; flagged as TODO before this ADR
moves to `accepted`.

---

## References

- `crates/db-clickhouse/schema/init.sql` — current CH schema; the
  invariant that survives the mirror.
- `crates/backfill-runner/README.md` — `--target clickhouse` flow and
  partition-writer lifecycle.
- ClickHouse docs: [`ALTER TABLE … FREEZE`](https://clickhouse.com/docs/en/sql-reference/statements/alter/partition/#freeze-partition),
  [`ALTER TABLE … ATTACH PART`](https://clickhouse.com/docs/en/sql-reference/statements/alter/partition/#attach-partitionpart).
