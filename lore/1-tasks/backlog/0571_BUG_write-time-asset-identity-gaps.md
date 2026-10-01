---
id: '0571'
title: 'BUG: rows keyed on an asset that has no `assets` row — issuer-only pool legs, SAC balances keyed on the contract'
type: BUG
status: backlog
related_adr: ['0051', '0058']
related_tasks: ['0374', '0530', '0503']
tags:
  ['clickhouse', 'indexer', 'data-integrity', 'priority-low', 'effort-medium']
links: []
history:
  - date: '2026-09-22'
    status: backlog
    who: karolkow
    note: >
      Filed from the 0530 review of PR #455. Two write paths store an asset
      surrogate for which no `assets` row is ever written. Both measured on
      production and traced to their cause; the two orphaned pool legs were
      repaired by hand the same day, the balances are not.
---

# BUG: rows keyed on an asset that has no `assets` row

## Summary

Two write paths store an asset surrogate that joins to nothing in `assets`,
so every read that resolves the asset shows it as unknown. Both are the same
shape — the identity row is only ever produced by one specific ledger event,
and the referencing row is written without it — but they have separate
causes and separate fixes:

1. **Classic pool legs created by the asset's issuer** get no `assets` row.
2. **Contract-held SAC balances** are keyed on the SAC contract surrogate
   instead of the classic asset it wraps, when the SAC's `asset_sac` facet
   says "not deployed".

## Part 1 — issuer-only pool legs

**Cause.** A classic credit `assets` row is produced only from a trustline
change (`detect_classic_credit_assets`, `crates/xdr-parser/src/state.rs:1112`)
or a SAC sighting. The issuer of an asset may open a pool-share trustline
without holding a trustline to its own asset — stellar-core's
`ChangeTrustOpFrame` skips the check when the source `isIssuer` — so a pool
whose only participant is the issuer changes no asset trustline, and its
credit leg's surrogate has no row.

**Measured 2026-09-16:** 2 of 52,974 classic pools — `XLM/PIF`
(`C305DB2D401608D9732783120AE9BE061B95FCEB6A8E3EAD45E24D1686AA7252`) and
`XLM/SUR810` (`92415CA8B582ECF78B31CAFD17CDEC19E30005411570D882EA1F5F399CF0F1C6`).
Both confirmed on chain (`getLedgerEntries`): empty pools, one pool-share
trustline, held by the issuer. No balances, no deposits.

**Repaired 2026-09-22** before the pair columns can be dropped (PR #455 —
after the drop the code and issuer of these legs would exist nowhere in the
database): two rows inserted into `assets`, `id` = the value already in the
pool's `legs`, code and issuer from the pair columns. Orphaned classic legs
after the insert: 0 of 107,044.

**Fix still owed:** when a classic pool row is staged, emit an `assets`
identity row for each credit leg, the way the SAC deploy path does
(`push_asset`). Both callers of `classic_pools::build_pool_rows` need it —
the indexer (`crates/db-clickhouse/src/persist/stage.rs:1113`) and the
checkpoint seed (`crates/backfill-runner/src/snapshot/pools.rs:79`).
**Sequenced after PR #455 merges, as a separate PR** (decided 2026-09-22):
PR #455 rewrites the same builder while dropping the pair columns.

## Part 2 — SAC balances keyed on the contract

**Cause.** `build_balance_rows` (`crates/db-clickhouse/src/persist/stage.rs:559`)
resolves a storing contract through the SAC → classic map; a contract absent
from the map is keyed as a type-3 token (`:576`), silently. The map
(`fetch_sac_classic_map`, `crates/db-clickhouse/src/persist.rs:292`) keeps only
facets with `max(sac_deployed) = 1`. For 138 SACs the facet says 0 although
`soroban_contracts` holds them with `is_sac = 1` and a deploy ledger — so the
deploy was seen, but its facet was never marked deployed. Why the deploy path
did not write `sac_deployed = 1` for these is not yet known; that is the root
to find.

The same map and the same silent fallback key soroban pool legs:
`contract_token_asset_id` in PR #455 (`stage.rs`) is the leg-side twin of
`build_balance_rows`. 2 soroban pools have a leg on one of the 138 SACs
today. Runbook mutation A repairs those rows (its map does not filter on
`sac_deployed`), but a new registration naming one of these SACs orphans its
leg again until the root is fixed.

**Measured 2026-09-22:**

|                                                          | count                   |
| -------------------------------------------------------- | ----------------------- |
| SAC contracts in `soroban_contracts`                     | 4,029                   |
| — facet marked deployed                                  | 3,889                   |
| — facet marked **not** deployed                          | **138**                 |
| — no facet at all                                        | 2                       |
| `balances` (holder, asset) pairs keyed on a SAC contract | **286**                 |
| — holders (all of them contracts)                        | 146                     |
| — SAC contracts involved                                 | 135 (all among the 138) |

