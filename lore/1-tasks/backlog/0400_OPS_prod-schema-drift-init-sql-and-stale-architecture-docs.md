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
  - date: 2026-07-21
    status: backlog
    who: karolkow
    note: >
      Full prod-vs-`init.sql` cross-validation run (read-only) while closing 0426.
      **The whole drift is four items, and the column layer is nearly clean** — 28
      table definitions compared column-by-column against `system.columns`, and only
      one table differed. Concrete inventory, so this task no longer has to start by
      discovering its own scope.
      **(1)** `assets_pre0339` — prod-only table, 368,490 rows / 5.22 MiB. Not an
      accident: 0339 (stkrolikiewicz) kept it as a soak backup and its runbook warns
      it is NOT a full-table snapshot. Archived 2026-07-02; the deferral had no owner
      and no trigger, and it is still there 19 days later.
      **(2)** `transaction_hash_dict` — prod-only **Dictionary**, absent from
      `init.sql`, `status = LOADED` but `element_count = 1`, which looks wrong and
      should be explained before it is either documented or dropped.
      **(3)** `idx_oaa_transaction_id` — declared in `init.sql`, **missing on prod**.
      Drift in the direction that matters for reads: the code claims an index the
      server does not have.
      **(4)** `operation_asset_appearances.net_settled` — in `init.sql`, absent on
      prod. **Already owned by 0419** (its rollout carries the `ALTER TABLE … ADD
      COLUMN`), and the API code reading it is on develop, not master — so this is
      planned undeployed work, not drift. It is also a **release-ordering hazard**:
      deploying the API ahead of 0419's ALTER yields `Code 47` on that endpoint, the
      same failure shape as the 0304 → 0388 → 0392 family.
      One correction to this task's own opening table: the `oa_pool_seek` projection
      it lists as prod-only is **not on prod** — `system.projections` is empty, and
      CH 26.3 refuses projections on `ReplacingMergeTree` outright (`Code 344`, cf.
      0353). That row is stale.
  - date: 2026-07-29
    status: backlog
    who: karolkow
    note: >
      Body reconciled with the 2026-07-21 entry, which it had been contradicting:
      the opening table still advertised `oa_pool_seek` as outstanding prod-only
      drift, and an acceptance criterion still demanded it be added to
      `init.sql` — a criterion nobody could ever satisfy. Both corrected, and the
      criterion repointed at `assets_pre0339`, which is the real prod-only object.
      One claim in the 2026-07-21 entry also does not hold: `transaction_hash_dict`
      is NOT absent from `init.sql` — `CREATE DICTIONARY IF NOT EXISTS` has been
      there since 2026-05-10 (`8b41d9d7`, task 0204). Its `element_count = 1`
      anomaly stands and is still worth explaining. Re-verified everything else on
      prod today (columns, types, engines, sort keys, skip indexes, projections,
      views, dictionaries): the only structural gaps are `net_settled` and
      `idx_oaa_transaction_id`, both owned by 0419.
  - date: 2026-08-06
    status: backlog
    who: karolkow
    note: >
      Re-measured all four items on prod (read-only, via 0455 work).
      (1) assets_pre0339: GONE - dropped since July, resolved. (2)
      transaction_hash_dict element_count=1: explained, not an anomaly - the
      dictionary layout is ComplexKeyCache, so element_count reports entries
      currently cached, not source size; 1 = cold cache. (3)
      idx_oaa_transaction_id: still missing on prod - 0419 deployed the
      net_settled column but not the index; orphaned half, needs
      ALTER ADD INDEX + MATERIALIZE (operator). (4) net_settled: EXISTS on
      prod now, resolved. NEW fifth item: prod carries
      idx_oa_asset_issuer_id on operations_appearances, which init.sql
      deliberately omits (comment says DROP frees ~97 MiB) - a decided but
      never-executed DROP, reverse-direction drift; operator decision.
      Structural layer closes with those two ALTERs; the docs half
      (architecture describing the retired PG world) remains.
  - date: 2026-08-06
    status: backlog
    who: karolkow
    note: >
      Structural layer reconciled: idx_oa_asset_issuer_id dropped on prod
      (0381's recorded decision executed), idx_oaa_transaction_id
      added+materialized then re-audited as consumer-less (19.87 GiB for a
      withdrawn read path - see 0419's same-day entry; resolved same day:
      dropped on BOTH sides - prod and init.sql, with the consumer story
      recorded at the removal site). Lesson for this task's
      docs half: declared-vs-actual drift can sit in the DECLARATION -
      init.sql carried an index for a read that no longer exists, so
      reconciling prod TOWARD init.sql was reconciling toward a stale claim.
      The comparator must flag both directions, and init.sql entries need
      their consumer named.
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
| ~~`oa_pool_seek` projection on `operations_appearances`~~             | **does not exist** — see below              | listed here in error                                               |

`closed_at_mm` was applied deliberately and is correct — the defect is that code
and prod diverged with nothing to detect it.

The projection row was wrong from the start and is struck: `system.projections`
is empty on prod, `operations_appearances` carries three bloom indexes and no
projection, and ClickHouse 26.3 refuses projections on `ReplacingMergeTree`
outright (`Code 344`, cf. 0353) — so it could not have existed. Re-verified
2026-07-29. The drift this task owns is real, but its inventory is the one in
the 2026-07-21 history entry, not this table.

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
`closed_at_mm` is already done. The enumerated remainder is in the 2026-07-21
history entry, re-verified 2026-07-29: `assets_pre0339` (prod-only table),
`idx_oaa_transaction_id` and `operation_asset_appearances.net_settled` (both
declared in code, absent on prod — owned by 0419). Everything else matches:
28 table definitions, their columns, types, engines and sort keys, plus both
materialised views and the dictionary, which `init.sql` does create.

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
- [ ] `assets_pre0339` is either dropped or documented in `init.sql` as a
      deliberate prod-only soak backup, with an owner and a removal trigger
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
