---
id: '0047'
title: 'ClickHouse on Hetzner as primary API datastore (PG retirement scheduled)'
status: accepted
deciders: [stkrolikiewicz, fmazur]
related_tasks: ['0228', '0239', '0241', '0243']
related_adrs: ['0044', '0045']
tags:
  [
    architecture,
    clickhouse,
    hetzner,
    api,
    primary-datastore,
    pg-retirement,
    cost-optimization,
    olap,
  ]
links: []
history:
  - date: '2026-05-20'
    status: accepted
    who: stkrolikiewicz
    note: >
      Ratification of the architectural shift from "Postgres-primary + CH-pilot"
      (ADR 0044) to "ClickHouse-primary + PG-retirement" — agreed in the M1-M3
      sequencing planning session. This ADR elevates the CH pilot from parallel
      evaluation store to the single source of truth for API reads. Aligned with
      ADR 0045 (FREEZE+rsync+ATTACH transport for historical backfill) and the
      operational reality that RDS pg_restore staging cutover (the original
      backfill-execution-plan) is now a dead path.
---

# ADR 0047: ClickHouse on Hetzner as primary API datastore

**Related:**

- [ADR 0044 — ClickHouse pilot parallel store](./0044_clickhouse-pilot-parallel-store.md)
- [ADR 0045 — ClickHouse local-backfill → Hetzner mirror via FREEZE + rsync + ATTACH PART](./0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md)
- [Task 0228 — parallel-backfill merge into Hetzner CH + post-merge validation](../1-tasks/active/0228_FEATURE_parallel-backfill-merge-and-validation/README.md)
- [Task 0239 — AWS-side cutover: Lambdas out-of-VPC, mTLS to Hetzner, NAT GW + RDS decommission](../1-tasks/backlog/0239_FEATURE_aws-side-cutover-mtls-to-hetzner.md)
- [Task 0241 — Indexer Lambda hard swap PG→CH + cutover runbook](../1-tasks/backlog/0241_FEATURE_indexer-hard-swap-pg-to-ch-and-cutover-runbook.md)
- [Task 0243 — API feature flag per module — gradual PG↔CH migration](../1-tasks/backlog/0243_FEATURE_api-feature-flag-pg-to-ch-per-module.md)

---

## Context

When ADR 0044 was drafted (2026-05-08), ClickHouse was scoped as a **parallel
pilot** sitting next to the existing Postgres-primary path:

> "Stand up a parallel ClickHouse store mirroring the current Postgres schema
> (...) to evaluate analytical throughput and storage trade-offs without
> touching the existing Postgres path. (...) no indexer dual-write, no API
> read-path changes." — ADR 0044 history entry, 2026-05-08

Since then several facts have changed the picture:

1. **CH schema stable on real ledgers.** Task 0206 landed the production CH
   writer (`db_clickhouse::persist::PartitionWriter`), 17 tables + 1
   Dictionary. Smoke fixture validated on 128k ledgers (~9 GiB on disk).
   Empirical compression amendments (codec ZSTD(3) for `topics_xdr`/`data_xdr`)
   confirmed strong storage characteristics for the OLAP workload.

2. **Cost asymmetry made RDS path untenable.** Per task 0216 research:
   AWS NAT Gateway + RDS dominate the running cost for our workload
   (~$345/month combined at staging-sized config, scaling worse with traffic).
   A Hetzner-hosted dedicated CH box (Server Auction: 12+ core AMD Ryzen 9,
   128 GB DDR4 ECC, 2× 1.92 TB U.2 NVMe RAID 1, ~€60/month) plus AWS-side
   Lambda-only egress comes in well below.

3. **Hetzner deployment shipped.** Task 0227 delivered the Hetzner infra
   artefacts (Ansible playbook, mTLS CA, Caddy mTLS gate, Docker compose
   stack). Production CH endpoint live behind mTLS at `<ch-prod-domain>:443`.

