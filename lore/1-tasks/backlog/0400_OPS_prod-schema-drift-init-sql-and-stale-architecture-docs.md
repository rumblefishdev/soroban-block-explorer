---
id: '0400'
title: 'OPS: prod-only CH schema objects missing from init.sql + architecture docs describe a retired Postgres world'
type: OPS
status: backlog
related_adr: ['0032']
related_tasks: ['0357', '0356', '0281']
tags: [priority-medium, effort-medium, layer-clickhouse, phase-post-launch]
links:
  - crates/db-clickhouse/schema/init.sql
  - docs/architecture/database-schema/clickhouse-pilot.md
  - docs/architecture/database-schema/database-schema-overview.md
history:
  - date: 2026-07-17
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0357's 2026-07-17 load-test series. The `closed_at_mm` minmax
      index on `ledgers` — worth 27.5bn fewer rows read per 12-min run at 50M/mo,
      69% of the whole series-2 win — existed ONLY on the prod box, applied by an
      online ALTER, absent from `init.sql`. A box rebuilt from code would have
      silently lost it. Fixed in-band for this one index (0357's PR adds it to
      `init.sql` with the `idx_acc_id` comment convention); this task owns the
      recurring class + the docs gate that should have caught it.
---

# OPS: prod-only CH schema objects missing from init.sql + stale architecture docs

## Summary

ClickHouse schema objects get applied to the prod box via online `ALTER` and then
never land in `crates/db-clickhouse/schema/init.sql`. A box provisioned from code
therefore does NOT match prod. This is not hypothetical: the `closed_at_mm` minmax
index was carrying **69% of the 2026-07-17 performance win** while living nowhere
but on the running server.

The ADR 0032 "docs updated" gate that should catch this cannot currently be
satisfied honestly, because the schema pages under `docs/architecture/**` still
describe **Postgres as the production source of truth** and ClickHouse as a
**"read-empty pilot"** — a world that has not existed for months.

## Context

Two known instances of the same class, both found while working on something else:

| object                                                                | where it lives                              | how it was found                                                   |
| --------------------------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------ |
| `INDEX closed_at_mm closed_at TYPE minmax GRANULARITY 4` on `ledgers` | prod only → **now in `init.sql`** (0357 PR) | 0357 load test: `lpchart` 77.9M → 26.3M read_rows/req, −27.5bn/run |
| `oa_pool_seek` projection on `operations_appearances`                 | prod only, still not in `init.sql`          | 0281 window                                                        |

Both were applied deliberately and both are correct — the defect is that code and
prod diverged with nothing to detect it.

The docs half is worse than stale, it is actively misleading:

- `database-schema-overview.md` — _"The Postgres schema described there is the
  production source of truth"_. Postgres (RDS) is retired; its `CREATE INDEX
idx_ledgers_closed_at ON ledgers (closed_at DESC)` is Postgres syntax for a
  store that no longer exists.
- `clickhouse-pilot.md` — _"parallel ClickHouse store … **not** as a replacement"_,
  _"Status: read-empty pilot"_, _"indexer dual-write and API reads are deliberately
  deferred"_. All three statements are false today.

Consequence: a schema PR that dutifully follows ADR 0032 has nowhere truthful to
write. That is why an index worth 27.5bn rows/run slipped through with no doc
change and no reviewer catch.

## Implementation

Three separable pieces — the first is cheap and stops the bleeding.

### 1. Reconcile prod → `init.sql` (small)

Diff every `SHOW CREATE TABLE` on prod against `init.sql` and close the gaps.
`closed_at_mm` is already done; `oa_pool_seek` is the known remainder. Do not
assume these two are the only ones — enumerate, don't guess:

```bash
# per table: compare deployed DDL against the file
ssh deploy@<box> "docker exec -i app-clickhouse-1 clickhouse-client \
  --query \"SELECT name FROM system.tables WHERE database='default'\""
# then SHOW CREATE TABLE <t> for each, diff vs init.sql
```

### 2. A drift gate (medium — the actual fix)

Something that fails loudly when prod and `init.sql` diverge. Cheapest credible
option: a scheduled job (or a CI job with read-only prod access) that runs
`SHOW CREATE TABLE` for every table and diffs against a normalised dump of
`init.sql`, alerting to Slack on mismatch. It does not need to be clever — it
needs to exist, because the current detector is "someone runs a load test and
notices 27.5bn rows".

### 3. Retire the Postgres-era schema docs (medium)

Make `docs/architecture/database-schema/**` describe the deployed ClickHouse
store: mark `database-schema-overview.md` historical (or delete it — Postgres is
gone), and promote the ClickHouse page from "pilot" to the production schema
reference including skip indexes / projections. Until this lands, the ADR 0032
checkbox on any schema PR is a formality that cannot be met in good faith.

## Acceptance Criteria

- [ ] Every prod CH table's deployed DDL is reconciled with `init.sql` (enumerated, not sampled)
- [ ] `oa_pool_seek` projection is in `init.sql` (or consciously documented as prod-only, with the reason)
- [ ] A drift gate exists that fails/alerts when deployed DDL diverges from `init.sql`
- [ ] `docs/architecture/database-schema/**` describes ClickHouse as production; Postgres content is marked historical or removed
- [ ] Skip indexes + projections are documented where the ADR 0032 gate points reviewers
- [ ] **Docs updated** — this task IS the docs update; see criterion above
- [ ] **API types regenerated** — N/A (schema/ops only; no API surface change)

## Notes

Scope boundary: this task does **not** propose changing what is deployed. Every
prod-only object found so far is correct and load-bearing — `closed_at_mm` is
worth 27.5bn rows per run. The goal is that code describes reality, and that the
next divergence is caught by a gate rather than by a load test.
