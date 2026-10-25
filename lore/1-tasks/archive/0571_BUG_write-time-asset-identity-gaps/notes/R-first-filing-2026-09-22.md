# R: the SAC half as first filed (2026-09-22)

Moved from the README on 2026-10-16. The root asked for here was found then;
see [R-sac-identity-2026-10-16.md](R-sac-identity-2026-10-16.md).

## Part 2 as filed

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
