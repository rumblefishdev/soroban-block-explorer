---
id: '0246'
title: 'DOCS: comprehensive sweep of pre-pivot RDS prose in tech design + infrastructure docs (post ADR 0047)'
type: DOCS
status: backlog
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

## Acceptance Criteria

- [ ] All RDS references in `technical-design-general-overview.md` classified
      and either retained-as-historical, rewritten, or marked with ADR 0047 link
- [ ] Same for `infrastructure-overview.md` (coordinated with 0239)
- [ ] Cost table updated: Hetzner Server Auction line item replaces RDS rows
- [ ] Scaling table updated: read replica + Multi-AZ rows replaced with CH
      multi-node guidance
- [ ] Architecture diagrams (ASCII / Mermaid) updated to show
      Hetzner CH instead of RDS PostgreSQL
- [ ] D3 security ACs updated: "no public RDS endpoint" → N/A post-decom;
      replace with "mTLS gate on Hetzner CH, no public DB endpoint"
- [ ] **Docs updated** — task IS the docs update
- [ ] **API types regenerated** — N/A — task does not touch `crates/api/**`

## Dependencies

- 0242 (ADR 0047 ratification) — must land first
- 0239 docs-update — coordinate to avoid double-edit on infrastructure-overview.md

## Notes

- Effort estimated medium because tech design + infrastructure docs are ~1000+
  lines each with substantial pre-pivot prose. Realistic 1-2 days focused work.
- Lower priority than active M1/M2 code work — docs cleanup is important but
  not blocking any milestone closure. Schedule when team has bandwidth.
- This is a docs-only task; no code changes, no API contract changes.
