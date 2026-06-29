---
id: '0331'
title: 'FEATURE: bespoke Soroban-token total_supply + holder_count via event-fold MV'
type: FEATURE
status: active
related_adr: ['0043', '0044']
related_tasks: ['0304', '0194', '0210', '0138', '0243', '0323']
tags: [clickhouse, soroban, assets, enrichment, effort-medium, milestone-2]
milestone: 2
links:
  - crates/db-clickhouse/schema/init.sql
  - crates/api/src/assets/queries_ch.rs
  - crates/xdr-parser/src/scval.rs
history:
  - date: '2026-06-26'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from 0304 metadata-validation session. Bespoke Soroban fungible
      tokens (asset_type=3) render "—" for Total supply + Holders in the assets
      list. Prototype validated 2026-06-26 that an event-fold over soroban_events
      reproduces on-chain total_supply() exactly (6/6 sample). New angle vs the
      scoped-out 0138 (storage-scan); covers the type-3 gap that 0194 (types 1,2)
      and 0210 (classic parity) leave open.
  - date: '2026-06-26'
    status: active
    who: karolkow
    note: Promoted to active to start implementation.
---

# FEATURE: bespoke Soroban-token total_supply + holder_count via event-fold MV

## Summary

Surface `total_supply` and `holder_count` for **bespoke Soroban fungible tokens**
(`asset_type = 3`) in the assets list/detail — today `—` for every Soroban token.
The trigger is type-3, but the chosen fix is the **fundamental balance model**
(Option C): a single unified per-holder `balances` table from ledger STATE, raw+decimals
representation for all asset types. (Original event-fold plan REFUTED — see CURRENT PLAN.)

> ⚠️ The H1/filename still say "event-fold MV" (historical). The actual approach is
> ledger-state + Option C — see **CURRENT PLAN** below; everything under "DECISION",
> "Implementation", "Findings", and "Implementation Plan (SUPERSEDED)" is trail.

## CURRENT PLAN 2026-06-29 (karolkow) — Option C unified balances (authoritative)

### Goal
One unified per-holder balance model for ALL asset types (Option C), raw+decimals
everywhere. type-3 was the trigger; the fix is the fundamental balance representation.

### Confirmed decisions
- **Mechanism = ledger STATE** (`ContractData Balance(Address)` entries), NOT event-fold
  (refuted: vault/rebasing drift; 54% non-SEP-41 events — see Findings).
- **Architecture = Option C**: unified `balances(holder_id, asset_id, amount, version)`,
  not separate per-type tables. (Reworks the separate `soroban_token_balances` from the
  earlier commits into this.)
- **Representation = raw `Int128` + `decimals`** (from asset/metadata), scaled at READ.
  Everywhere — classic migrates off `Decimal128(7)`. Universal fixed-point pattern.
- **Holder dimension** `addresses(id Int64, strkey String, kind Enum: account|contract|…)`
  — single surrogate over any `ScAddress`. Start account+contract (YAGNI on the rest).
- **Asset surrogate**: add a single `assets.id Int64` (assets is composite-keyed today;
  `balances.asset_id` needs ONE column — mirrors `accounts.id`/`soroban_contracts.id`).
- **Supply = instance `TotalSupply` key** (extend the 0297 instance-storage extraction;
  archival-proof), fallback Σ entries for plain SEP-41. NOT event-sum (vaults drift).
- **Holders = `countIf(amount > 0)`** over `balances`.
- **Seed = RPC snapshot** (holder set from `soroban_events` → batched raw
  `getLedgerEntries`, 99.69% readable incl. archived). **NO ledger reprocess** — proven
  100% by an independent adversarial agent; matches the user's preference.
- **SAC (type-2)**: keep two rows but compute REAL wrapped holders (option b) — coord 0210.
- **Empty tokens** (64% of type-3, no events): `—` is correct, not a coverage gap.

### Non-blockers resolved
- **0198 is a POSTGRES task** (Seq Scan on the PG `account_balances_current`,
  `crates/api/src/accounts/queries.rs`), NOT the CH path — **no collision with Option C**.
  Action: verify accounts is served from CH (PG path dead) → **archive 0198 as obsolete**.
- **9% type-3 missing decimals** = 386 tokens with NO on-chain `METADATA` struct
  (non-standard tail) → fall back to 7. Not a backfill gap (data absent).

