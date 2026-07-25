---
id: '0248'
title: 'DOCS: comprehensive sweep of pre-pivot RDS prose in tech design + infrastructure docs (post ADR 0047)'
type: DOCS
status: completed
related_adr: ['0044', '0045', '0047']
related_tasks: ['0242']
tags:
  [
    priority-medium,
    effort-medium,
    layer-docs,
    architecture,
    grooming,
    follow-up,
    clickhouse,
  ]
milestone: 2
links:
  - docs/architecture/technical-design-general-overview.md
  - docs/architecture/infrastructure/infrastructure-overview.md
  - lore/2-adrs/0047_clickhouse-primary-api-datastore.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from task 0242 future work. ADR 0047 ratified CH-on-Hetzner as
      primary API datastore but full sweep of pre-pivot RDS prose was deferred
      from 0242 (small effort budget). This task carries the docs-architecture
      cleanup obligation per ADR 0032 for the sections 0242 did not touch.
  - date: '2026-07-22'
    status: active
    who: karolkow
    note: >
      Promoted after verifying the premise still held: 36 `RDS` hits remained in
      `technical-design-general-overview.md`, 6 in `infrastructure-overview.md`.
      **The premise verification changed the task.** Both documents carried a
      note deferring this sweep until "the RDS teardown lands (task 0239)", and
      classification bucket (a) told the sweeper to retain RDS prose as an
      "accurate description of the M1 transition state". Both are false:
      task 0239 is COMPLETED and its Phase 6 decommission was satisfied
      **vacuously** — per the 0249 archive, **zero production stacks were ever
      deployed** (`validateConfig` blocked on `hostedZoneId: "CHANGE_ME"`), so
      no production RDS existed to tear down. Only a *staging* RDS ever ran, and
      it was destroyed on 2026-05-21. The RDS/bastion CDK stacks are in
      `.trash/`.
      So the prose does not describe a frozen deployment — it describes a design
      that was never built. Bucket (a) is empty; every hit is (b) rewrite or
      (c) mark-as-superseded. Recorded in both documents' status notes.
  - date: '2026-07-23'
    status: completed
    who: karolkow
    note: >
      **Done.** Three docs rewritten to the ClickHouse-on-Hetzner reality that
      actually runs: `technical-design-general-overview.md` (36 RDS hits — every
      component/env/scaling/cost/security section, four ASCII diagrams redrawn
      including the post-0239 out-of-VPC Lambda + public-subnet Galexie topology),
      `infrastructure-overview.md` (6 hits + the "still skeletal" / phantom
      `postgres` service / 17-vs-28-table corrections), and `clickhouse-pilot.md`
      (HISTORICAL banner — it is the reasoning behind ADR 0047, not a description
      of a parallel store). Verified `database-schema-overview.md`,
      `backend-overview.md`, `endpoint-queries-clickhouse/README.md` needed
      nothing — their Postgres mentions are already correctly framed as retired
      (0244 did that sweep).
      Three non-RDS corrections surfaced and were fixed in passing, each one a
      claim that was actively misleading, not just stale: the "single DB
      transaction (ADR 0027)" line (CH has no cross-table ACID; ADR 0027 is
      superseded; the real mechanism is the `ledgers` row flushed last as a
      commit marker), the "CI/CD deploys via cdk / staging-production parity"
      claim (opposite of `docs/deployment.md` — no staging, no CI deploy, manual
      from a laptop), and the pilot's read-empty framing. All six acceptance
      criteria resolved.
---

# Comprehensive RDS prose sweep in tech design + infrastructure docs

## Summary

ADR 0047 (CH-on-Hetzner = primary API datastore) ratified 2026-05-20. Task 0242
updated D1 acceptance criteria + §7.4 Deliverable 1 prose but left pre-pivot
RDS-centric prose untouched in §6 (Architecture), §7.3 (Scaling Model), and
the infrastructure-overview document. This task does the comprehensive sweep.

## Context

`docs/architecture/technical-design-general-overview.md` and
`docs/architecture/infrastructure/infrastructure-overview.md` were written
pre-pivot when RDS PostgreSQL was the prod datastore. Post-ADR 0047 + ADR 0044
ratification, these docs contain stale prose. Examples surfaced during 0242:

