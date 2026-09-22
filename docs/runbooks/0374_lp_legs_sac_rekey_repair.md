# Repair: soroban `liquidity_pools.legs` keyed on the SAC (task 0374)

A one-off repair of `legs` on soroban rows (`pool_kind = 1`), for rows written
before the corrected writer was deployed.

## What is wrong and why this exists

The three soroban registry-row builders keyed every leg on the token's own
contract surrogate. That is right for a Soroban-native token — its contract
surrogate IS its `assets.id` — and wrong for a SAC: ADR 0051 retired
`asset_type = 2`, so a SAC has no `assets` row of its own and a leg keyed on
its surrogate points at nothing. The same asset therefore had two ids depending
on the kind of pool referencing it, and no soroban pool carried native XLM's id.

The writer is fixed in two parts, both in `crates/db-clickhouse`:

1. `contract_token_asset_id` in `persist/stage.rs` keys a leg on the asset,
   through the same SAC-to-classic map the balance path uses.
2. `sac_classic_map_needed` in `persist.rs` loads that map on any ledger that
   registers a pool, live and under `--only` alike. Without it a pool
   registered in a ledger with no token balance change, and every pool a
   targeted re-parse rewrote, still got a SAC-keyed leg.

This runbook repairs the rows written before BOTH parts.

Measured read-only on production 2026-09-22: **1,454 of 1,553 soroban legs are
keyed on a SAC**, on 739 of 770 pools, and 0 soroban pools carry native XLM's
id.

## Preconditions — the order is the whole safety argument

A ClickHouse mutation rewrites the parts that exist when it starts. Rows
inserted afterwards are untouched. So **every writer still producing old-rule
rows must be finished before step 3**, or it re-introduces them.

1. **No registry backfill is running.** It emits `liquidity_pools` rows, so a
   run in flight is such a writer.
2. **The corrected writer is deployed** — both parts above, indexer and
   `backfill-runner` alike. From that moment the live indexer writes correct
   legs, which is what makes step 3 safe without a pause.
3. Only then: run the repair.

**No indexer pause is needed** — that is what the ordering buys. A pause would
be required only if the mutation ran before the writer was deployed.

**Standing hazard, permanent:** running any backfill with a binary that lacks
either part after this repair re-breaks the column. Precondition 2 applies to
every later run, not just this one.

## The rewrite expression

Used identically by the dry run and the mutation, so what you verify is what
you run:

```sql
arrayMap(
  (x, y) -> if(y = 0, x, y),
  legs,
  arrayMap(x ->
    (SELECT CAST(groupArray((s.sacid, a.aid)), 'Map(Int64, Int64)')
     FROM (SELECT sac_contract_id AS sacid,
                  any(asset_type) t, any(asset_code) c, any(issuer_id) i
           FROM asset_sac WHERE sac_contract_id != 0
           GROUP BY sac_contract_id) AS s
     INNER JOIN (SELECT asset_type t, asset_code c, issuer_id i, any(id) aid
                 FROM assets WHERE asset_type IN (0, 1)
                 GROUP BY asset_type, asset_code, issuer_id) AS a
       ON a.t = s.t AND a.c = s.c AND a.i = s.i)[x],
    legs))
```

Notes on the shape, each one deliberate:

- The map is built from `asset_sac` LIVE, not from a pasted snapshot, so it
  cannot go stale between generating and running.
- A missing key yields `0`, and `0` is never a valid leg — hence the
  `if(y = 0, x, y)` fallthrough, which leaves Soroban-native legs alone.
- The subquery appears **once**. Two `groupArray` subqueries feeding
  `transform(x, keys, values, x)` would also work, but their row order is not
  guaranteed to match between two independent executions — that would silently
  transpose the mapping. The `Map` form has no such hazard.
- No `OPTIMIZE ... FINAL` afterwards: a mutation rewrites parts in place rather
  than inserting a new version, so there is no version tie to merge away. It
  also updates the RMT's duplicate physical rows, which is what we want.

## 1. Dry run (read-only, safe to repeat)

Substitute `<REWRITE>` with the expression above.