### Sequence (no 0198 gate)
1. `addresses` dimension (account+contract)
2. `assets.id` surrogate
3. unified `balances` table (raw+decimals)
4. type-3 → `balances` (repoint persist off `soroban_token_balances`; drop that table)
5. aggregate + read over `balances`, supply via `TotalSupply` key
6. **migrate classic** `account_balances_current` → `balances` (`Decimal128(7)`→raw) —
   touches CH account-detail + aggregate + frontend; the one higher-risk step (NOT 0198)
7. **RPC-snapshot seed** bin + `TotalSupply` extraction (extend 0297)
8. docs (ADR 0032) + frontend amount rendering (coord 0257/0304)

### Already built (earlier commits — reused / reworked by C)
- `extract_soroban_token_balances` (ContractData Balance parser) — **reused** for live
  holder ingestion.
- `soroban_token_balances` table + persist + `soroban_asset_aggregates` + read coalesce —
  **reworked** into the unified `balances` by steps 3-6.

### Open risks
- **TTL eviction handling**: `removed → 0` must NOT zero an archived-but-positive balance
  (raw reads still return archived values). Verify how eviction appears in LedgerEntryChanges.
- value-shape: bare `i128` confirmed 4/4 type-3; struct shape is SAC (type-2).

## Prior art / why this is a distinct task

- **0194 (completed)** populates `total_supply` + `holder_count` for **asset_type
  1 & 2** (classic credit + SAC) by summing `account_balances_current`
  (`asset_aggregates_mv WHERE asset_type IN (1,2)`). **Type 3 is explicitly not
  covered** — bespoke Soroban tokens have no trustlines.
- **0210 (backlog, classic-parity BUG)** extends classic `total_supply` to 4
  Horizon sources; its **Phase 3 ("SAC contract holdings from `contract_data`")**
  overlaps mechanically — the event-fold here is a **better mechanism for that
  Phase too** (standardised events vs a contract-storage scan). 0210 also puts
  `holder_count` parity **out of scope**; this task owns holders for type 3.
  → cross-linked; a note added to 0210.
- **0138 (archived — SCOPE-OUT)** tried exactly this (Soroban token balances) via
  a **`contract_data` storage scan** and was abandoned because **token storage
  key layouts are non-standard** (per-contract `Balance(addr)` schemas). The
  event path **supersedes that approach** — SEP-41 `transfer`/`mint`/`burn` topics
  ARE standardised, so identification is reliable.

## Context

- Soroban token **balances** live in per-holder **persistent `ContractData`**
  (`Balance(addr)` keys), not in `account_balances_current` — so the existing
  aggregate can't see them. (Token **metadata** lives in the single
  `ScContractInstance.storage` map — that is what 0297/0304 extract; balances are
  a different, sprawling storage tier — hence the asymmetry.)
- SEP-41 events are already ingested **decoded** in `soroban_events` (`topics_xdr`
  / `data_xdr` are tagged JSON, not raw XDR). The NFT side already folds
  `mint`/`transfer`/`burn` the same way (`nft_reparse.rs`).

## Validation (prototype, 2026-06-26)

Event-fold `Σmint − Σ(burn+clawback)` matched on-chain `total_supply()` **exactly**
on every sampled token that exposes the getter: nk_ustry, native-USDC-RAUM-LP,
PPRIME-USDC-SOROSWAP-LP, SMOL, AQUA-XTAR-SOROSWAP-LP (264575131),
LIBRE-oUSD-SOROSWAP-LP (487976433). Method = `simulate total_supply()` via
py-stellar-sdk against mainnet RPC. Several tokens (GIGGLE, NRX, pool shares)
**revert `total_supply()` on-chain** — the fold supplies a number the getter can't,
which is part of the value.

## DECISION 2026-06-29 (karolkow) — PIVOT: ledger-STATE, not event-fold

**The event-fold premise (incl. the "6/6 exact" prototype above and the whole
event-fold Implementation Plan + Analysis below) is REFUTED. Chosen mechanism:
capture per-holder Soroban balances from `ContractData` `Balance(Address)` ledger
entries — the SAME way trustlines feed `account_balances_current` — and aggregate
supply/holders uniformly. Everything below this section is kept as historical trail.**

### Why the fold was refuted (measured on prod via `stellar` RPC, 2026-06-29)

- On-chain `total_supply()` (storage) vs folded, 3/3 tokens with a getter MISMATCH:
  MERU 126.7B vs **63.8B** (fold ×0.5); eurSAFO 39.4B vs **31.3B**; EUTBL 40.8B vs **39.2B**.
  Per-holder also off (MERU balance(GDPY…) chain 800,009,446,178 vs folded 988,468,658,323).
