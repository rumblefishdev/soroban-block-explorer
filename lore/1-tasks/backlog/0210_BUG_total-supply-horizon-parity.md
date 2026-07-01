---
id: '0210'
title: 'BUG: assets.total_supply Horizon parity — extend MVP sum to 4 sources'
type: BUG
status: backlog
related_adr: ['0043']
related_tasks: ['0194', '0197', '0331', '0339', '0323']
tags:
  [priority-high, effort-medium, layer-indexer, layer-xdr-parsing, correctness]
milestone: 2
links:
  - https://developers.stellar.org/docs/data/horizon/api-reference/aggregations/assets/list
history:
  - date: '2026-05-12'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0194 Future Work. 0194 shipped an MVP `total_supply`
      that sums only trustlines via `SUM(account_balances_current.balance)`
      per `(code, issuer_id)`. Horizon `/assets` aggregates 4 sources; the
      MVP misses 3 of them, causing known drift up to ~20-50% on DeFi
      assets (USDC w/ Soroswap + SAC). This task closes the gap and
      validates parity against an external source.
  - date: '2026-06-30'
    status: backlog
    who: stkrolikiewicz
    note: >
      Re-confirmed in a SAC/asset modeling session: the SAC contract-holder gap
      (`holder_count` + `total_supply` miss contract-side `ContractData` balances) is real
      and has a Horizon parity target (`num_accounts` + `num_contracts` / `contracts_amount`).
      Phase 3 (SAC contract holdings) owns the supply half; the `holder_count` half stays
      deferred here (out of scope) but now has a confirmed Horizon target if un-deferred —
      note the "semantics differ from trustline count" caveat applies to the ACCOUNT side;
      the CONTRACT side has a clean `num_contracts` target. 0323 Phase 2 executed →
      `soroban_contracts` is now deployed-only, so deployed-SAC identification for Phase 3 is
      cleaner (`is_sac=true, deployed>0`). Entity-model context: 0339 (SAC = facet of
      classic_credit, not a separate asset_type).
---

# BUG: `assets.total_supply` Horizon parity — extend MVP sum to 4 sources

## Summary

0194 shipped `assets.total_supply` as a per-ledger recompute summing **only trustlines**:

```sql
SUM(account_balances_current.balance) WHERE (code, issuer_id) = (...)
```

Horizon `/assets` aggregates the same field across **four** sources. Three are
missing from the MVP — producing systematic under-count on every classic credit
that also lives in claimable balances, LP reserves, or SAC contract storage.
Drift is up to ~20-50% on DeFi assets (USDC w/ Soroswap + SAC) per 0194 closing
notes.

This is the only ADR 0043 list-endpoint column whose **correctness** is suspect.
0197 audit verifies only non-NULL, not value parity — so this gap will not be
caught by the audit and must be its own task.

## Context

### The four sources Horizon aggregates

1. **Trustlines** — `account_balances_current.balance` per `(code, issuer_id)`.
   Already summed by 0194 MVP. ✅
2. **Claimable balances** — `claimable_balances.amount` per `(code, issuer_id)`.
   Pre-claim hot wallet liquidity. ❌ NOT in MVP.
3. **Liquidity pool reserves** — `liquidity_pools.reserve_a` / `reserve_b` per
   asset participant. Significant for AMM-listed pairs. ❌ NOT in MVP.
   **0194 implementation Round 4 prototyped this**; schema already in place
   per 0194 §1b.
4. **SAC contract holdings** — Stellar Asset Contract instance balance held
   inside Soroban contracts (SAC entries in `contract_data`). Per-asset SAC
   contract tracking required. ❌ NOT in MVP; needs new tracking.

### Why this matters