- `technical-design-general-overview.md:289` — VPC topology diagram shows
  RDS PostgreSQL as core component
- `:317` — "explorer's own RDS for every partition-pruned read"
- `:447` — "Database connection pooling — RDS Proxy manages connection pools"
- `:476`, `:521` — architecture diagrams with RDS PostgreSQL boxes
- `:497-498` — "Lambda Ledger Processor → RDS (write) / Lambda Rust/axum API → RDS (read)"
- `:540-544` — Components table with RDS PostgreSQL as sole production data store
- `:580` — "Database | RDS PostgreSQL 16"
- `:593-594` — Staging/Production environments described in terms of RDS
- `:601-602` — RDS backups + KMS + TLS baselines
- `:611-624` — PostgreSQL + RDS scaling/alerting table
- `:698` — Diagram with "RDS PostgreSQL (block explorer schema — Section 6)"
- `:729` — "writes directly to the target RDS"
- `:745` — Components — "writes all chain data to RDS"
- `:802` — "is written to RDS"
- `:1314-1315` — Cost table — "RDS PostgreSQL" + "RDS Storage"
- `:1331-1333` — Scaling expansions — "Add RDS read replica", "Enable RDS Multi-AZ"
- `:1397` — D3 security AC — "no public RDS endpoint"
- `:1411-1412` — D3 security AC — "RDS has no public endpoint, production RDS
  backups/PITR/deletion protection enabled, RDS and S3 encrypted"