- **54% of type-3 token events have NULL `signature`** (1,027,142 / 1,885,974) — bespoke
  app events, not SEP-41. MERU mints shares via `DeFindexVault/deposit` (amount buried as
  `df_tokens_minted` in a nested map), NOT `mint`. eurSAFO/EUTBL are yield-bearing
  (rebasing) — balances change with **no event at all**. The fold cannot see any of these,
  and it's unbounded: every protocol invents its own event vocabulary.
- The fold reconstructs state from an incomplete history. Reading the ledger entry IS the
  state. The standard `Balance(Address)` ContractData key was confirmed readable on MERU
  (a non-SEP-41 vault token): raw entry value 800,009,446,178 = `balance()` exactly.

### Why this isn't a protocol asymmetry (the inherited assumption, refuted)

- The xdr-parser ALREADY decodes `ContractData` balance entries
  ([ledger_entry_changes.rs:189](../../../crates/xdr-parser/src/ledger_entry_changes.rs)).
  [state.rs:66](../../../crates/xdr-parser/src/state.rs) drops them
  (`if !is_contract_instance_key(&change.key) continue`). The data exists; it's filtered out.
- 0138's "non-standard storage keys" verdict is an **unmeasured assertion** — 0138 was
  actually scoped out for *documentation-scope* reasons (account page = classic-only). The
  standard key works. **Reopen 0138 with the correct reasoning (TTL/archival + doc-scope).**

### Chosen design — option A now, option C later

- **Option A (NOW): parallel `soroban_token_balances (contract_id, holder_id, balance,
  last_updated_ledger)`**, `ReplacingMergeTree(last_updated_ledger)` — a mirror of
  `account_balances_current`, written by a new `extract_soroban_token_balances()` in
  state.rs (filter `key == Vec[Symbol("Balance"), Address]`). Supply/holders aggregate
  = `sum`/`countIf(>0)`. **(As-built correction — see Implementation: a SHARED
  `asset_aggregates` proved impossible — classic is `Decimal128(7)`, type-3 is raw
  `Int128`/arbitrary decimals — so a PARALLEL `soroban_asset_aggregates`, coalesced at
  read, not a shared table.)** Read path mirrors the classic join. Classic hot path
  untouched (`account_balances_current` is mid-perf-surgery under task 0198).
  - **Bonus, ~free:** account portfolio ([fetch_balances](../../../crates/api/src/accounts/queries_ch.rs))
    can finally show Soroban holdings (today invisible); top-holders becomes possible.
  - **Holders span two namespaces:** 34% of type-3 holders are CONTRACTS (C-addresses),
    not G-accounts — hence a parallel table (not shoved into account-keyed
    `account_balances_current`), and the motivation for option C.
- **Option C (FUTURE): unified identity dimension** — see
  [notes/I-unified-identity-dimension.md](notes/I-unified-identity-dimension.md).

### Honest costs / open items for option A

1. **TTL / archival.** Soroban persistent entries archive if TTL lapses → a dormant
   holder's `Balance` entry can vanish from live state → summing entries may under-count
   vs `total_supply()`. Mitigation: read supply from the instance `TotalSupply` storage key
   (always-live, like METADATA which 0297 already extracts) + holders from Balance entries
   (handle restore/archival). **This is the real version of what 0138 half-sensed.**
2. **Backfill.** Seed existing balances: reprocess historical `ContractData` changes
   (backfill-runner) OR RPC-snapshot baselines + live entry-change ingest forward.
3. **Key coverage.** Standard `Balance(Address)` confirmed for MERU; MEASURE % across all
   4,065 type-3 tokens at implementation (most soroban-sdk/OZ conform; quantify the tail).

### Acceptance criteria — SUPERSEDED, rewrite on implementation

The event-fold acceptance criteria below are obsolete. New ACs (storage-state) to be
written: `soroban_token_balances` populated from ContractData Balance entries;
supply/holders match on-chain getters on a ≥10-token sample incl. a vault (MERU) and a
rebasing token (eurSAFO/EUTBL); TTL/archival handled; account portfolio shows Soroban
holdings; docs (ADR 0032) + reopen 0138.

---

## Implementation 2026-06-29 (karolkow) — built, all green, uncommitted

