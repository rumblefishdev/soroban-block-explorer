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
- **SAC (assets type-2) computed INDEPENDENTLY — IN THIS TASK, but SCHEDULED LAST (step 9),
  spike-gated, does NOT block the core.** (Too many unknowns — struct value, double-count,
  built-in SAC — to gate the core type-3 + unified work on it.) Today both the classic
  (assets type-1) and SAC (assets type-2) rows share
  the one `(code, issuer)` aggregate → identical supply/holders. Instead compute the SAC
  on its own and VERIFY against classic:
  - **holders(SAC)** = classic trustline holders (G-accounts, `account_balances_current`)
    ∪ SAC contract holders (C-addresses, the SAC's `ContractData Balance` entries). **Will
    DIFFER from classic** (adds contract holders). Needs the SAC value decoder (see problems).
  - **supply(SAC)** = the asset's total issuance → SHOULD equal classic trustline sum;
    **verify equality, don't copy**.
  - **Problems to handle (flagged):**
    1. **SAC balance value is a STRUCT** (`BalanceValue{amount,authorized,clawback}`), NOT
       bare `i128` → the type-3 parser (`decode_scval_i128`) skips it. Need a struct decoder.
    2. **Double-count risk on supply**: contract-held balances moved FROM accounts within the
       same fixed issuance — summing trustlines + contract-held double-counts. Correct
       independent supply = the asset total (≈ classic sum); the verify is near-tautological
       unless read from the SAC's own getter.
    3. **SAC is BUILT-IN (no Wasm)** → CLI can't `invoke ... total_supply` by name (spec
       unavailable; confirmed failing). On-chain verification needs a raw-entry/known-key
       spike. This is the part that overlaps 0210 Phase 3 — now pulled into 0331 per decision.
- **Empty tokens** (64% of type-3, no events): `—` is correct, not a coverage gap.

### Gotcha — two different `asset_type` enums (load-bearing)
- `account_balances_current.asset_type` = **Horizon** (0 native, 1 alphanum4 [code ≤4],
  2 alphanum12 [code 5-12]) — verified by code length on prod (type-1 len 1-4, type-2 len 5-12).
- `assets.asset_type` = **project** (0 native, 1 classic_credit, 2 sac, 3 soroban).
- The classic aggregate `WHERE asset_type IN (1,2)` over `account_balances_current` = all
  classic credit (both code lengths) — correct. Do NOT confuse with assets' SAC=2.

### Surrogate keys — confirmed nature (for `assets.id`)
All existing surrogates are `cityhash64` of the natural key: `account_id=hash64(G-strkey)`,
`contract_id=hash64(C-strkey)`, even `transaction_id=hash64(real 32B tx hash)` (a derived
surrogate, not the raw hash). `assets` never had a self-surrogate because nothing referenced
an asset as ONE unit — other tables reference its COMPONENTS (`issuer_id`, `contract_id`).
`balances.asset_id` is the first to need it. Add `assets.id = hash64(canonical identity)`
(native→sentinel; classic→`CODE:ISSUER`+type marker; soroban→contract strkey) — same pattern
as `accounts`. Deterministic = replay-idempotent (the reason hash, not a counter).

### Non-blockers resolved
- **0198 is a POSTGRES task** (Seq Scan on the PG `account_balances_current`,
  `crates/api/src/accounts/queries.rs`), NOT the CH path — **no collision with Option C**.
  Action: verify accounts is served from CH (PG path dead) → **archive 0198 as obsolete**.
- **9% type-3 missing decimals** = 386 tokens with NO on-chain `METADATA` struct
  (non-standard tail) → fall back to 7. Not a backfill gap (data absent).

### Sequence (no 0198 gate) — status as of 2026-06-29

Code for steps 1–7 is on branch `feat/0331` (NOT yet merged / deployed). Remaining:
frontend (step 8), SAC (step 9), and prod run + validation of the seed.