`docs/architecture/infrastructure/infrastructure-overview.md` likely has
similar pre-pivot prose (not exhaustively grep'd; sweep here).

## Implementation Plan

### Step 1: Grep audit

```bash
grep -n "RDS" docs/architecture/technical-design-general-overview.md
grep -n "RDS" docs/architecture/infrastructure/infrastructure-overview.md
grep -n "RDS" docs/architecture/database-schema/database-schema-overview.md
grep -n "RDS" docs/architecture/backend/backend-overview.md
grep -n "Postgres\|postgres" docs/architecture/**/*.md
```

Catalogue all hits. Classify each:

- (a) **Retain unchanged** — historical / cost-context refs (e.g. "RDS would
  have cost ~$175/month" in cost analysis), or refs to pre-pivot baseline that
  remain accurate descriptions of the M1 transition state.
- (b) **Rewrite** — prose describing the prod runtime that no longer matches
  the CH-on-Hetzner topology.
- (c) **Replace with link to ADR 0047** — sections where a paragraph-level
  rewrite is too invasive, but a "see ADR 0047 for the post-pivot
  architecture" marker is sufficient.

### Step 2: Rewrite prose

For each (b) hit, write CH-on-Hetzner equivalent. Reference patterns:

- "RDS PostgreSQL" → "ClickHouse on Hetzner (`ch-prod-01`)"
- "RDS Proxy" → "Hetzner CH connection pool (clickhouse::Client at Lambda cold-start)"
- "PostgreSQL connection pooling" → "ClickHouse HTTP client (cheap-to-clone, single instance per Lambda)"
- "Multi-AZ RDS" → "single-node Hetzner with Borg backup → BX21 (read replica deferred per ADR 0047)"
- "RDS Storage 1 TB gp3" → "Hetzner 2× 1.92 TB NVMe RAID 1 (md1 ~1.7 TB usable)"
- Cost table — replace AWS RDS line item with Hetzner Server Auction line item
- Architecture diagrams — update ASCII art / Mermaid blocks

### Step 3: Update infrastructure-overview.md

Same treatment for `infrastructure/infrastructure-overview.md`. Likely
overlaps with 0239 docs-update obligation (Phase 6 RDS decommission docs).
Coordinate: if 0239 already touches the same section, defer to 0239 PR;
otherwise this task carries the update.

### Step 4: Cross-link ADR 0047

Where significant edits land, add inline link:
`per [ADR 0047](../../lore/2-adrs/0047_clickhouse-primary-api-datastore.md)`.

### Step 5: Removed pieces

Some sections may not need rewriting but **removal** (no longer applicable
post-pivot):

- "Add RDS read replica" expansion row in §7.3 scaling table — replace with
  CH read scaling guidance (multi-node MergeTree if SLA demands)
- "RDS Multi-AZ" — N/A for Hetzner single-node; replace with backup-restore
  RPO/RTO guidance

## What the sweep found beyond RDS naming

The RDS substitutions were mechanical. Three claims found alongside them were
**wrong about how the system works**, not merely stale in vocabulary — these are
the findings worth reading:

1. **"Commit the whole 14-step `persist_ledger` in a single DB transaction
   (ADR 0027)."** ClickHouse has no cross-table ACID, and ADR 0027 is
   `status: superseded`. What actually happens (`persist/writer.rs:74-78`) is an
   ordering discipline: the `ledgers` row is buffered in RAM and flushed **last**,
   after every other insert has ack'd, so it serves as a commit marker and a
   partial write is re-done by the reconcile rather than half-committed. Corrected
   in both documents — this one would have misled anyone reasoning about crash
   behaviour.

2. **"Infrastructure rollout is part of the product delivery model, not a
   manual-only operations process."** The exact opposite of
   [`docs/deployment.md`](../../../docs/deployment.md), which opens by stating
   production is the only environment and every deploy is run manually from an
   operator laptop. Corrected, with the pointer added.

3. **`clickhouse-pilot.md` still declared itself a read-empty pilot standing next
   to a Postgres production store.** Present tense, entirely false. Given a
   HISTORICAL banner rather than a rewrite — its measurements are the reasoning
   behind ADR 0047 and worth keeping, its framing is not.

Smaller corrections in the same class: `infrastructure-overview.md` claimed the
repo was "still skeletal" (it is in production), listed a `postgres` service that
does not exist in `docker-compose.yml`, cited "17 tables + 1 Dictionary" against an
actual 28 tables / 3 materialized views / 1 Dictionary, and linked task 0204 in
`active/` when it is archived.

## Acceptance Criteria

- [x] All RDS references in `technical-design-general-overview.md` classified and
      either rewritten or marked superseded. **Bucket (a) "retain as historical"
      turned out to be empty** — see the history note. Remaining `RDS` hits are
      only in the corrective notes themselves.
- [x] Same for `infrastructure-overview.md`. Not coordinated with 0239 — 0239 is
      completed and closed vacuously, which is the finding.
- [x] Cost table updated: Hetzner Server Auction line replaces the RDS rows,
      sourced to ADR 0047 (~€60/mo). NAT Gateway row dropped too — 0239 removed it
      when Lambdas left the VPC. **No new total quoted**: the surviving AWS figures
      are un-verified pre-pivot estimates and are now labelled as such.
- [x] Scaling table updated: read-replica / Multi-AZ rows replaced with vertical-
      first + replicated-second-node guidance.
- [x] Architecture diagrams updated — four ASCII blocks. The VPC diagram was
      redrawn, not relabelled: Lambdas are **outside** the VPC and Galexie is in a
      **public** subnet post-0239, so swapping the database box alone would have
      left the topology wrong.
- [x] D3 security ACs updated: mTLS/Caddy CN allow-list replaces "no public RDS
      endpoint". **The lost properties are named rather than dropped** — no PITR
      (weekly Borg, RPO up to 7 days) and no deletion protection, both being
      properties of a managed service never adopted.
- [x] **Docs updated** — task IS the docs update. Four files:
      `technical-design-general-overview.md`, `infrastructure-overview.md`,
      `clickhouse-pilot.md`, plus verification that `database-schema-overview.md`,
      `backend-overview.md` and `endpoint-queries-clickhouse/README.md` needed
      nothing (their Postgres mentions are correctly framed as retired — 0244 did
      that sweep).
- [x] **API types regenerated** — N/A — task does not touch `crates/api/**`.

## Dependencies

- 0242 (ADR 0047 ratification) — must land first
- 0239 docs-update — coordinate to avoid double-edit on infrastructure-overview.md

## Notes

- Effort estimated medium because tech design + infrastructure docs are ~1000+
  lines each with substantial pre-pivot prose. Realistic 1-2 days focused work.
- Lower priority than active M1/M2 code work — docs cleanup is important but
  not blocking any milestone closure. Schedule when team has bandwidth.
- This is a docs-only task; no code changes, no API contract changes.