Storage-state path, TDD throughout. Suites green: xdr-parser 280, db-clickhouse 63,
indexer 40, api 43; clippy clean; api-types regenerated + `check-generated` clean.

**Write pipeline:**
- `xdr-parser::extract_soroban_token_balances` — `ContractData` `Balance(Address)` →
  `(contract_id, holder, balance i128, ledger)`; `removed`→0; skips `state` pre-image,
  non-balance keys, and non-bare-i128 values (4 tests).
- Schema `soroban_token_balances` RMT(`last_updated_ledger`); persist `build_soroban_balance_rows`
  (1 test) + `StageInputs`/`StagedLedger`/writer wiring; live indexer (`process.rs` →
  `handler`) + **backfill (`sink.rs`) for free** (shared `ParseOutput`).

**Aggregate + read:**
- `soroban_asset_aggregates` table + refreshable MV (raw `Int128`).
- `assets/queries_ch.rs` (both detail + list): `LEFT JOIN soroban_asset_aggregates ON
  asset_type=3 AND contract_id`; `coalesce` classic+type-3 supply/holders. DTO doc +
  api-types updated.

### Emerged decisions
1. **Parallel `soroban_asset_aggregates`, NOT a shared `asset_aggregates`** — type wall:
   classic `Decimal128(7)` (pre-scaled) vs type-3 raw `Int128`/arbitrary decimals (PIKA
   43224 overflows any Decimal). Coalesce at read instead. Classic path 100% untouched.
2. **`holder` stored as raw StrKey String** (not surrogate) — avoids 0323 one-way-hash
   loss; ~48k rows; display-ready for portfolio/top-holders.
3. **type-3 `total_supply` returned RAW** (frontend scales by `decimals`); classic stays
   pre-scaled. Documented in the DTO.
4. **Supply = `sum(Balance entries)`** for the MVP — see TTL caveat in follow-ups.

### Validation (prod, `stellar` RPC, 2026-06-29)
- Standard `Balance(Address)` key + **bare i128** confirmed on 4/4 sampled type-3 tokens
  (MERU vault, EUTBL, KLT, eurSAFO) — extractor covers them. Struct-value shape is
  SAC-only (type-2, out of scope). Spent-down holders keep a `0`-valued entry (→ the
  `removed`→0 + `balance>0`-filter both matter).

### Follow-ups (not done; spawn on develop)
- **TTL/archival precision** — a dormant holder's `Balance` entry can archive → summing
  entries under-counts vs `total_supply()`. Read supply from the instance `TotalSupply`
  key (always-live, like METADATA/0297); holders from `Balance` entries + restore handling.
- **Deploy + backfill + prod-validate** — `soroban_token_balances` doesn't exist on prod;
  deploy indexer, run backfill (after live catch-up per the Notes gate), validate supply
  vs on-chain getters on ≥10 tokens incl. a vault + a rebasing token.
- **Frontend type-3 amount rendering** (raw → scale by `decimals`) — coordinate with 0257/0304.
- **0210 Phase 3** — SAC `BalanceValue` struct decoder rides this same ingestion (cross-linked).

## Findings 2026-06-29 (two independent agents) — RPC-snapshot seed viable at 100%

Two unbiased agents (no access to this task) audited the data. Net:

**A 100% backfill does NOT require ledger reprocess — an RPC snapshot suffices.**
(Corrects the earlier "reprocess required" assumption.)
- **64% of type-3 (2623/4071) have no events and are EMPTY on-chain** (0 `Balance`
  entries, never minted) → `—` is correct, not a coverage gap. The meaningful set is
  ~1448 (1409 real + 39 oracle/upgrade-only empties).
- **Holder set is fully recoverable from `soroban_events`** (G/C addresses in topics/data
  — even the DeFindex vault emits standard `mint` with the recipient in a topic).
- **Raw `getLedgerEntries` returns values for TTL-archived entries: 642/644 = 99.69%**
  across 40 stale tokens (`liveUntil=0`). `invoke` fails on archived; raw read does not.
- **Supply is best read from the instance `TotalSupply` i128 key** (raw, archival-proof,
  same instance-storage path as METADATA/0297). DeFindex `TotalSupply`=126678419935462 ==
  `total_supply()`. For DeFindex-class vaults, Σ(per-holder events) DRIFTS (`report`/fee
  mints move shares without standard events) — so supply must NOT be summed from events;
  per-holder STATE (entries) + the `TotalSupply` key are correct.