4. **Postgres staging cutover became a dead path.** The original
   `lore/3-wiki/backfill-execution-plan.md` proposed `pg_dump` → `pg_restore`
   into staging RDS, then live indexer cutover. ADR 0045 replaced that
   transport with FREEZE + rsync + ATTACH PART directly into Hetzner CH
   (the actual mechanism for getting 11.5M-ledger backfill onto the prod
   box). With no PG cutover, there is no path for PG to ever have the full
   historical corpus on the prod side.

5. **Architectural skew between code and infra plan.** Plan 0228 line 35
   stated "API stays on AWS (Lambdas + Galexie)", which is true for compute
   placement but does not address what the API **reads**. The clarification
   is: Lambdas stay on AWS, but they read Hetzner CH (over public internet,
   mTLS-authenticated). Task 0239 (AWS-side cutover) builds the mTLS connection
   layer; task 0241 (indexer hard swap) writes only to CH; task 0243 rewrites
   API queries from sqlx to a ClickHouse client.

The decision before this ADR: keep CH as a "parallel pilot" indefinitely,
or formalize that CH is the prod primary and PG is being retired.

---

## Decision

**ClickHouse on Hetzner (`ch-prod-01`) is the primary datastore for all API
reads.** RDS Postgres is retired in 0239 Phase 6 (scheduled for M3 — post
launch, as the final cost-optimization step).

Specifically:

1. **Live indexer Lambda** (task 0241) writes only to Hetzner CH. No
   dual-write to PG. Hard swap on deploy. PG path in `crates/indexer/` is
   removed.

2. **API Lambda** (task 0243) reads from Hetzner CH for all 9 handler
   modules. Migration is gradual (feature flag per module — env var
   `API_DATASOURCE_<MODULE>`), defaulting to PG during the transition
   window. Each module flips to CH after a 24h smoke and 7d stable signal.
   After all modules flip, task 0244 removes the sqlx + PG query path.

3. **Galexie** stays on AWS ECS Fargate (write-path only — produces
   `LedgerCloseMeta` files in S3). No change.

4. **RDS PostgreSQL** is decommissioned in 0239 Phase 6 (M3). Final
   manual snapshot is retained 30 days as the only rollback path.

5. **Single-node CH on Hetzner** is the failure boundary for the API.
   No read replica, no HA at launch. Backup strategy: Borg → BX21 Storage
   Box (per task 0236, not yet ordered). A separate restore-runbook task
   is spawned before prod launch (M3).

---

## Rationale

**Cost.** NAT GW + RDS dominate AWS spend for the indexer + API workload.
Hetzner egress is sponsored (via AWS Open Data Program for the
`aws-public-blockchain` S3 source). The Hetzner box runs at roughly 6-8%
of the equivalent RDS-on-AWS cost while comfortably exceeding the
performance budget on a single-node MergeTree.

**OLAP fit.** All public API endpoints are read-only analytical queries
over append-only ledger data (list with pagination, aggregations, filtered
scans). MergeTree column store + partition pruning is the natural shape;
PostgreSQL OLTP overhead (MVCC bookkeeping, row-store I/O amplification,
shared buffers thrashing on long scans) is wasted on this workload.
Reference set of CH endpoint queries (task 0207 archive,
`docs/architecture/database-schema/endpoint-queries/01..17_*.sql`) already
validates the query patterns end-to-end on the CH pilot.

**Storage characteristics.** ZSTD(3) on `topics_xdr`/`data_xdr` compresses
the repeated JSON envelope ~20-40× (empirically measured post-0206). Whole
soroban-era projected on-disk footprint ~800 GB, comfortably fitting the
1.7 TB usable RAID 1 box with headroom for retention growth.

**No-rewrite-cost for fetchers.** Enrichment fetchers (`Sep1Fetcher`,
`NftTokenUriFetcher`) are storage-agnostic and reused verbatim by task
0231 (CH enrichment port). Only the persist layer and dispatch loop are
new.

