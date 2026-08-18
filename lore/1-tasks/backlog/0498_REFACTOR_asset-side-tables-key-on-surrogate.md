---
id: '0498'
title: 'REFACTOR: key the asset side tables on the surrogate id, not the natural 4-tuple'
type: REFACTOR
status: backlog
related_adr: ['0051', '0056']
related_tasks: ['0339', '0331']
tags: [backend, clickhouse, assets, api, priority-low, effort-small]
links: []
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Spawned from the LP-holdings decision session, which audited the SAC
      design on its own merits rather than treating it as precedent. The
      design holds; this defect and one unmeasured cost fell out of the audit.
---

# REFACTOR: asset side tables join on 4 columns

## The defect

`asset_sac` and `asset_enrichment` are keyed byte-for-byte on the natural
asset identity `(asset_type, asset_code, issuer_id, contract_id)` instead of
on the `assets.id` surrogate. Every join spells all the columns
(`crates/api/src/assets/queries.rs:465`, `liquidity_pools/queries.rs:751`,
`:1823`, `assets/queries.rs:1298`), and every evolution of asset identity —
the pool `pool_id` key column from ADR 0056 being the live example — forces
the question "do the mirrors need updating too". A surrogate-keyed side table
is invisible to identity evolution and joins on one column.

Every other dimension relationship in the schema already goes through a
surrogate (`balances.asset_id`, `accounts.id`, `soroban_contracts.id`).

## First establish WHY it is this way

Likely reason: `assets.id` carries `DEFAULT 0` until a row is rewritten or
backfilled (`schema/init.sql:304-307`), so at the time `asset_sac` was built
the surrogate may not have been trustworthy for every row. **Verify that on
production** (`countIf(id = 0)` over `assets FINAL`) before re-keying — if
zero-id rows still exist, the backfill of `assets.id` is a prerequisite, not a
footnote.

## Also measure while in there

`asset_sac` holds **469,043 rows** (prod, 2026-08-17) against ~340k classic
assets — unmerged RMT duplicates, the task 0420 pattern — and every read pays
a `GROUP BY` dedup subquery. Measure the read cost before/after re-keying;
this was never measured when the table was designed.

## Done means

- Side tables keyed (or at least joined) on `assets.id`; joins are one column
- `assets.id = 0` population verified zero, or backfilled first
- read_rows before/after recorded
- **Docs updated** — `docs/architecture/**` schema section if the key changes
- **API types** — N/A (no wire change)