```sql
SELECT count() AS pools,
       countIf(legs != <REWRITE>) AS pools_changed,
       sum(length(legs)) AS legs_total,
       sum(arrayCount(i -> legs[i] != (<REWRITE>)[i],
                      range(1, length(legs) + 1))) AS legs_changed
FROM (SELECT pool_id,
             argMax(pool_kind, last_updated_ledger) AS k,
             argMax(legs, last_updated_ledger) AS legs
      FROM liquidity_pools GROUP BY pool_id)
WHERE k = 1
```

Measured 2026-09-22: `770 | 739 | 1553 | 1454`. The numbers grow as pools
register; what matters is that `legs_changed` equals the SAC-keyed count from
step 2's hard invariant read without the rewrite.

## 2. Dry run of the invariants (read-only)

Both checks, against the state the mutation WOULD produce:

```sql
WITH t AS (SELECT <REWRITE> AS new_legs
           FROM (SELECT pool_id,
                        argMax(pool_kind, last_updated_ledger) AS k,
                        argMax(legs, last_updated_ledger) AS legs
                 FROM liquidity_pools GROUP BY pool_id)
           WHERE k = 1),
     fl AS (SELECT arrayJoin(new_legs) AS leg FROM t),
     sac AS (SELECT sac_contract_id AS sid FROM asset_sac
             WHERE sac_contract_id != 0 GROUP BY sac_contract_id),
     an AS (SELECT id, 1 AS found FROM assets GROUP BY id)
SELECT count()                    AS legs_after,
       countIf(sac.sid != 0)      AS hard_still_keyed_on_a_sac,
       countIf(an.found != 1)     AS soft_no_assets_row
FROM fl LEFT JOIN sac ON sac.sid = fl.leg
        LEFT JOIN an  ON an.id  = fl.leg
```

Measured 2026-09-22: `1553 | 0 | 1` (without the rewrite: `1553 | 1454 | 1455`).

- **`hard_still_keyed_on_a_sac` must be 0.** This is the defect itself; it has
  no legitimate non-zero value.
- **`soft_no_assets_row` is a counter, not a gate.** A leg can legitimately
  resolve to nothing when we have never observed its token at all. Today that
  is exactly one: pool
  `8FE06922A146D7BEBAB9CDD93D0E34224AFE09CFB42465378423F26DA8DE3370`,
  registered at ledger 50,875,676 by one of the five dead early config-factory
  deployments. It holds one `pool_state_changes` row (its creation reserves),
  emitted 2 events ever, and its leg token has emitted **zero** — above the
  ingest floor (50,457,424), simply inert. If this number climbs, a token
  family is escaping the dimension writers; investigate rather than relax the
  check.

Do not merge the two into one "every leg resolves in `assets`" check. It cannot
pass while that dead pool exists, and the pressure would then be to weaken it
until it catches nothing.

## 3. The mutation (WRITE — operator only)

```sql
ALTER TABLE liquidity_pools UPDATE legs = <REWRITE> WHERE pool_kind = 1;
```

Single-server `ReplacingMergeTree`, no replication, so no `ON CLUSTER`.

## 4. Watch it

```sql
SELECT mutation_id, command, parts_to_do, is_done, latest_fail_reason
FROM system.mutations
WHERE table = 'liquidity_pools'
ORDER BY create_time DESC LIMIT 3
```

`is_done = 1` and an empty `latest_fail_reason` mean it finished. Mutations are
asynchronous: the statement returns before the work does.

## 5. Verify

Re-run the invariant query from step 2 **without** the rewrite — read `legs`
directly. `hard_still_keyed_on_a_sac` must be `0`, and `soft_no_assets_row`
must be the same small number the dry run predicted.

Then confirm the read-side consequence — a native XLM leg now matches:

```sql
SELECT countIf(has(legs, -6959166271784855184)) AS soroban_pools_with_native_xlm
FROM (SELECT argMax(pool_kind, last_updated_ledger) AS k,
             argMax(legs, last_updated_ledger) AS legs
      FROM liquidity_pools GROUP BY pool_id)
WHERE k = 1
```

Measured 2026-09-22: `0` before, `260` with the rewrite applied in a read. That
number grows as pools register; `0` before is the part that must hold.

## Rollback

There is none in the "undo the mutation" sense — a mutation is destructive of
the previous values. It does not need one: the old values are recomputable from
the registration events, which sit complete in `soroban_events`, and the
corrected writer reproduces the new values from the same source. If the repair
were ever judged wrong, the answer is a registration re-parse, not an inverse
mutation.