**Operational simplicity post-retirement.** Once 0239 Phase 6 lands, the
runtime stack is: Galexie ECS + indexer Lambda (AWS, mTLS to CH) + API
Lambda (AWS, mTLS to CH) + Hetzner CH (Docker compose under Caddy + mTLS).
No RDS, no RDS Proxy, no NAT Gateway, no bastion. Significantly smaller
attack surface and operational toil.

---

## Alternatives Considered

### Alternative 1: Keep CH as parallel pilot, PG as primary

**Description:** Continue running both stores; indexer dual-writes both,
API reads PG. CH for analytics only.

**Pros:**

- Zero code change to the API path.
- Both stores serve their natural workload (OLTP and OLAP).
- Read replica + Multi-AZ for RDS available if SLA demands grow.

**Cons:**

- Doubles the write cost (Lambda writes + storage on both sides).
- Doubles the operational surface (two schemas to evolve, two backup
  strategies, two failure modes).
- AWS cost remains dominated by NAT GW + RDS, defeating the cost win
  from the Hetzner box.
- No clear "primary" — every architectural decision has two answers.

**Decision:** REJECTED — the cost win from Hetzner only materializes if
RDS is actually decommissioned. The "parallel forever" stance preserves
the worst-of-both characteristics.

### Alternative 2: Move CH to AWS (RDS for Aurora-MySQL-CH, ClickHouse Cloud, or self-hosted on EC2)

**Description:** Same architectural shift (CH-primary, PG-retired) but
host CH on AWS to keep one cloud.

**Pros:**

- Single cloud, single bill.
- Simpler networking (no public-internet egress, no mTLS).
- AWS IAM-based access control already established.

**Cons:**

- ClickHouse Cloud is significantly more expensive than a Hetzner box for
  this workload (analytical query patterns, single-tenant).
- Self-hosted on EC2 = same NAT GW / VPC overhead we're trying to escape.
- Loses the sponsored egress benefit for `aws-public-blockchain` S3 reads
  (Hetzner pulls those for free via the AWS Open Data Program; AWS-region
  reads from a non-`us-east-1` region pay cross-region transfer per
  `crates/backfill-runner/README.md`).

**Decision:** REJECTED — cost analysis (task 0216) showed Hetzner
dedicated server delivers the target performance budget at a fraction of
ClickHouse Cloud or self-hosted EC2 cost.

### Alternative 3: PG as primary, CH as read replica via CDC

**Description:** Keep PG as the system of record. Replicate to CH
via Debezium / `pg_recvlogical` for analytical queries. API picks PG or
CH per query.

**Pros:**

- PG remains the OLTP authority.
- CH stays read-only (simpler operational model).
- Existing PG path stays unchanged.

**Cons:**

- Adds a CDC component (Debezium / equivalent) — new operational surface.
- Replication lag adds latency for "current tip" queries that need recent
  state from CH.
- Doesn't address the AWS cost problem (RDS + NAT GW remain).
- We don't actually need PG as the OLTP authority — the data is
  append-only ledger writes, not transactional state.

**Decision:** REJECTED — adds complexity without solving the cost issue
that motivates the shift.

---

## Consequences

### Positive

- **AWS cost cut.** RDS + NAT GW + RDS Proxy + bastion all removed in
  0239 Phase 6. Monthly bill drops to Lambda invocations + S3 + Galexie
  ECS + DNS + Secrets Manager (under $100/month at the target traffic
  budget).
- **OLAP-optimized read path.** Column store, partition pruning, ZSTD
  compression all suit the API's analytical workload.
- **Reduced operational surface.** One primary store, one schema to
  evolve, one backup target.
- **Storage characteristics meet 11.5M-ledger backfill.** ~800 GB on
  1.7 TB box leaves room for several years of retention growth before
  capacity becomes a concern.