| Asset                          | Trustlines only           | Horizon total | Drift                    |
| ------------------------------ | ------------------------- | ------------- | ------------------------ |
| Native XLM                     | n/a (excluded by Horizon) | n/a           | n/a                      |
| Plain classic credit (no DeFi) | ~accurate                 | ~accurate     | ~0%                      |
| USDC                           | partial                   | high          | ~20-50% (per 0194 notes) |
| Any AMM-listed pair            | partial                   | high          | bound by pool TVL share  |
| Any SAC-wrapped asset          | partial                   | high          | bound by Soroban TVL     |

Drift makes `/v1/assets` list-endpoint `total_supply` misleading vs every other
Stellar explorer. Block explorer SHOULD match Horizon by default.

## Scope

### In

- Extend `recompute_asset_aggregates` (`crates/indexer/src/handler/persist/write.rs`)
  to sum across **all four** sources for `total_supply`.
- Add the three missing sources one-by-one with separate sub-blocks (LP
  reserves first — schema already in place; claimable balances second; SAC
  contract holdings last — heaviest design work).
- For SAC: design + implement per-asset SAC contract holdings tracking. Likely
  a new aggregation table populated by the indexer when SAC-related
  ContractData entries appear. Open question: do we track at-rest or
  per-flow? At-rest is what Horizon does.
- Final validation: external-source parity check on a representative asset
  set — run the new recompute on a backfilled snapshot, then compare against
  Horizon `/assets?asset_code=...&asset_issuer=...` AND
  stellar.expert API for the same assets. Document drift %. Acceptance target
  < 1% drift on the sample (any larger gap means missing source or wrong
  arithmetic).

### Out

- `assets.holder_count` Horizon parity — separate task if drift surfaces;
  active-holder semantics differs from Horizon's "trustline count" anyway,
  per 0194 §1c.
- Changing the field semantics (e.g. adding a separate `circulating_supply`
  column that excludes issuer-held balance). Out of scope; would need ADR.
- Re-running 0196 enrichment-backfill drain — separate, follows once the new
  fields land.

## Implementation Plan

### Phase 1: LP reserves (smallest delta)

Schema in place (`liquidity_pools` table populated by indexer). 0194 Round 4
already prototyped this path; resurrect prototype, add to
`recompute_asset_aggregates`. UPDATE statement gets a UNION-ALL or additional
LEFT JOIN LATERAL. Cost: similar shape to the trustline sum, ~marginal
overhead.

### Phase 2: Claimable balances

`claimable_balances.amount` per `(code, issuer_id)`. Indexer already writes
this table. Add a second sum to the recompute statement.

### Phase 3: SAC contract holdings (design-heavy)

Requires per-asset SAC contract holdings tracking. Two paths:

- **3a — derive at recompute time** from `contract_data` entries (SAC
  instance balances live in `LedgerEntry::ContractData` per the SAC contract
  pattern). Cost: scans a wide table per recompute.
- **3b — maintain a dedicated aggregation table** (`asset_sac_holdings(code,
issuer_id, balance)`) updated by the indexer when SAC ContractData entries
  change. Cost: extra write path, but recompute reads a small narrow table.

Pick 3b if indexer can identify SAC entries cheaply via xdr-parser; pick 3a
otherwise. Decide via a spike before committing schema.

### Phase 4: External-source parity validation (acceptance gate)

After all three sources land, run a one-shot parity check:

1. Pick a sample of ~20 assets covering: plain classic credit, USDC, AMM-only
   asset, SAC-wrapped asset, mixed (all four sources).
2. For each, query:
   - `GET /v1/assets/{code}-{issuer}` on staging (post-backfill)
   - Horizon `/assets?asset_code=...&asset_issuer=...`
   - stellar.expert `/explorer/public/asset/{code}-{issuer}` API
3. Diff `total_supply` across all three. Expected: < 1% drift between our
   value and Horizon. stellar.expert is a tiebreaker.
4. Document results in a snapshot under `docs/audits/2026-MM-DD-
total-supply-parity.md`. Each row a real (code, issuer, ours, horizon,
   stellar.expert, drift%) entry.