All 286 amounts are non-zero. Deploy ledgers and balance ledgers of the 135
both fall in `59,219,574 .. 62,803,627`; nothing written since 2026-09-16.
For none of the 286 pairs does a row exist under the classic asset id, so
these are the only copies — misfiled, not duplicated. The six largest carry
random-hex codes (e.g. `592ace85374d`); user impact looks small, not verified
for the rest.

**Fix owed:**

1. Find why the deploy path leaves `sac_deployed = 0` for a deployed SAC, and
   fix it at the root.
2. Stop the silent fallback in both places: a contract known to be a SAC
   that the map cannot resolve must not be keyed as a type-3 token — neither
   as a balance (`build_balance_rows`) nor as a pool leg
   (`contract_token_asset_id`).
3. Repair the 286 rows. `asset_id` is in the `balances` sort key
   (`holder_id, asset_id`), so this cannot be an `ALTER … UPDATE`: insert
   each row under the classic id (same map shape as mutation A in
   `docs/runbooks/0374_lp_legs_sac_rekey_repair.md`), then delete the
   contract-keyed rows, then recompute `balance_aggregates` for the affected
   assets. Correct the 138 facets as part of the repair.

## Investigated, not in scope

- **`operation_asset_appearances`: 7,530 asset ids with no `assets` row** —
  not a gap. They are assets named in operation bodies that never existed on
  the ledger: in a sampled window, 71% of the transactions failed, and the
  successful rest are offer cancellations (`amount 0` + `offer_id`) naming a
  placeholder asset, which stellar-core never checks on delete
  (`ManageOfferOpFrameBase::checkOfferValid` returns early for a delete).
- **`balances`: 25 other contract ids with no `assets` row** (23 classified
  `Other`, 2 `Nft`) — classifier coverage, not identity (NFT side: 0392).

## Noted while repairing the pool legs (2026-09-22)

- **Scheduling:** picked up after the #455 split finishes (decision, karolkow).
  Nothing in that split depends on either part; PR #474 repaired the soroban
  legs of the 138 SACs below with a map that ignores `sac_deployed`, so only a
  NEW registration naming one of them re-orphans a leg.
- **The map's query is the table's whole scan.** `fetch_sac_classic_map` reads
  every `asset_sac` row (456,408 on 2026-09-22, 8 MiB) to return the 3,890 with
  a SAC contract: 0.11 s measured per call, and the indexer calls it once per
  ledger that needs it. Invisible live, a real cost in a full backfill. Fix it
  in whichever change touches the map next — an `asset_sac` row carries no
  index on `sac_contract_id`.

## Acceptance Criteria

- [ ] Part 1: a classic pool whose only participant is the issuer stages an
      `assets` row for its credit leg — unit test on `build_pool_rows`'s
      callers, both the indexer and the seed
- [ ] Part 1: after deploy, orphaned classic legs stay 0 (query below)
- [ ] Part 2: root cause of `sac_deployed = 0` on a deployed SAC found and
      fixed; test reproduces it
- [ ] Part 2: neither `build_balance_rows` nor `contract_token_asset_id` keys
      a known SAC as a type-3 token
- [ ] Part 2: the 286 rows re-keyed, `balance_aggregates` recomputed; no
      `balances` row keyed on a SAC contract id remains
- [ ] **Docs updated** — `docs/architecture/**` sections on asset identity /
      ingestion describe where an `assets` row comes from, or `N/A — reason`
- [ ] **API types regenerated** — `N/A` unless `crates/api/**` changes

## Measurement queries

```sql
-- Part 1: classic legs with no assets row (expect 0)
WITH p AS (SELECT pool_id, argMax(legs, last_updated_ledger) legs,
                  argMax(pool_kind, last_updated_ledger) kind
           FROM liquidity_pools GROUP BY pool_id)
SELECT countIf(leg NOT IN (SELECT id FROM assets))
FROM p ARRAY JOIN legs AS leg WHERE kind = 0;

-- Part 2: deployed SACs whose facet says not deployed
WITH f AS (SELECT max(sac_contract_id) sid, max(sac_deployed) dep FROM asset_sac
           WHERE sac_contract_id != 0
           GROUP BY asset_type, asset_code, issuer_id, contract_id)
SELECT countIf(f.dep = 0) FROM f
WHERE f.sid IN (SELECT id FROM soroban_contracts WHERE is_sac);

-- Part 2: (holder, asset) pairs keyed on a SAC contract (286 on 2026-09-22)
SELECT uniqExact(holder_id, asset_id) FROM balances
WHERE asset_id IN (SELECT id FROM soroban_contracts WHERE is_sac);
```
