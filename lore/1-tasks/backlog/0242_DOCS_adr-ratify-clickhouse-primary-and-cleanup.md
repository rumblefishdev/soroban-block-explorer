---
id: '0242'
title: 'DOCS: ADR 0046 + 0044/0045 ratify + tech design AC update + obsolete cleanup (backfill-plan, task 0174)'
type: DOCS
status: backlog
related_adr: ['0044', '0045']
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
      Spawned z M1-M3 sequencing planu (2026-05-20). Formalna ratyfikacja decyzji
      że ClickHouse na Hetznerze = primary API datastore (zamiast RDS Postgres).
      ADR 0044 i 0045 są obecnie status `proposed`; team zatwierdził kierunek.
      Plus cleanup obsolete docs/tasks po PG-cutover-path become martwy.
---

# ADR 0046 + 0044/0045 ratify + tech design AC update + obsolete cleanup

## Summary

Formalna ratyfikacja decyzji "ClickHouse jako primary API datastore (post-RDS-drop)".
Nowy ADR 0046, status flip dla 0044/0045 `proposed → accepted`, update tech design
ACs ("RDS" → "ClickHouse"), banner SUPERSEDED na backfill-execution-plan, close
task 0174 jako obsolete.

## Context

ADR 0044 ("CH parallel store") + 0045 ("FREEZE+rsync+ATTACH transport") są obecnie
`proposed`. Team decision (2026-05-20): ratyfikować. ADR 0044 opisuje CH jako
"parallel pilot" — to nieadekwatne post-pivot, gdzie CH jest **primary** dla API
reads.

Dodatkowo:

- `docs/architecture/technical-design-general-overview.md` D1 AC #2/#3 odnoszą się
  do "RDS" — to teraz "ClickHouse".
- `lore/3-wiki/backfill-execution-plan.md` opisuje pg_dump/pg_restore cutover —
  to martwy szlak (superseded przez ADR 0045 FREEZE+rsync+ATTACH).
- Task 0174 (split pre/post-restore migrations) — obsolete bo nie ma już
  pg_restore w prod path.

## Implementation Plan

### Step 1: Napisać ADR 0046

`lore/2-adrs/0046_clickhouse-primary-api-datastore.md`:

- **Title**: "ClickHouse on Hetzner as Primary API Datastore (post-RDS retirement)"
- **Status**: `accepted`
- **Deciders**: [stkrolikiewicz, fmazur]
- **Related ADRs**: ['0044', '0045']
- **Related tasks**: ['0228', '0241', '0243']
- **Context**: pivot od PG-primary do CH-primary. Cost (NAT GW + RDS dominują),
  OLAP query patterns suit CH, Hetzner egress sponsored przez AWS Open Data
  Program dla `aws-public-blockchain` S3.
- **Decision**: CH na Hetznerze = single source of truth dla API reads. RDS
  decommissioned w Phase 6 0239 (M3).
- **Trade-offs**: no read replica (single-node), failure mode → Borg backup na
  BX21 Storage Box (per 0236) + restore runbook (do napisania po 0245 lub w
  follow-up). API egress AWS → Hetzner over public internet (mTLS-auth).
- **Consequences**: API rewrite required (= task 0243). Indexer hard swap (= 0241).
  RDS retirement (= 0239 Phase 6, M3).

### Step 2: Status flip ADR 0044 + 0045

W `lore/2-adrs/0044_*.md` i `0045_*.md`:

- Frontmatter `status: proposed` → `status: accepted`
- Dodać history entry:
  ```yaml
  - date: 2026-05-20
    status: accepted
    who: stkrolikiewicz
    note: 'Ratified post-pivot. CH on Hetzner = primary API datastore (see ADR 0046).'
  ```

### Step 3: Tech design AC update

`docs/architecture/technical-design-general-overview.md`:

- Linia ~1353-1354 D1 AC #2: "RDS `ledgers` table contains all ledgers..."
  → "**ClickHouse** `ledgers` table contains all ledgers..."
- Linia ~1355-1356 D1 AC #3: "RDS `soroban_events_appearances` table..."
  → "**ClickHouse** `soroban_events_appearances` table..."
- Sprawdzić też reszta tech designu: §6.2 (storage section), §7.4 (deliverables),
  diagrammy. Wszelkie wzmianki RDS w prod context → ClickHouse.

### Step 4: Banner SUPERSEDED na backfill-execution-plan

`lore/3-wiki/backfill-execution-plan.md` — top of file:

```markdown
> ⚠️ **SUPERSEDED** by ADR 0044/0045 (CH FREEZE+rsync+ATTACH transport) +
> task 0228 (parallel-backfill merge). PG pg_restore staging cutover described
> below is no longer the prod path. Retained for historical reference only.
```

Alternative: `mv lore/3-wiki/backfill-execution-plan.md .trash/`. Sugerowane
zostawić z banner (historical context może być przydatny).

### Step 5: Close 0174

`lore/1-tasks/backlog/0174_FEATURE_split-pre-post-restore-migrations.md`:

- Move to `lore/1-tasks/archive/`
- Frontmatter:
  - `status: canceled`
  - Dodać history entry:
    ```yaml
    - date: 2026-05-20
      status: canceled
      who: stkrolikiewicz
      reason: obsolete
      note: 'Superseded by ADR 0044/0045. No pg_restore in prod path, so split-migrations rationale gone.'
    ```

## Acceptance Criteria

- [ ] ADR 0046 napisany w `lore/2-adrs/0046_clickhouse-primary-api-datastore.md`,
      status `accepted`
- [ ] ADR 0044 status `proposed` → `accepted` z history entry
- [ ] ADR 0045 status `proposed` → `accepted` z history entry
- [ ] `docs/architecture/technical-design-general-overview.md` D1 AC #2/#3 update
      ("RDS" → "ClickHouse"); sweep dokumentu dla innych stale RDS refs
- [ ] `lore/3-wiki/backfill-execution-plan.md` SUPERSEDED banner top-of-file
- [ ] 0174 moved to `lore/1-tasks/archive/` z `status: canceled`, reason `obsolete`
- [ ] **Docs updated** — task obejmuje docs update (sam task = docs)
- [ ] **API types regenerated** — N/A — task does not touch `crates/api/**` ani
      `Cargo.{toml,lock}` ani `libs/api-types/**`

## Notes

- Decision date: 2026-05-20 (M1-M3 sequencing planning session).
- ADR 0046 jest meta-ADR — formalizuje już istniejący kierunek, nie wymyśla nowych
  konceptów. ADR 0044/0045 dostarczają technical details; 0046 łączy kropki dla
  API reads.
- Backup runbook (CH single-node failure recovery z Borg na BX21) — TBD jako
  follow-up task po 0245 lub w M3 hardening phase.