- **Indexer code simplifies.** No dual-write logic; `persist_ledger` is
  one CH write per ledger.
- **API code simplifies (post-0244).** One query layer, one connection
  pool, one set of integration tests.

### Negative

- **Single-node failure domain.** No read replica at launch. Borg backup
  → BX21 Storage Box is the recovery path; restore takes hours, not
  seconds. Acceptable for the launch budget; HA progression (read replica
  or multi-node MergeTree) deferred until SLA demands it (per the same
  staged-availability model in §7.3 of the infrastructure overview).
- **Public-internet egress to Hetzner.** Lambdas reach CH over the
  public internet, authenticated by mTLS. ~30-60 ms latency from
  `us-east-1` to Falkenstein. Within the p95 <200 ms budget but reduces
  the headroom for downstream work (query planning, partition reads).
- **Stale API window during transition (M1→M2).** After 0241 hard swap
  but before 0243 completes per-module rollout, API serves stale PG data
  (PG no longer receives new ledgers from indexer). Acceptable
  pre-launch trade-off; team-aligned per the M1-M3 sequencing plan.
- **Cross-cloud secret distribution.** mTLS client certs live in AWS
  Secrets Manager (one bundle per service per environment). Cert rotation
  needs a documented procedure (TODO in 0239 risk-considerations).
- **No bastion for emergency RDS access post-decommission.** Once
  Phase 6 runs, RDS is gone. The final manual snapshot is the only
  rollback path for 30 days; after that, full restore from CH historical
  - recompute-state is the only recovery option for any PG-only data.
    We've audited that no live state depends on PG-only data.

---

## Delivery Checklist

Per [ADR 0032](./0032_docs-architecture-evergreen-maintenance.md), any ADR
that changes the shape of the system MUST be landed together with the
corresponding updates to `docs/architecture/**`.

- [x] `docs/architecture/technical-design-general-overview.md` updated —
      D1 AC #2/#3 ("RDS" → "ClickHouse") + sweep dla deliverable storage refs (this PR)
- [ ] `docs/architecture/database-schema/database-schema-overview.md` updated
      — N/A — schema docs already track CH schema independently of this ADR
      (per ADR 0044). Update if subsequent sweep finds stale RDS refs.
- [ ] `docs/architecture/backend/backend-overview.md` updated — N/A — no
      backend doc edit required by this ADR; backend code changes belong to
      task 0243 (API rewrite) which carries its own docs-update obligation.
- [ ] `docs/architecture/frontend/frontend-overview.md` updated — N/A —
      no frontend impact.
- [ ] `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
      updated — N/A — indexing pipeline doc edit belongs to task 0241
      (indexer hard swap), not this ADR. ADR 0047 only formalizes the
      decision; 0241 carries the pipeline-shape change.
- [ ] `docs/architecture/infrastructure/infrastructure-overview.md`
      updated — N/A — infra topology change belongs to task 0239 (AWS-side
      cutover) which carries the docs-update obligation for the
      Lambda-out-of-VPC + RDS decommission shape.
- [ ] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` updated —
      N/A — no XDR parsing impact.
- [x] This ADR is linked from each updated doc at the relevant section
      (technical-design-general-overview.md updated in this PR; other docs
      marked N/A per above; downstream tasks carry their own ADR-link
      obligation).

---

## References

- [AWS Open Data Program — `aws-public-blockchain`](https://registry.opendata.aws/aws-public-blockchain/)
  — sponsored egress for ledger archive reads
- [ClickHouse MergeTree documentation](https://clickhouse.com/docs/engines/table-engines/mergetree-family/mergetree)
  — partition pruning + ZSTD codec rationale
- [Hetzner Server Auction](https://www.hetzner.com/sb/) — hardware tier
  (12+ core AMD Ryzen 9, 128 GB DDR4 ECC, 2× 1.92 TB U.2 NVMe)