1. ✅ `addresses` dimension (account+contract)
2. ✅ `assets.id` surrogate
3. ✅ unified `balances` table (raw+decimals)
4. ✅ type-3 → `balances` (repointed persist off `soroban_token_balances`; dropped that table)
5. ✅ aggregate + read over `balances`
6. ✅ **migrate classic** `account_balances_current` → `balances` — **6a/6b/6c done**;
   **6d (retire legacy classic path) deferred** → see Post-task follow-ups.
7. ✅ **RPC-snapshot seed** bin (`backfill-runner balance-seed`) + `TotalSupply`-key supply
   (commits `f1f0a66f` seed, `1406ea5b` supply). ◻ Remaining: prod run under the catch-up
   gate + ≥10-token on-chain validation (vault + rebasing).
8. ✅ docs (ADR 0032 — clickhouse-pilot §4f + indexing-pipeline §6.2) + **frontend amount rendering
   done** (`scaleByDecimals` in libs/ui): AssetsTable + AssetSummary scale `total_supply`, and
   AccountBalances scales the account portfolio. (All API modules are `ch` in prod — verified
   compute-stack.ts — so the portfolio is raw; PG is dead, so the legacy-pre-scaled concern is moot.
   Also cut the dead PG balance path: `balance-seed` is CH-only, accounts `fetch_balances` PG path removed.)
9. ⏸ **(LAST, spike-gated) SAC type-2 independent** — NOT started. Spike (SAC total_supply vs
   classic + how contract-holders are stored) → if clean: `BalanceValue` struct decoder +
   trustline∪contract holders + verify supply==classic. If messy: defer to 0210/0323. Does NOT block 1-8.

### Already built (earlier commits — reused / reworked by C)
- `extract_soroban_token_balances` (ContractData Balance parser) — **reused** for live
  holder ingestion.
- `soroban_token_balances` table + persist + `soroban_asset_aggregates` + read coalesce —
  **reworked** into the unified `balances` by steps 3-6.

### Open risks
- **TTL eviction handling**: `removed → 0` must NOT zero an archived-but-positive balance
  (raw reads still return archived values). Verify how eviction appears in LedgerEntryChanges.
- value-shape: bare `i128` confirmed 4/4 type-3; struct shape is SAC (type-2).

### Why not event-fold — re-litigated 2026-06-29 (staleness-IMMUNE proof + external analysis)

An external analysis (Stanisław, via his own Claude session) proposed REVERTING to the original
event-fold (refreshable MV over `soroban_events`, supply = Σmint−Σburn), arguing no backfill is
needed — only ingest catch-up. Re-examined in full. **Verdict: event-fold stays rejected as the
supply source / sole truth; his backfill mechanics are correct but answer the wrong question.**

**Confound found + acknowledged.** The original DECISION refutation numbers (MERU 126.7B vs 63.8B
etc.) compared a STALE fold against CURRENT mainnet: prod CH `soroban_events` is ~190,480 ledgers
/ ~12 days behind tip (measured 2026-06-29: max `ledger_sequence`=63,059,708 vs mainnet 63,250,188).
Those magnitudes are confounded and do NOT by themselves prove the fold wrong (Karol caught this).

**New proof — measures event VOCABULARY, not freshness → unaffected by the lag (prod CH, 2026-06-29):**
- **65.4% of type-3 event volume is fold-blind** — 1,266,280 of 1,936,527 events have NULL or
  non-SEP-41 signatures. Fold keys only on transfer/mint/burn/clawback = 34.6%.
- **179 type-3 tokens emit events but ZERO standard SEP-41** → fold sees nothing → would report
  supply=0 / holders=0 ("—") despite real on-chain activity (~12% of the ~1448 meaningful set).
- **14 tokens emit transfer/burn but ZERO mint** → supply entered via a non-folded path →
  `Σmint−Σburn` is zero/negative while the token circulates → fold is mathematically inconsistent.
- Value-moving events the fold misses, present in the data: `deposit` (30 tokens), `transfer_event`,
  `admn_mnt`, `vault_deposit`/`VaultDeposit`; rebasing/yield: `accrue_interest` (3), `EpochProcessed`,
  `vaulted_event` (balance/supply grows with NO transfer event at all).

