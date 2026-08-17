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
  - date: '2026-07-02'
    status: backlog
    who: karolkow
    note: >
      2 of the 4 sources SUBSUMED by task 0331 (unified `balances`): #1 trustlines
      (classic→balances migration) and #4 SAC/contract holdings (contract-held type-0/1
      re-key, ADR 0051 — incl. Soroban-DEX pool reserves, which are contract-held). The
      old `recompute_asset_aggregates` mechanism is dead (PG retired); supply is now
      `sum(balances)`. Remaining = the 2 NON-contract sources: #2 claimable balances
      (only ops parsed, no state table) + #3 native protocol LP reserves
      (`LiquidityPoolEntry`, not a contract). Rewritten scope: write synthetic `balances`
      rows for those two. Still backlog. See the dated status section in the body.
  - date: '2026-07-02'
    status: backlog
    who: claude
    note: >
      Faza-3 item folded here from the 0331 OPS close-out (2026-07-02): a per-protocol decoder
      for CUSTOM-STORAGE Soroban LP pools. ~264 type-3 LP-share tokens (Comet `CPAL` x136,
      `Pool Share Token`/Soroswap x128) render `—` for supply because their LP-share balances
      live in custom u32-keyed instance storage, NOT the standard SEP-41 `Balance(Address)`
      ContractData key the 0331 seed reads. Needs one decoder per protocol (Comet / Soroswap /
      Phoenix layouts). SCOPE FLAG: this is a SOROBAN (type-3) LP-SHARE SUPPLY gap, distinct
      from 0210's classic Horizon-parity core (#2 claimable + #3 native-LP) — parked here per
      operator; a standalone task or 0199 (LP analytics) may be a cleaner home if it muddies
      0210. The pool's HELD reserves are already captured by 0331 (contract-held); only the
      LP-SHARE token supply is missing. External check (StellarExpert live, 2026-07-02)
      confirmed the type-3 coverage is otherwise complete — no indexing gap, just this decoder.
  - date: '2026-08-18'
    status: backlog
    who: karolkow
    note: >
      Task 0505 MERGED IN and its file removed. Two things changed. (1) This
      task's verification target was Horizon, which Karol ruled legacy and
      banned from verification on 2026-08-17 — so the acceptance criterion
      "< 1% drift vs Horizon" is no longer usable. The replacement is the
      protocol's own `LedgerHeader.total_coins` / `fee_pool`, which we already
      receive in every ledger and currently discard. (2) That reframes the
      goal: supply is not validated by matching another indexer, it is
      validated by a reconciliation identity that must balance. See the
      2026-08-18 section in the body.
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

## 2026-07-02 (karolkow) — 2 of 4 sources SUBSUMED by task 0331; mechanism changed

Task **0331** (unified `balances` model, Option C) closed **2 of the 4 Horizon
sources** — including the one this task flagged as "heaviest design work":

- **#1 Trustlines — DONE.** The classic `account_balances_current` → `balances`
  migration + live single-write lands every trustline holding in `balances`.
- **#4 SAC / contract holdings — DONE.** 0331's contract-held type-0/1 re-key
  (ADR 0051) indexes every contract that holds a classic/native asset via its SAC
  as a `balances` row keyed on the wrapped asset. **This includes Soroban-DEX pool
  reserves** (Soroswap/Phoenix etc. — they hold their reserves AS a contract), which
  was the bulk of the SAC-holdings concern.

**Mechanism is different now.** This task's plan targets `recompute_asset_aggregates`
in `crates/indexer/src/handler/persist/write.rs` — that whole PG path is **dead**
(PG retired). On the new model, supply = `sum(balances)` via `balance_aggregates`, so
each source just needs its holdings written as `balances` rows (additive, no recompute).

**Remaining = the 2 NON-contract sources only:**

- **#2 Claimable balances** — we parse the _operations_ (create/claim/clawback) but
  keep **no state table** of per-asset claimable amounts (no `claimable_balances`
  table on prod). Needs a new ingestion path.
- **#3 NATIVE protocol LP reserves** — a classic Stellar AMM (`LiquidityPoolEntry`)
  is NOT a contract; its reserves live in the protocol pool entry, not a trustline or
  a `Balance(contract)` entry, so 0331 does not capture them. (`liquidity_pools` holds
  74,728 pool _definitions_ but no reserve columns; reserves are in `pool_snapshots`.)

**Rewritten scope:** write synthetic `balances` rows for claimable amounts (holder =
claimable-balance id) + native-LP reserves (holder = pool id), keyed by `assets.id`.
Then `sum(balances)` reaches full Horizon parity. Much smaller than the original 4-source
recompute. Until then, classic-asset `total_supply` undercounts by (claimable + native-LP
reserves) — the residual ~20-50% drift on heavily-pooled assets.

## 2026-08-18 (karolkow) — the oracle changes: `total_coins`, not Horizon

**Horizon is legacy and banned from verification.** The old acceptance target
("< 1% drift vs Horizon `/assets`") cannot be used. Two independent reasons,
either sufficient: Horizon is another indexer's opinion rather than the
protocol's own accounting, and it has twice misled this project on fields it
derives itself.

**The replacement is already in every ledger and we throw it away.**
`LedgerHeader` carries `total_coins` and `fee_pool` — the protocol's own count
of every stroop in existence. Our `ledgers` table stores six header fields
(`sequence`, `hash`, `closed_at`, `protocol_version`, `transaction_count`,
`base_fee`) and discards the rest, including both of these, plus `base_reserve`
(minimum-balance reasoning) and `bucket_list_hash` (checkpoint state hash —
useful to task 0502).

### The reconciliation identity — this task's real acceptance criterion

The right question is not "does our sum match an external figure" but "does the
ledger balance". For XLM:

```
total_coins  =  Σ account XLM          (indexed today)
              + Σ claimable balances   (source #2 — NOT indexed)
              + Σ native LP reserves   (source #3 — NOT indexed)
              + fee_pool               (header field — not stored)
```

Our sum should therefore **fall short of `total_coins` by exactly the
unindexed terms**. Equality today would signal a double-count, not success.

That inverts how this task proves itself. Instead of chasing a percentage
against someone else's number, the residual becomes a **measurement of the
remaining gap**, and it should shrink to `fee_pool` alone as sources #2 and #3
land. When it does, the identity closes — and that is the completion signal.

For non-native assets there is no header equivalent, so those keep an
external cross-check; use raw XDR / the checkpoint snapshot (task 0502),
never Horizon.

### The continuous reconciliation check

Not a CI test — CI has no production data, and a one-shot verification would
have caught none of this year's regressions. It must be a **monitored
invariant**: both sides already live in ClickHouse (`total_coins` per ledger
once stored, the sum in `balance_aggregates`), so the residual is a query that
can run on a schedule alongside the existing aggregate refresh.

What makes it useful is that the residual should be **stable**, not zero.
Alert on unexplained movement, not on a threshold:

- residual jumps up → we started missing value (a write path dropped rows, a
  venue grew, ingestion fell behind);
- residual jumps down or goes negative → we are counting value that is not
  there (phantom balances, a double-count, ghosts).

Concrete evidence that this is not hypothetical: ~1.3M phantom XLM from
merged-account ghosts (task 0321) sat inside the published `total_supply`
undetected, and would have moved this residual the day it appeared.

### Scope added by the merge

- Store `total_coins`, `fee_pool`, `base_reserve`, `bucket_list_hash` on
  `ledgers` (`ALTER … ADD COLUMN … DEFAULT` first, then the writer — the
  ADR 0055 deployment order).
- Establish the identity above with each term measured, not asserted —
  including where contract-held XLM (SAC, re-keyed to native by ADR 0051)
  sits within it.
- Ship the residual as a monitored invariant with alerting on movement.
- Replace the Horizon acceptance criterion with "the identity closes".

### Notes carried over from 0505

- **Circulating supply is not total supply.** Our published 105,409,692,490
  XLM looked like a 2x error against the quoted ~50B until the ~55.4B in
  `GALAXYVOID…` — SDF's 2019 burn address — was verified by decoding its raw
  `AccountEntry` via `getLedgerEntries`: the chain agrees with us to the
  stroop. **Task 0342 owns the display convention**; this task only supplies
  the number that makes the distinction measurable.
- That episode is itself the argument for storing the oracle: answering
  "why 105B and not 50B" required external sources and hand-decoded XDR, and
  would have been one query if `total_coins` were stored.
- Source #2 (claimable balances) overlaps task **0504**, which found the same
  gap from the other direction — five ledger entry types parsed and never
  stored. Whichever runs first should claim the ingestion path; the other
  consumes it.

## Context

### The four sources Horizon aggregates

1. **Trustlines** — `account_balances_current.balance` per `(code, issuer_id)`.
   ✅ **DONE via 0331** (migrated into unified `balances`).
2. **Claimable balances** — `claimable_balances.amount` per `(code, issuer_id)`.
   Pre-claim hot wallet liquidity. ❌ NOT done (only ops parsed; no state table).
3. **Liquidity pool reserves** — NATIVE protocol AMM (`LiquidityPoolEntry`) reserves
   per asset participant. ❌ NOT done (not a contract; reserves in `pool_snapshots`).
   _(Soroban-DEX pool reserves are a CONTRACT holding → already captured by 0331 #4.)_
4. **SAC contract holdings** — Stellar Asset Contract instance balance held
   inside Soroban contracts (SAC entries in `contract_data`).
   ✅ **DONE via 0331** (contract-held type-0/1 re-key, ADR 0051).

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

- [~] Supply sums all 4 sources. **Trustlines + SAC/contract holdings DONE via 0331**
  (`sum(balances)` over the unified model — the dead `recompute_asset_aggregates`
  is superseded); **claimable + native-LP reserves remain**.
- [x] SAC contract holdings path — DONE via **0331 + ADR 0051** (contract-held type-0/1
      re-key; state-based, no separate aggregation table needed).
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

  - resolves SAC `contract_id → (code, issuer)`. The "non-standard storage-key
    layouts" line that scoped out 0138 is **disproven** (the standard `Balance(Address)`
    key was confirmed readable on a real vault token). **0331 lands the ingestion
    framework first; Phase 3 is a small extension on top, not a separate path.**
    0331 still owns type-3 (bespoke Soroban) supply+holders, out of this task's scope.

  * **Double-count trap:** the same classic asset is held two ways — as G-address
    trustline holdings (source #1) AND as Soroban `Balance` entries held by
    C-contracts via the SAC (source #4). Total supply is the ADDITIVE union:
    `trustline holdings + C-contract Balance holdings`. A trustline holder and a
    contract holder are distinct entries, so each is counted once — it's a sum of
    the two sources, NOT a subtraction of one from the other. Matches Horizon parity.

- **0194 deliberately deferred this.** From 0194 closing history (2026-05-XX):
  "Full Horizon-parity total_supply (LP reserves + claimable_balances + SAC
  contract holdings) explicitly deferred to Future Work." This task is the
  promised follow-up.
- **0197 doesn't catch this.** The audit checks non-NULL on sample queries, not
  value parity. Spawned independently per the 0197 punch list.
- **Sequencing.** Phase 1+2 can ship together (LP reserves + claimable
  balances) as a smaller PR. Phase 3 needs its own PR with the spike + design
  decision. Phase 4 parity check runs on top of Phase 3.