### Refined architecture (post-findings)
- **Holders** ← per-holder `Balance(Address)` entries. LIVE: indexer ContractData parser
  (already built). SEED: RPC snapshot (holder set from events → batched raw
  `getLedgerEntries` ≤190 keys/req → nonzero count).
- **Supply** ← instance `TotalSupply` key (extend the 0297 instance-storage extraction);
  fallback Σ entries for plain SEP-41 without the key. NOT event-sum.
- **Seed = RPC snapshot** (light; no galexie reprocess). Live stays indexer-driven.
- Decimals already on prod (0297): 3840/4222 type-3 have them, **2522 ≠ 7** (variable
  confirmed); ~9% missing → fall back to 7.

### Recovery pipeline (seed, no reprocess)
1. Holder candidates ← scan G/C strkeys in the token's `soroban_events` topics+data.
2. Batched raw `getLedgerEntries` on `Balance(addr)` keys → drop zero/absent →
   `holder_count` = nonzero count.
3. `total_supply` ← instance `TotalSupply` key, else `total_supply()`, else Σ balances
   (exact only for non-vault SEP-41). Empty tokens render `—`.

## Implementation Plan (SUPERSEDED — event-fold; historical trail)

### Event shapes (confirmed)

- `topics_xdr` = JSON array `[{"type":"sym","value":"transfer"},{"type":"address","value":"…"},…]`.
  Event name = element 1 `value`.
- `transfer` (3 topics): `[transfer, from, to]`. `mint`: `[mint, admin, to]` (3) **or**
  `[mint, to]` (2). `burn`: `[burn, from]`. (`clawback` rare.)
- `data_xdr` amount = **two shapes**: `{"type":"i128","value":"<dec>"}` (most) **and**
  `{"type":"map","value":[{"key":{…amount},"value":{"type":"i128","value":"<dec>"}}]}`
  (~29832 mints). Extractor MUST handle both. Use **`Int128` / `Decimal256`** (NOT
  `Decimal128(7)` — overflows on 0-dec / huge-supply tokens, e.g. PIKA decimals=43224).

### Aggregation (CRITICAL construction — see CH gotcha #19b)

- **Supply** = `sumIf(amt, ev='mint') − sumIf(amt, ev IN ('burn','clawback'))` grouped by
  `contract_id` (numeric), **with ZERO joins over the event stream**.
- **Holders** = per-address fold: recipient (`+amt`) = **last** topic for `transfer`/`mint`;
  sender (`−amt`) = topic 2 for `transfer`, last topic for `burn`/`clawback`. Then
  `balance = Σdelta` per `(contract_id, address)`, `holder_count = countIf(balance > 0)`.
- Materialise as `soroban_asset_aggregates {contract_id Int64, total_supply, holder_count}`
  (mirror `asset_aggregates_mv`). Decide refreshable-MV (full recompute, simple) vs
  incremental/scheduled based on `soroban_events` size/cost.