**Why fresh data does NOT fix it (deductive).** Catch-up adds more events of the same distribution;
it cannot turn a `deposit` into a `mint`, nor emit a transfer for silent yield accrual. Freshness
fixes the confound (stale-vs-current magnitudes), NOT the structural gap. Fold is correct for
conformant SEP-41 tokens; permanently wrong for vault / rebasing / custom-mint tokens.

**Stanisław's analysis — what's right, what it misses.**
- RIGHT (and useful to US): no historical backfill (0228 filled events from ~50.46M, 9.27B rows —
  verified `min(ledger_sequence)=50,457,424`); **no reparse** — verified in code: the silent-drop
  bug was only in `nft.rs::detect_nft_events`; the generic event stream serializes verbatim
  (`event.rs:143-144`, all topics+data via `scval_to_typed_json`, no shape-drop). His completeness
  proof BENEFITS our seed: we use events only to ENUMERATE holders (G/C addresses in topics), then
  read VALUE from RPC — so the 179 fold-blind tokens are still discoverable by our pipeline.
- MISSES: he answered "does event-fold need a backfill" (no, + catch-up) — NOT "does event-fold
  produce correct numbers." His MV pattern + wording are from the SUPERSEDED event-fold plan, likely
  written without the DECISION/pivot in view. "Wait for catch-up → numbers correct" holds for
  conformant tokens, fails for the 65%-blind volume above.

**Net effect on this task: architecture + sequence UNCHANGED.** Supply = `TotalSupply` key (NOT
fold). Holders = `Balance` entries (event SET + RPC VALUE); fold acceptable only as an
approximation for conformant tokens if ever needed. Reparse open-question now CLOSED (below).

### Step 7 — design + open questions (NEXT; 0 commits; reconstructed 2026-06-29 after session loss)

> The 06-29 research session that produced this was accidentally deleted and is
> unrecoverable. This block + the commit messages are the only surviving record of the
> step-7 reasoning — keep it current so it survives the next session loss.

**Status — core BUILT 2026-06-29 (TDD).** `backfill-runner balance-seed` (CH-only). Pure units
`rpc_snapshot::balance_ledger_key` + `decode_balance_entry` unit-tested (RED→GREEN); orchestration
`balance_seed.rs` (candidate scan → RPC `getLedgerEntries` → decode → upsert `balances` +
`addresses`) mirrors `upgradeable_backfill`; candidate SQL validated on prod CH (MERU = 5974 holder
candidates). Supply = authoritative instance `TotalSupply` key (RPC-probed: i128 on MERU/
USDC-style, absent on plain soroban-sdk → fallback `Σ balances`), written to
`soroban_token_supply` and coalesced over the sum at read. Remaining before prod: run under
the catch-up gate + ≥10-token on-chain validation.

**Scope:** seed existing holders + pick the supply source. The live indexer only captures
NEW `ContractData` changes; step 7 backfills the balances that already exist on-chain.

**Reuse — NOT from scratch.** `crates/backfill-runner/src/rpc_snapshot.rs` (built for the
0320/0326 WASM-upgrade backfill, untouched by 0331) already has the `getLedgerEntries`
`RpcClient` + batching + ledger-key builders (account / trustline / contract_code) + snapshot
decoders. Step 7 = **add** a `Balance(Address)` ContractData key builder + a contract-instance
key builder, **add** an i128-balance decoder + a `TotalSupply` instance-key decoder, then wire
a seed bin on top. Do not rebuild the client.

**Pipeline (from Findings):** holder candidates ← G/C strkeys in the token's `soroban_events`
topics+data → batched raw `getLedgerEntries` on `Balance(addr)` keys → drop zero/absent →
`holder_count` = nonzero count. supply ← instance `TotalSupply` key, else `total_supply()`,
else Σ balances (exact only for non-vault SEP-41). Empty tokens (64%) → `—`.