5. Any drift > 1% on a non-edge-case asset = bug, fix before merge.

## Acceptance Criteria

- [ ] `recompute_asset_aggregates` sums trustlines + LP reserves + claimable
      balances + SAC contract holdings.
- [ ] SAC contract holdings path designed via spike + ADR amendment to 0043
      (or new ADR) documenting the choice between 3a / 3b.
- [ ] Per-ledger overhead measured. Target: < +10% over post-0194 baseline.
      0194 measured +4% baseline; new ceiling +14%.
- [ ] External-source parity snapshot committed to `docs/audits/`. Sample
      ≥ 20 assets, drift < 1% on ≥ 95% of them, every outlier explained
      (issuer-held excluded? SAC entry not yet tracked?).
- [ ] Docs updated: `docs/architecture/database-schema/database-schema-overview.md`
      §4.10 Assets (total_supply now 4-source aggregate);
      `docs/architecture/xdr-parsing/` if new SAC parsing lands;
      `docs/architecture/indexing-pipeline/indexing-pipeline-overview.md`
      §5.2 step 14 if recompute shape changes substantially.
- [ ] ADR 0043 cross-checked. Allocation (list-endpoint + on-chain → indexer)
      unchanged — no amendment needed unless SAC path forces a new column.

## Future Work

- **Continuous parity monitor** — periodic CI job that re-runs Phase 4 sample
  against Horizon and alerts on drift > 5%. Defer; one-shot validation is
  enough for v1.
- **`circulating_supply`** column — issuer-held balance excluded. Out of
  scope; would need product decision + ADR.

## Notes

- **Phase 3 mechanism = the 0331 ContractData-balance ingestion (UPDATED
  2026-06-29).** ⚠️ The earlier "event-fold over `soroban_events`" idea is
  REFUTED — measured on prod (`stellar` RPC): the fold under-counts 3/3 tokens
  with a getter (vault / rebasing / non-SEP-41-event tokens change balances with
  no foldable event; 54% of type-3 events are non-SEP-41). 0331 pivoted to reading
  **ledger STATE**: `ContractData` `Balance(Address)` entries → `soroban_token_balances`
  (the parser already decodes these — `xdr-parser::extract_soroban_token_balances`).
  Phase 3 (SAC contract holdings) is the **same mechanism on type-2 SAC contracts**:
  same `Vec[Symbol("Balance"), Address]` key, same table/framework — the only delta
  is the value shape (SAC stores a `BalanceValue` **struct**: amount + authorized +
  clawback flags, vs the bespoke-token bare `i128`), so Phase 3 adds a struct decoder
  + resolves SAC `contract_id → (code, issuer)`. The "non-standard storage-key
  layouts" line that scoped out 0138 is **disproven** (the standard `Balance(Address)`
  key was confirmed readable on a real vault token). **0331 lands the ingestion
  framework first; Phase 3 is a small extension on top, not a separate path.**
  0331 still owns type-3 (bespoke Soroban) supply+holders, out of this task's scope.
  - **Double-count trap:** a SAC holds the real classic asset as a G-address
    trustline (already in source #1) AND represents it as Soroban `Balance` entries
    held by C-contracts. Sum only the C-contract holdings not already in the
    trustline sum — matching Horizon's source #4.

- **0194 deliberately deferred this.** From 0194 closing history (2026-05-XX):
  "Full Horizon-parity total_supply (LP reserves + claimable_balances + SAC
  contract holdings) explicitly deferred to Future Work." This task is the
  promised follow-up.
- **0197 doesn't catch this.** The audit checks non-NULL on sample queries, not
  value parity. Spawned independently per the 0197 punch list.
- **Sequencing.** Phase 1+2 can ship together (LP reserves + claimable
  balances) as a smaller PR. Phase 3 needs its own PR with the spike + design
  decision. Phase 4 parity check runs on top of Phase 3.