- **Resolve `id → strkey/symbol/decimals` SEPARATELY** at read time on the small
  per-page set (GROUP-BY-collapsed CTE per gotcha #13/#19) — never fold + join in one
  full-table query (re-introduces the RMT ×N row multiplication; see #19b — this is the
  ×100 the prototype hit, not a fold error).

### Read path

- `assets/queries_ch.rs`: for `asset_type = 3`, LEFT JOIN `soroban_asset_aggregates` on
  the numeric `contract_id` over the page set. `AssetRow.total_supply` / `holder_count`
  already exist (currently NULL for Soroban) — just populate.

## Acceptance Criteria

- [ ] `soroban_asset_aggregates` (MV/table) computes supply + holder_count from
      `soroban_events` with **no joins over the event stream** (gotcha #19b).
- [ ] Amount extractor handles `i128`-direct **and** `map{amount}` shapes; `Int128`/`Decimal256`.
- [ ] Holders per-address fold covers `transfer`/`mint`/`burn`/`clawback`.
- [ ] Assets list + detail surface supply + holders for `asset_type = 3` (no `—` where
      data exists); id→symbol/decimals resolved on the small per-page set.
- [ ] Supply matches on-chain `total_supply()` on a ≥10-token sample **including** a
      map-shape mint token and the AQUA-XTAR / LIBRE ×100-regression cases; holders spot-checked.
- [ ] Caveats documented: conformant tokens only (non-emitters stay `—`); non-standard
      event shapes logged, not silently mis-summed.
- [ ] **Docs updated** (ADR 0032): `docs/architecture/database-schema/**` (new MV/table) + `endpoint-queries` assets SQL.
- [ ] **API types** — N/A expected (`total_supply`/`holder_count` already in the assets
      response, currently NULL; populating ≠ schema change). Confirm no DTO change at PR time.

## Notes

- **Sequencing — do this AFTER live ingest resumes and catches up (`L_stop` → tip).**
  0331 is read-side (no conflict with ingest running, unlike the 0304 backfill which
  needed it stopped), but the fold reads `soroban_events`: the 0304/0281 window left a
  gap from ~`L_stop`, so deploying the MV before catch-up **under-counts** recent supply.
  The MV also stays fresh only while live ingest keeps writing events. No other blockers
  (assets `asset_type=3` read path already on `ch`).

- Reuses the validated CI check: sample tokens → `simulate total_supply()` (py-stellar-sdk)
  → compare vs the fold. Catches drift from missed/non-standard events.
- Mechanism is **also a candidate for 0210 Phase 3** (SAC contract holdings) — same
  event-fold instead of a `contract_data` scan. Coordinate if both land.

## Analysis 2026-06-29 (karolkow) — event-fold cost/0323 (SUPERSEDED by DECISION above)

> Kept for trail. The unified-MV-over-events design here is obsolete (event-fold refuted —
> see DECISION). The 0323-disjointness and `asset_type=3` scoping findings still hold and
> carry over to the storage-state approach (same type-3 contract set).


**Chosen approach: ONE shared `asset_aggregates` table + ONE refreshable MV, not a
parallel pipeline.** Mirror the disjoint key the `assets` table already uses
(type 1,2 → code+issuer; type 3 → contract_id). The MV body becomes
`balances-branch (asset_type IN (1,2), contract_id=0) UNION ALL event-fold-branch
(asset_type=3 set, code='' issuer=0)`. Read path coalesces two guarded LEFT JOINs to
the SAME table (`asset_type IN (1,2)` on code+issuer; `asset_type=3` on contract_id) —
a 3-col equi-join breaks type-2 (SAC has contract_id in `assets` but the balance row is
keyed code+issuer/contract_id=0). No second table, no second MV, no API-types change
(`total_supply`/`holder_count` already in the response, NULL today).

**Cost — the "is the MV too heavy" Future-Work question is ANSWERED: no.** The 4.37B
`transfer` firehose is the SAC/DEX stream (type-2 / reserved-SAC addresses, see 0323),
NOT the bespoke tokens. Pre-filtering the fold to `contract_id IN (assets WHERE
asset_type=3)` collapses the input to **~647k events** (transfer 457,703 · mint 134,143
· burn 54,934). Measured on prod (`chq`, 2026-06-29): supply fold **0.574s**, holder
fold **1.873s** (1269 tokens, max 5,042 holders). Negligible on the existing 2-min
refreshable MV — same cadence, no incremental machinery needed.

**LOAD-BEARING: the fold MUST stay scoped to `asset_type=3`**, never a broad "any
contract emitting transfer" — a broad fold re-admits the SAC firehose → wrong numbers +
the ×N cost blow-up.

**0323 (SAC-as-asset depollution) does NOT affect this task — verified disjoint.**
0323 operates on SACs (`asset_type=2`): un-deployed SACs move out of `soroban_contracts`
into `assets` as type-2 rows. 0331 folds `asset_type=3` bespoke WASM tokens. Prod check
(2026-06-29): of the 4,065 type-3 assets, **0 are `is_sac=true`, all 4,065 are real WASM
(`contract_type=3`), 0 missing a contract row.** SACs are handled by the balance branch
(`asset_type IN (1,2)`), so 0323 growing the type-2 population never changes the type-3
fold input. Sequencing: disjoint tables (0323 = `soroban_contracts`/`nfts_pending`;
0331 = `soroban_events` read + new aggregate rows) → no new dependency; 0331 keeps its
own "after ingest catch-up" gate.

## Future Work

- ~~Refreshable-MV (full recompute) vs incremental aggregation — pick by `soroban_events`
  cost; spawn a follow-up only if the simple MV proves too heavy.~~ **RESOLVED 2026-06-29:**
  full recompute wins — pre-filtered fold is ~647k rows / <2s (see Analysis). No
  incremental machinery needed.
- Non-standard / non-conformant token event shapes (logged class) — separate follow-up if
  the residual is material.