**Precondition — catch-up gate (sequencing, NOT code).** The seed reads CURRENT mainnet state via
RPC, but the live balance path is fed by the indexer, which is ~12 days / 190,480 ledgers behind
tip (2026-06-29). Run the seed only once the indexer is at ~tip — else the seed is correct at
seed-time but decays stale until the lagging indexer reaches the seed ledger (~2 weeks). Manual
check: `SELECT max(ledger_sequence) FROM soroban_events` ≈ mainnet tip (same gate as the Notes
sequencing item). Optionally a guard in the seed bin refusing to run if the indexer is >N ledgers
behind. Not feature logic.

**Open questions — resolve before/at implementation:**
1. **`TotalSupply` key — RESOLVED 2026-06-29.** RPC-probed across classes: instance-storage
   `Symbol("TotalSupply")` i128 (MERU vault `126,717,554,425,310`; USDC-style: present; plain
   soroban-sdk: ABSENT → `Σ balances` fallback). Built: `instance_ledger_key` +
   `decode_total_supply` → `soroban_token_supply` table, coalesced over `balance_aggregates` at read.
2. **TTL / eviction in `LedgerEntryChanges` still OPEN** (see Open risks) — `removed→0` must not
   zero an archived-but-positive balance. Blocks both live and seed correctness.
3. **Batch cap:** this doc says ≤190 keys/req; `rpc_snapshot.rs` has its own constant — reuse it,
   don't introduce a third number.
4. **Reparse — RESOLVED 2026-06-29: NOT needed.** Verified in code (`event.rs:143-144` serializes
   topics+data verbatim; the silent-drop bug was nft-detector-only, `nft.rs::detect_nft_events`).
   `soroban_events` is a faithful holder-enumeration source; re-decoding recovers nothing. (Also
   confirms no historical event backfill — 0228 filled from ~50.46M, verified.)

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

## Acceptance Criteria (storage-state — rewritten 2026-06-29; replaces the event-fold list)

- [x] Unified `balances` populated from `ContractData Balance(Address)` ledger STATE
      (live parser + the `balance-seed` RPC snapshot) — NOT an event-fold.
- [x] `balance_aggregates_mv` computes `total_supply` (`sum`) + `holder_count`
      (`countIf(amount > 0)`) over `balances`, keyed by `assets.id`.
- [x] type-3 supply read from the authoritative instance `TotalSupply` key, coalesced
      over the sum (`soroban_token_supply`); plain tokens fall back to `Σ balances`.
- [x] Assets list + detail surface supply + holders for `asset_type = 3` (read coalesce).
- [x] Balance + supply decode unit-tested; non-bare-`i128` (SAC struct) + non-`Balance`
      keys skipped, never mis-summed.
- [x] **Docs (ADR 0032):** `clickhouse-pilot.md §4f` (balance family) + `indexing-pipeline §6.2`
      (balance-seed). **API types:** regen produced no diff (SQL-only change) — confirmed.
- [ ] **Prod validation:** run `balance-seed` under the catch-up gate; supply/holders match
      on-chain getters on a ≥10-token sample incl. a vault (MERU) + a rebasing token (EUTBL/eurSAFO).
- [x] **Frontend** raw-amount rendering (`scaleByDecimals` by `decimals`) — AssetsTable + AssetSummary
      (`total_supply`) **and** AccountBalances (account portfolio). PG balance path cut (CH-only).
- [ ] **(deferred)** SAC type-2 independent supply/holders — step 9, spike-gated.

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

## Post-task follow-ups (deferred — record only, NOT yet spawned)

- **6d — retire legacy classic balance storage.** Steps 6a/6b/6c shipped the classic→unified
  `balances` migration but left the old path in place during transition: `account_balances_current`
  is still dual-written (6a), and the legacy PG `fetch_balances` path is still kept (6c). 6d =
  drop that legacy code/table once the unified `balances` model is prod-validated. Overlaps
  **0243** (PG→CH per-module migration). Deferred to a post-task follow-up; not spawned yet.
