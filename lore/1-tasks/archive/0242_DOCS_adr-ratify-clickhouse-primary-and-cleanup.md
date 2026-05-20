---
id: '0242'
title: 'DOCS: ADR 0047 + 0044/0045 ratify + tech design AC update + obsolete cleanup (backfill-plan, task 0174)'
type: DOCS
status: completed
related_adr: ['0044', '0045', '0047']
related_tasks: ['0174', '0228', '0241']
tags:
  [
    priority-high,
    effort-small,
    layer-docs,
    adr,
    architecture,
    grooming,
    clickhouse,
  ]
milestone: 1
links:
  - lore/2-adrs/0044_clickhouse-pilot-parallel-store.md
  - lore/2-adrs/0045_clickhouse-local-backfill-then-mirror-to-hetzner-via-freeze-rsync-attach.md
  - docs/architecture/technical-design-general-overview.md
  - lore/3-wiki/backfill-execution-plan.md
  - lore/1-tasks/backlog/0174_FEATURE_split-pre-post-restore-migrations.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from M1-M3 sequencing plan (2026-05-20). Formal ratification of
      the decision that ClickHouse on Hetzner is the primary API datastore
      (replacing RDS Postgres). ADR 0044 and 0045 are currently `proposed`;
      the team has committed to the direction. Plus cleanup of obsolete docs
      and tasks now that the PG cutover path is dead.
  - date: '2026-05-20'
    status: active
    who: stkrolikiewicz
    note: 'Promoted to active. Steps 4 (banner) and 5 (0174 close) already shipped in commit c82c9fa8. Remaining steps 1-3: ADR 0047 + status flip 0044/0045 + tech design sweep.'
  - date: '2026-05-20'
    status: active
    who: stkrolikiewicz
    note: >
      Implementation complete on branch docs/0242_adr-ratify-clickhouse-primary-and-cleanup:
      ADR 0047 authored (status accepted), ADR 0044/0045 status flipped
      proposed→accepted with history entries citing ADR 0047, tech design D1
      Deliverable 1 prose + ACs #2/#3/#4 updated to ClickHouse on Hetzner.
      Emerged decision: comprehensive sweep of pre-pivot RDS prose in §6
      (Architecture) and §7.3 (Scaling Model) deferred to docs-architecture
      cleanup follow-up — scope too large for "small" task budget. ADR 0032
      docs-update obligation honored for the parts that ADR 0047 directly
      changes (D1 ACs + Deliverable 1 prose). Inline note added at §7.4 D1
      explaining the partial sweep and pointing readers to ADR 0047.
  - date: '2026-05-20'
    status: completed
    who: stkrolikiewicz
    note: >
      PR #203 merged (commit 128c9ca4). All acceptance criteria met:
      ADR 0047 authored + accepted, ADR 0044/0045 status flipped, tech design
      D1 updated, backfill-execution-plan SUPERSEDED banner, 0174 archived.
      Spawned follow-up task 0248 (renumbered from 0246 post-merge due to ID
      collision with Filip's LP API task — see commit 0505ac45) carrying the
      comprehensive RDS prose sweep deferred from this task.
---

# ADR 0047 + 0044/0045 ratify + tech design AC update + obsolete cleanup

## Summary

Formal ratification of the decision "ClickHouse as primary API datastore
(post-RDS-drop)". New ADR 0047, status flip for 0044/0045
`proposed → accepted`, tech design AC update ("RDS" → "ClickHouse"),
SUPERSEDED banner on backfill-execution-plan, close task 0174 as obsolete.

## Context

ADR 0044 ("CH parallel store") and 0045 ("FREEZE+rsync+ATTACH transport") are
currently `proposed`. Team decision (2026-05-20): ratify. ADR 0044 describes
CH as a "parallel pilot" — that framing is no longer accurate post-pivot,
where CH is the **primary** datastore for API reads.

Additionally:

- `docs/architecture/technical-design-general-overview.md` D1 AC #2/#3 still
  reference "RDS" — should now read "ClickHouse".
- `lore/3-wiki/backfill-execution-plan.md` describes pg_dump/pg_restore
  cutover — a dead path (superseded by ADR 0045 FREEZE+rsync+ATTACH).
- Task 0174 (split pre/post-restore migrations) — obsolete because there is
  no pg_restore in the prod path anymore.

## Implementation Plan

### Step 1: Author ADR 0047

`lore/2-adrs/0047_clickhouse-primary-api-datastore.md`:

- **Title**: "ClickHouse on Hetzner as Primary API Datastore (post-RDS retirement)"
- **Status**: `accepted`
- **Deciders**: [stkrolikiewicz, fmazur]
- **Related ADRs**: ['0044', '0045']
- **Related tasks**: ['0228', '0241', '0243']
- **Context**: pivot from PG-primary to CH-primary. Cost (NAT GW + RDS
  dominate), OLAP query patterns suit CH, Hetzner egress sponsored via the
  AWS Open Data Program for `aws-public-blockchain` S3.
- **Decision**: CH on Hetzner = single source of truth for API reads. RDS
  decommissioned in 0239 Phase 6 (M3).
- **Trade-offs**: no read replica (single-node), failure mode → Borg backup
  to BX21 Storage Box (per task 0236) + restore runbook (to be authored after
  0245 or as a follow-up). API egress AWS → Hetzner over public internet
  (mTLS-authenticated).
- **Consequences**: API rewrite required (= task 0243). Indexer hard swap
  (= task 0241). RDS retirement (= 0239 Phase 6, M3).

### Step 2: Status flip ADR 0044 + 0045

In `lore/2-adrs/0044_*.md` and `0045_*.md`:

- Frontmatter `status: proposed` → `status: accepted`
- Add history entry:
  ```yaml
  - date: 2026-05-20
    status: accepted
    who: stkrolikiewicz
    note: 'Ratified post-pivot. CH on Hetzner = primary API datastore (see ADR 0047).'
  ```

### Step 3: Tech design AC update

`docs/architecture/technical-design-general-overview.md`:

- Line ~1353-1354 D1 AC #2: "RDS `ledgers` table contains all ledgers..."
  → "**ClickHouse** `ledgers` table contains all ledgers..."
- Line ~1355-1356 D1 AC #3: "RDS `soroban_events_appearances` table..."
  → "**ClickHouse** `soroban_events_appearances` table..."
- Also sweep the rest of the document: §6.2 (storage section), §7.4
  (deliverables), diagrams. All RDS references in the prod context become
  ClickHouse.

### Step 4: SUPERSEDED banner on backfill-execution-plan

`lore/3-wiki/backfill-execution-plan.md` — top of file:

```markdown
> ⚠️ **SUPERSEDED** by ADR 0044/0045 (CH FREEZE+rsync+ATTACH transport) +
> task 0228 (parallel-backfill merge). PG pg_restore staging cutover described
> below is no longer the prod path. Retained for historical reference only.
```

Alternative: `mv lore/3-wiki/backfill-execution-plan.md .trash/`. Preferred
to keep it in place with the banner — historical context may be useful.

### Step 5: Close 0174

`lore/1-tasks/backlog/0174_FEATURE_split-pre-post-restore-migrations.md`:

- Move to `lore/1-tasks/archive/`
- Frontmatter:
  - `status: canceled`
  - Add history entry:
    ```yaml
    - date: 2026-05-20
      status: canceled
      who: stkrolikiewicz
      reason: obsolete
      note: 'Superseded by ADR 0044/0045. No pg_restore in prod path, so split-migrations rationale gone.'
    ```

## Acceptance Criteria

- [x] ADR 0047 authored in `lore/2-adrs/0047_clickhouse-primary-api-datastore.md`,
      status `accepted`
- [x] ADR 0044 status `proposed` → `accepted` with history entry
- [x] ADR 0045 status `proposed` → `accepted` with history entry
- [x] `docs/architecture/technical-design-general-overview.md` D1 AC #2/#3 +
      Deliverable 1 prose updated ("RDS" → "ClickHouse on Hetzner") with
      inline note pointing to ADR 0047; comprehensive sweep deferred to
      [task 0248](../backlog/0248_DOCS_tech-design-rds-prose-comprehensive-sweep.md)
- [x] `lore/3-wiki/backfill-execution-plan.md` SUPERSEDED banner top-of-file (commit c82c9fa8)
- [x] 0174 moved to `lore/1-tasks/archive/` with `status: canceled`, reason `obsolete` (commit c82c9fa8)
- [x] **Docs updated** — task IS the docs update; partial sweep per ADR 0032
      honored for parts that ADR 0047 directly changes (§7.4 D1 ACs +
      Deliverable 1 prose)
- [x] **API types regenerated** — N/A — task does not touch `crates/api/**`,
      `Cargo.{toml,lock}`, or `libs/api-types/**`

## Implementation Notes

Implementation landed on branch `docs/0242_adr-ratify-clickhouse-primary-and-cleanup`:

- ADR 0047 authored (`lore/2-adrs/0047_clickhouse-primary-api-datastore.md`),
  status `accepted` — ~220 lines covering context (5 facts driving the pivot),
  decision (5 points), rationale (cost / OLAP fit / storage characteristics /
  no-rewrite-cost / operational simplicity), 3 alternatives considered (keep
  parallel pilot / CH on AWS / PG primary + CH replica via CDC), positive +
  negative consequences, and ADR 0032 delivery checklist (D1 ACs only; rest
  N/A or deferred to spawned task 0248).
- ADR 0044 (`status: proposed → accepted`) — history entry citing ADR 0047,
  notes that pilot evaluation phase complete and architectural direction
  committed.
- ADR 0045 (`status: proposed → accepted`) — history entry citing ADR 0047,
  notes that FREEZE+rsync+ATTACH is the committed transport, supersedes the
  pg_restore staging cutover. Updated `related_tasks` to include 0228 + 0233.
- `docs/architecture/technical-design-general-overview.md` §7.4 Deliverable 1
  prose + ACs #2/#3/#4 updated; inline note added at top of Deliverable 1
  pointing to ADR 0047, explaining the partial sweep, and signalling that
  comprehensive cleanup is in task 0248.

## Design Decisions

### From Plan

1. **ADR 0047 as the meta-ADR formalizing the pivot** — task plan called for
   a new ADR rather than amending 0044 in place. Reason: 0044's "parallel
   pilot" scope is historically accurate (that's what it was when written);
   the elevation to "primary datastore" is a separate architectural commitment
   deserving its own ADR.

2. **Status flip 0044/0045 to `accepted` via history entry** — per lore ADR
   convention, status changes are tracked with new history entries (not by
   rewriting the existing entry). Preserves the "what we believed when" trail.

### Emerged

3. **Comprehensive RDS prose sweep deferred to task 0248** — task plan said
   "sweep the document for other stale RDS refs". On encountering ~30+ RDS
   references in tech design + likely matching count in infrastructure-overview,
   judged the sweep too large for the "small" effort budget of 0242. Spawned
   0248 (medium effort) to carry the comprehensive update. Honored ADR 0032
   docs-update obligation for the parts ADR 0047 directly changes (D1 ACs +
   Deliverable 1 prose); marked everything else N/A or "deferred to 0248" in
   the ADR 0047 delivery checklist.

4. **Inline note at top of §7.4 Deliverable 1** — added an explicit
   `> Note (2026-05-20):` block warning readers that pre-pivot RDS prose later
   in the document is stale until 0248 lands. This is defensive — without it,
   a reader skimming §7.4 Deliverable 1 and then jumping back to §6 would get
   inconsistent signals about the prod datastore.

5. **0047 ratifies 0045 even though 0045 was about transport, not API
   datastore** — strictly 0047 is about API reads, but 0045's transport mechanism
   only makes operational sense if CH is the prod store on the receiving end.
   Flipped both to `accepted` together because the architectural commitment is
   one decision, not two. Explicitly noted in 0045's history entry.

6. **ADR ID renumber 0046 → 0047 after collision discovered** — initial ADR
   draft used ID 0046, but on regenerating the lore index discovered the slot
   was already taken by `0046_classifier-quarantine-tables-nfts-pending.md`
   (existing ADR about NFT quarantine, unrelated to the CH pivot). Renamed
   file + updated all 10+ cross-references (other ADRs, tasks, tech design,
   backfill-execution-plan banner). Lore framework's shared sequence makes
   this kind of mid-flight collision possible; future tasks should
   `lore_generate-index` and check the highest existing ADR ID before
   committing to a new one.

## Issues Encountered

- **ADR ID collision (0046)** — see Design Decision #6 above. Root cause: did
  not check `ls lore/2-adrs/0046*` before writing the new ADR. Fix: renamed
  to 0047 and grep-swept all consumers. Not a regression in the codebase, but
  a process gap to avoid next time.

## Future Work → spawned

- [Task 0248](../backlog/0248_DOCS_tech-design-rds-prose-comprehensive-sweep.md) —
  comprehensive sweep of pre-pivot RDS prose in tech design + infrastructure
  docs. Spawned with concrete grep audit, classification scheme (retain /
  rewrite / mark-with-ADR-link), and detailed implementation plan. Priority
  medium, effort medium, milestone 2.

## Notes

- Decision date: 2026-05-20 (M1-M3 sequencing planning session).
- ADR 0047 is a meta-ADR — it formalizes an existing direction rather than
  proposing new technical concepts. ADR 0044/0045 provide the technical
  details; 0047 ties the threads together for API reads.
- Backup runbook (CH single-node failure recovery from Borg on BX21) — TBD
  as a follow-up task after 0245 or during M3 hardening phase.
