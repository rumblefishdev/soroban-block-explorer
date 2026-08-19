---
id: '0507'
title: 'FEATURE: a numbered schema migration ladder — `init.sql` is currently the migration'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0455']
tags: ['architecture', 'clickhouse', 'schema', 'effort-medium', 'priority-high']
links: []
history:
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0455 review sweep (finding 16). A whole-repo
      architecture audit rated this one of two items to do first regardless of
      everything else. Verified 2026-08-19: there is no `migrations/` directory
      anywhere in the repository; `crates/db-clickhouse/schema/init.sql` is the
      only DDL artifact.
---

# FEATURE: a numbered schema migration ladder

## Summary

The database schema has no versioned migration path. `init.sql` describes the
schema as it should be **now**; every change to a live table is an `ALTER`
issued by hand, recorded (if at all) in a task journal. There is no way to ask
"which schema version is this database on", no way to replay a schema from
empty to current, and no gate that a deployed binary and the table it writes to
agree.

## Context

Measured 2026-08-19: no `migrations/` or `migration/` directory exists in the
repository. The single DDL artifact is `crates/db-clickhouse/schema/init.sql`.

The cost is already documented elsewhere in this repo rather than hypothetical:

- A struct that dropped a field the table still carried failed **client-side**
  at insert time, and the fix required a deploy, an `ALTER` and a container
  recycle as one coordinated window.
- Several tasks carry `ALTER` statements in their prose as the record of a
  schema change. Prose is not replayable.
- `docs/backups.md` records that a restore does not re-deliver the rolled-back
  range. A restore also cannot tell whether the restored schema matches the
  running binaries.

The stated binding constraint from the audit: with the schema growing and no
ladder, no single person can safely add an entity.

## Implementation

Shape is open — that is part of the task. The decision to make first is whether
the ladder is:

1. **A numbered SQL directory** applied by a small runner that records applied
   versions in a table. Simplest; matches how `init.sql` already reads.
2. **An existing migration tool** that supports ClickHouse. Less code, one more
   dependency, and the tool must handle ClickHouse DDL semantics
   (`ON CLUSTER`, `EXCHANGE TABLES`, RMT specifics) rather than assume Postgres.
3. **Schema-as-code generated from the Rust structs.** Attractive because the
   client-side insert failure above came from struct/table drift — but it
   inverts the source of truth and needs an escape hatch for anything the
   structs cannot express.

Whichever is chosen, three properties are non-negotiable:

- `init.sql` stops being the source of truth, or becomes a generated artifact.
- A database can state which version it is on.
- Applying the ladder from empty reproduces production's schema exactly, and
  that equality is checked, not assumed.

## Acceptance Criteria

- [ ] Ladder mechanism decided and recorded as an ADR
- [ ] Every table currently in `init.sql` is reachable by replaying the ladder
      from empty, verified by diffing the replayed schema against production's
      `SHOW CREATE TABLE` output for every table
- [ ] The applied version is queryable from the database itself
- [ ] `docs/backups.md` and the deployment guide say how a restore establishes
      schema version
- [ ] **Docs updated** — `docs/architecture/database-schema/**` describes the
      ladder, not `init.sql`, as the schema of record
- [ ] **API types regenerated** — N/A unless the work touches `crates/api/**`
