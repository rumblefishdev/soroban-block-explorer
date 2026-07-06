---
id: '0357'
title: 'Asset-participation index re-model — native XLM first-class + complete per-asset activity (offers, all path-payment legs)'
type: FEATURE # fundamental data-model fix: schema + ingestion + XDR re-parse backfill + query rewrites
status: backlog
related_adr: ['0044', '0051'] # 0044 operations_appearances schema; 0051 SAC-as-facet / native surrogate convention
related_tasks: ['0348', '0331', '0334', '0243', '0333'] # 0348 = F2 origin; 0331/0334 = balances native-surrogate precedent; 0243/0333 = assets CH queries + bloom idx
tags:
  [
    'backend',
    'clickhouse',
    'data-model',
    'ingestion',
    'backfill',
    'effort-xlarge',
    'priority-medium',
    'epic',
  ]
links: []
history:
  - date: 2026-07-06
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0348 F2 investigation ("native XLM asset detail shows
      No transactions yet"). The investigation escalated into a full data-model
      audit + adversarial devil's-advocate pass against prod ClickHouse. Decision
      (karolkow): fix this FUNDAMENTALLY in its own task — no plasters, no
      hotfixes — accepting a full XDR-re-parse backfill if required. The stopgap
      "variant C" (native payments-only branch) built during the investigation
      was DELIBERATELY REVERTED so the fix is done once, correctly, here.
---

# Asset-participation index re-model

## Summary

`operations_appearances` (the inverted index that powers per-entity activity
lists) stores the ASSET dimension as a **single denormalised slot** per
operation row (`asset_code`, `asset_issuer_id`, `contract_id`), with exactly
**one row per operation**. That single-slot design cannot represent an
operation that touches more than one asset, and it models native XLM as
_absence_ (empty string / NULL) rather than a first-class key. This task
re-models asset participation as a proper **per-(operation, participating-asset)
index** — mirroring how accounts (`transaction_participants`) and pools
(`pool_ids Array`) are already modelled — so native XLM becomes a first-class
asset and every asset's activity list is complete.

This is a schema + ingestion + query-rewrite change with a **backfill that
re-parses operations from archived XDR** (the second asset leg was never stored
in ClickHouse, so it cannot be recovered by re-keying existing rows — the source
XDR is the only place both legs exist). Cost of the backfill is explicitly
accepted.

## Origin & calibrated thesis (post devil's-advocate)

Discovered chasing 0348/F2. Initial framing ("single-slot fundamentally breaks
per-asset lists") was **stress-tested against prod CH and partly walked back** —
recorded honestly here so we build the right thing:

- **Issued-asset lists are NOT broken.** The top-5 issued assets by volume
  (yXLM, USDC, AQUA, HELIX, SSLX) each surface **4 op types `[1,2,13,6]`** =
  Payment + PathPayment(receive) + PathPayment(send) + ChangeTrust. Users DO
  see varied, useful activity.
- **The acute bug is narrow:** only **native XLM** was fully empty
  (`GET /assets/native/transactions` → `{"data":[]}`), because native is the
  only asset with no positive key. Confirmed live + at `queries_ch.rs`
  (early-return when neither classic code+issuer nor contract identity present).
- **The systematic completeness gap** (affects all assets): **offers carry zero
  asset identity** and **path-payments store at most one of their legs**.

So this task = **fundamental completeness + native-first-class**, not "repair a
broken feature". Scope chosen deliberately (karolkow): do it right at the root.

## Root cause (measured on prod, 6.4 B rows)

`operations_appearances`: `ORDER BY (ledger_sequence, transaction_id,
application_order)`, `ReplacingMergeTree` → **exactly one row per operation**
(verified: rows == distinct `(ledger,tx,application_order)`, zero dups). Asset
columns are a single slot. Two failure modes:

1. **Many-to-many collapsed to one slot** — an op with N assets keeps ≤1.
2. **Native modelled as absence** — `asset_code=''`, `asset_issuer_id` NULL,
   `contract_id` NULL (xdr-parser persists native with NULL code/issuer), so it
   is indistinguishable from "op with no single asset" and is skipped by every
   identity filter.

Contrast — the codebase already has the RIGHT patterns for the other dimensions:
`accounts` → `transaction_participants` (per-(account,tx), leading key);
`pools` → `pool_ids Array` + bloom skip-index; `balances/supply/holders` →
native as a POSITIVE surrogate `cityhash64("native")` (task 0331 / ADR 0051).
See [[project_native_two_conventions]]. Asset participation on
`operations_appearances` is the one dimension still on the broken convention.

## Findings (full audit, ranked)

| #   | Finding                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | Severity | Evidence                                                                                                                                                                                                                                                                                                               |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F-A | **Asset single-slot on `operations_appearances`** — native empty + multi-asset legs lost. THE core.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | HIGH     | `queries_ch.rs fetch_transactions` early-return; SQL doc `10_get_assets_transactions.sql:61` "out of scope"; prod: offers 1.37B rows @ 0% asset_code; path-pay 34–55% coded                                                                                                                                            |
| F-B | **LP asset filters can't match native XLM legs** — native leg stored as `asset_a_code=''`, no `type=native` escape hatch; "show all XLM pools" unanswerable. XLM = most common LP leg.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | HIGH     | `liquidity_pools/queries_ch.rs:~825-851`                                                                                                                                                                                                                                                                               |
| F-C | **Account tx list drops participation roles** — `transaction_participants` structure is correct (many-to-many, 2.16 accts/tx) but extraction omits: crossed-offer counterparty (`ClaimAtom::OrderBook` discarded, `operation.rs:118`), claimable claimants, `SetOptions inflationDest`, revoke-sponsorship target, non-`transfer` token-event recipients (mint/burn). Effect: balance changes with no matching tx.                                                                                                                                                                                                                                                                                                                                                                                                                           | MEDIUM   | `stage.rs:1878-1914` write path; `operation.rs:118-121,346,270,372`; `event_filters.rs:53-68`                                                                                                                                                                                                                          |
| F-D | **Contract-held classic/native orphaned when its SAC is un-sighted** — `HAVING max(sac_deployed)=1` (`persist.rs:297`) drops the balance to a bespoke type-3 id with no `assets` row → invisible until re-run. Under-counts supply/holders.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | MEDIUM   | `persist.rs:297`, `stage.rs:355`                                                                                                                                                                                                                                                                                       |
| F-E | Offers are unindexed by asset for EVERY asset (product question: are offers "asset transactions"?).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | MEDIUM   | offers type 3/12 = 1.37B rows, `with_code=0`                                                                                                                                                                                                                                                                           |
| F-F | **SAC-contract activity not unioned into the asset page** — every classic/native asset has a Stellar Asset Contract whose invocations ARE indexed (`soroban_invocations_appearances`, per-contract, complete) and shown on the CONTRACT page, but the asset-tx query ignores `sac_contract_surrogate` (uses only the asset's OWN contract, =0 for native/classic). So native's 3.9M XLM-SAC invocations — and every classic asset's SAC activity — never appear on the asset page. Native is therefore NOT pure "absence": it has a positive SAC key that is loaded on the row but unused. **Cheap near-term win** independent of the re-model: add `sac_contract_surrogate` as an OR-branch in the tx predicate (surfaces the classic-side SAC ops immediately); the complete version unions `soroban_invocations_appearances` for the SAC. | HIGH     | XLM SAC `CAS3J7GY…` surrogate `-6164601581949826601`: 3.9M invocations (soroban_invocations_appearances) + 13,804 ops (operations_appearances); `queries_ch.rs:222` loads `sac_contract_surrogate` but `fetch_transactions:538` uses only own `contract_surrogate_id`; native SAC linked in `asset_sac` (asset_type=0) |

Audited clean (no analogous problem — the right patterns):

- **Soroban `soroban_invocations_appearances`**: grain = per-(contract,tx); every
  call-graph contract (incl. inner DEX token transfers) is its own row. No
  participation loss. `amount Int32` is a fold-count (max observed 31), not a
  token value — safe. Only accepted gap: auth-tree fallback for
  diagnostic-event-less txs (ADR 0029).
- **Accounts** (structure), **pools** (array), **holders/supply/native**
  (positive surrogate) — all correct.

## Fundamental design (proposed — for the ADR)

**Chosen approach: a per-(operation, participating-asset) index.** New table
`operation_asset_appearances` (name TBD), one row per (op, asset) with `asset_id`
as the **leading sort key** — symmetric with `transaction_participants` for
accounts. Native XLM keyed by the existing positive surrogate
`ids::asset_id(0,"",0,0) = cityhash64("native")` (reuse the 0331/ADR-0051
convention — do NOT invent a new native key).

**Complete asset activity = a UNION of three streams** (F-F): the classic-op
participation index (above) ∪ the asset's **SAC-contract invocations**
(`soroban_invocations_appearances` keyed by `sac_contract_surrogate` — already
indexed, complete incl. inner DEX legs) ∪ own-contract invocations for a
type-3 soroban token (already works). The design must define how the endpoint
merges/paginates these streams (single unified keyset vs tagged sources).

Consequences:

- Every asset's tx list becomes complete: payment, BOTH path-payment legs +
  path hops, BOTH offer sides, trustline ops, claimable, LP legs, native, AND
  the asset's SAC/Soroban transfer activity.
- `asset_id`-leading key → fast seeks for any asset (no non-leading-PK scan,
  no per-asset bloom index needed for the driver).
- Native is a normal asset — no special-casing in the query layer.

**Cheap near-term win (F-F, independent of the re-model / backfill):** the
asset-tx query already receives `sac_contract_surrogate` on the row but ignores
it. Adding it as an OR-branch in the existing contract predicate immediately
surfaces the classic-side SAC ops on the asset page; unioning
`soroban_invocations_appearances` for that surrogate surfaces the full
Soroban-transfer stream (native XLM: ~3.9M rows already sitting in the DB). This
can ship before the big re-model and is a strictly better native stopgap than
the reverted variant C.

**Ingestion:** emit one appearance row per participating asset of each op
(parser already has all legs — `operation.rs` sendAsset/destAsset/path, offer
buy/sell). **Backfill: re-parse operations from archived XDR** (S3, per ADR 0018) — the existing CH rows only have one leg, so re-keying is insufficient;
full re-parse is required and accepted.

**Alternative considered — array columns** (`asset_ids Array` on the existing
row, filter via `has()`, like `pool_ids`): cheaper (no new table, no row
multiplication) but `has()` is not a leading-key seek and it's a half-measure.
Rejected in favour of the participation table for a clean fundamental fix — to
be re-confirmed in the ADR.

## Sub-work (all in this epic — no plasters elsewhere)

1. **ADR** — asset-participation index re-model (approach, key, backfill, query
   contract). Evergreen docs (ADR 0032): update `docs/architecture/**` schema +
   the `10_get_assets_transactions.sql` doc.
2. **Schema** — new `operation_asset_appearances` (or array columns) + skip
   indexes.
3. **Ingestion** — emit per-asset appearance rows (live + backfill crate; see
   [[feedback_backfill_new_crate]] — backfill = new crate, don't extend
   backfill-runner).
4. **XDR re-parse backfill** — 6.4 B ops, staged/rolled out carefully.
5. **Query rewrites** — `/assets/{id}/transactions` variant(s) → single native-
   inclusive path on the new index; drop the empty-native early-return.
6. **F-B** — LP native-leg filter (surrogate or `type=native` hatch).
7. **F-C** — account participation role completeness (extract dropped roles).
8. **F-D** — contract-held classic/native un-sighted-SAC orphan.
9. **API types** — regen if DTO/route shape changes (`api-types:generate`).

## Explicitly reverted stopgaps (do NOT re-apply outside this task)

- **Variant C** (native payments+create_account branch via op-type + null
  identity) — built during the 0348/F2 investigation, **reverted** on 2026-07-06.
  It was a correct-but-partial plaster (payments + account-creation only, still
  single-slot). Superseded by the participation index here.

## Acceptance criteria

- [ ] ADR written + `docs/architecture/**` updated (schema + query docs)
- [ ] `operation_asset_appearances` (or agreed shape) live; native = surrogate
- [ ] Ingestion emits one row per participating asset per op (live)
- [ ] XDR re-parse backfill complete + validated (spot-check vs Horizon /
      stellar.expert for a sample of assets incl. native)
- [ ] `/assets/native/transactions` returns real native activity (payments,
      path-payment legs, …) — no early-return
- [ ] Issued-asset lists now include offers + both path-payment legs
- [ ] F-B: LP pools filterable by native XLM leg
- [ ] F-C: dropped account roles indexed (crossed-offer counterparty etc.)
- [ ] F-D: contract-held classic/native not orphaned on un-sighted SAC
- [ ] F-F: asset page unions its SAC-contract invocations (native shows its
      ~3.9M XLM-SAC transfers; every classic asset shows its SAC activity).
      Cheap-win variant (wire `sac_contract_surrogate` into the tx predicate)
      may ship first, ahead of the full re-model.
- [ ] API types regenerated if shape changed
- [ ] Validation vs Horizon / stellar.expert (see [[reference_chq_clickhouse_cli]])

## System-wide audit (2026-07-06) — full findings inventory

A 6-agent parallel audit (NFT, ledger/tx, DEX/pools, Soroban-events, cross-cutting
absence/two-hop sweep, aggregate/detail) against prod CH found this is **not an
assets-only problem** — the same four bug classes recur across the whole
explorer, and there is a **second under-modelled layer** (`soroban_events`,
9.5 B rows — larger than `operations_appearances`) whose fungible-token flow is
almost entirely undecoded. **All clusters below are in-scope for THIS task (0357)
— karolkow decided 2026-07-06 to keep everything in one task, not fan out into
sibling tasks.** They are organised as workstreams (see "Workstreams" below).

**Two under-modelled layers + a cross-cutting read-side theme:**

- **Layer 1 — classic appearance index** (`operations_appearances`, 6.4 B):
  single asset/contract slot; native as absence. ← this task.
- **Layer 2 — Soroban events** (`soroban_events`, 9.5 B): transfer/mint/burn
  `from`/`to`/`amount` never decoded into columns (dead decoder); participants
  only partially captured. **Co-equal re-model — spawn its own epic.**
- **Cross-cutting read-side**: contract-as-holder/owner rendered NULL; alternate
  identities (SAC, inner tx hash, Soroban-contract) not unioned; search gaps.

### K1 — participation-loss (many-to-many → one slot / fold)

| id   | finding                                                                                                                                                | sev  | where                                |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ---- | ------------------------------------ |
| K1-1 | Asset single-slot (native empty + multi-asset legs lost) — **this task**                                                                               | HIGH | operations_appearances               |
| K1-2 | Offer op carries NO asset/price (fallthrough); path-payment stores only destAsset                                                                      | HIGH | stage.rs:1755-1823                   |
| K1-3 | **Fungible transfer from/to/amount never decoded** (dead `parse_transfer`); amount hardcoded 1 (transfer 4.51B, mint 434M, burn 62.8M, clawback 54.3M) | HIGH | soroban_events; event_filters.rs:44  |
| K1-4 | tx detail `operations[]` folds identical ops → `len < operation_count`                                                                                 | MED  | stage.rs:906; queries_ch.rs:743      |
| K1-5 | Account participation roles dropped (crossed-offer counterparty, claimants, inflationDest, revoke target)                                              | MED  | stage.rs:1878-1914; operation.rs:118 |
| K1-6 | NFT single current-owner slot (mitigated by /transfers)                                                                                                | MED  | nfts schema                          |
| K1-7 | `soroban_events` RMT key excludes payload — latent row loss if event_index non-unique                                                                  | LOW  | init.sql:612                         |

### K2 — absence-modeling (empty-string / reserved / non-G silently skipped)

| id   | finding                                                                                                | sev      | where                                      |
| ---- | ------------------------------------------------------------------------------------------------------ | -------- | ------------------------------------------ |
| K2-1 | Native XLM asset transactions empty — **this task**                                                    | HIGH     | assets/queries_ch.rs:521                   |
| K2-2 | LP native XLM leg unmatchable — **16,552 pools (21.7%) invisible**; "XLM" filter matches 701 impostors | HIGH     | liquidity_pools/queries_ch.rs:826,848      |
| K2-3 | `transaction_participants` drops ALL non-G participants (C/B/L/M) in token transfers                   | HIGH     | stage.rs:477-489,1686                      |
| K2-4 | Fee-bump fee-source unattributed — **~45% of txs**; not in source_id nor participants                  | HIGH     | envelope.rs:238; stage.rs:455              |
| K2-5 | NFT contract-owner rendered NULL — **22% of NFTs, 51% of transfer rows**                               | HIGH     | nfts/queries_ch.rs:174                     |
| K2-6 | Pending NFTs invisible — **71K NFTs (5.5× visible)**, Other/NULL never promoted                        | HIGH     | nfts/queries_ch.rs (never reads \_pending) |
| K2-7 | mint/burn/clawback participants never registered per-account/asset (434M+62.8M+54.3M)                  | MED-HIGH | event_filters.rs:53                        |
| K2-8 | Balances contract-holder orphaned when SAC un-sighted                                                  | MED      | persist.rs:297                             |
| K2-9 | Search: no asset findable by name ("USD Coin"→USDC, "lumens"→XLM)                                      | MED      | search/queries_ch.rs:592                   |

### K3 — two-hop-not-unioned (alternate identity indexed but not merged)

| id   | finding                                                                                          | sev                | where                                                     |
| ---- | ------------------------------------------------------------------------------------------------ | ------------------ | --------------------------------------------------------- |
| K3-1 | Asset SAC contract not unioned into asset page (F-F) — 3.9M native + all classic SAC             | HIGH               | assets/queries_ch.rs:222 vs 538                           |
| K3-2 | **Fee-bump inner_tx_hash never indexed → hard 404** on inner-hash lookup (Horizon resolves both) | HIGH               | stage.rs:753 col written, never in transaction_hash_index |
| K3-3 | tx `contract_ids[]` drops nested/event-only contracts — **100% of Soroban txs** (~5/tx missed)   | HIGH               | common/ch.rs:154                                          |
| K3-4 | Account/asset pages not unioned with soroban_events token transfers                              | HIGH               | no accounts/assets read of soroban_events                 |
| K3-5 | Soroban-AMM pools (soroban_events.contract_id) never unioned into /liquidity-pools               | MED (ADR-deferred) | liquidity_pools/queries_ch.rs:511                         |
| K3-6 | Search SAC C-address doesn't resolve to wrapped asset (detail page does)                         | MED                | search/queries_ch.rs:588                                  |
| K3-7 | NFT collection activity not unioned on contract page (mitigated)                                 | LOW                | contracts/queries_ch.rs:191                               |

### K4 — aggregate/detail divergence

| id   | finding                                                                               | sev     | where                                    |
| ---- | ------------------------------------------------------------------------------------- | ------- | ---------------------------------------- |
| K4-1 | Contract invocations KPI 7-day window vs all-time table (0348 F1)                     | MED     | contracts (7d stats vs invocations list) |
| K4-2 | tx `operation_count` (header) vs folded `operations[]` (K1-4)                         | MED     | handlers.rs:387 vs 390                   |
| K4-3 | events feed amount=1; tx-detail fallback count-only; archive-vs-non-archive diverge   | MED     | contracts/queries_ch.rs:803              |
| K4-4 | invocations.amount = fold-count, not token value (grain trap vs events)               | LOW     | init.sql:616                             |
| K4-5 | Nullable-aggregate decode 500 trap (class — audit fetch_one over sum/max on Nullable) | LOW-MED | (systemic)                               |
| K4-6 | LP participants share_percentage stale (candidate, unconfirmed — quota)               | LOW     | liquidity_pools                          |

**Cleared (agents raised, verified NOT bugs):** classic LP has no alt-contract
identity (not a Soroban contract); muxed activity captured under base G;
classic-DEX pool crossings DO appear via `pool_ids` (a strength); NFT
event-type completeness + batch-mint fan-out correct; ledger tx_count exact;
fee-bumps not double-counted; native events written normally to soroban_events.

**Workstreams (ALL in-scope in this task — do NOT spawn separate tasks; decision
karolkow 2026-07-06):**

1. **Layer-1 classic asset-participation re-model** — the core (K1-1/1-2, K2-1,
   K3-1). Per-(op, asset) index + native surrogate + SAC union + XDR re-parse.
2. **Layer-2 Soroban-events token-flow re-model** — decode transfer/mint/burn
   from/to/amount into queryable columns; index participants incl. contracts;
   union into asset + account pages (K1-3, K2-3/2-7, K3-4). 9.5 B rows.
3. **Contract-as-holder/owner read-side union** — join `soroban_contracts` in
   balances-holders + NFT owner/transfer reads (K2-5, K2-8). Read-side, data intact.
4. **Fee-bump completeness** — index `inner_tx_hash` (K3-2, hard 404) + attribute
   `fee_source`/`fee_charged` per account/asset (K2-4, ~45% of txs).
5. **Search completeness** — by-name asset search (K2-9) + SAC C-address resolve (K3-6).
6. **DEX/trades** — per-asset trades/offers surface + Soroban-AMM pools (K2-2, K3-5).
7. **Aggregate/detail hygiene** — KPI-window alignment (K4-1), fold-vs-count
   (K4-2/3), nullable-aggregate 500 sweep (K4-5). Also NFT pending promotion (K2-6).

## Independent corroboration — Stanisław Królikiewicz (stkrolikiewicz), 2026-07-06

Independent analysis by the senior, scoped to the L1 classic asset-attribution
question, **matches this task's core** (same prod numbers, same diagnosis, same
fix options) — a strong cross-check:

- **"Native has zero operations" → REFUTED (agrees).** Native IS written
  (`asset_code=''`, `asset_issuer_id=NULL`, from `split_asset_ref("native") →
(None,None)`); **514.09 M** type-1 native payments exist. The "zero" is purely
  read-side: `/assets/:id/transactions` is out-of-scope for native (documented in
  `10_get_assets_transactions.sql`) and any `asset_code='XLM'` query returns 0
  because native is the empty string. = our K2-1.
- **Multi-asset loss → CONFIRMED (agrees).** `OpTyped::from_details` keeps one
  asset slot; RMT key `(ledger, tx, application_order)` has no asset → >1 asset
  per op architecturally impossible. Offers (SellOffer 771.6 M + BuyOffer 598.6 M
  - PassiveSell 0.13 M ≈ **1.37 B**) hit the parser `_` fallthrough → **no leg
    attributed**. Path-payments keep only `destAsset`, drop `sendAsset` → every one
    loses its **source leg** (type 2: 1.07 B, type 13: 0.67 B ≈ **1.74 B**). = our
    K1-1/K1-2.
- **New detail added:** **Claimable balances (~11.7 M)** — CreateClaimableBalance
  - ClawbackClaimableBalance also drop their asset. (Fold into the L1 re-model.)
- **Partial compensation confirmed:** `pool_ids` recovers AMM-pool-crossing
  path-payments/offers (asset via pool join); order-book crossing + source leg
  stay invisible. = agent "C2".
- **Type-read validated:** op types that can't be native (Clawback/AllowTrust/
  SetTrustLineFlags) never appear with empty `asset_code` → the `type` numbers are
  read correctly. Rules out a mis-decode explanation.
- Fix options he lists (2nd asset column / `operation_asset_participation` table /
  fan-out per (op × asset)) = the same options in this task's design. His framing:
  "billions of rows = not an accident, it's a property of the project."

## Red-team calibration (2026-07-06) — corrections to the audit above

A 4-agent adversarial red-team (each told to REFUTE, re-derive numbers on prod,
flag intended-by-design) stress-tested every finding. The core holds but several
figures/severities were **overstated**; recorded here honestly. These corrections
**supersede** the numbers/severities in the tables above.

**Overstatements corrected:**

- **Headline "~3.1 B / 48% lost" → ~2.4 B / 37.5% carry NO asset.** Path-payments
  were counted whole (1.734 B), but **42% (731 M) keep `destAsset`** — a valid
  per-asset entry, not a loss. Honest floor = offers 1.37 B (21.4%) + empty
  path-pay 1.003 B + claimable 26.9 M = **2.40 B (37.5%)** with zero asset. Of
  that, **offers (21.4%) are the unambiguous defect**; path-payment source-leg
  loss (15.7%) is a design-tradeoff, not a clean loss. `pool_ids` does NOT
  recover it (offers 0%, path-pay 4.5%/14.7%).
- **K2-1 (native tx empty): HIGH → LOW / by-design.** Documented out-of-scope,
  PG parity (`10_get_assets_transactions.sql:61-62`). Data exists; read-side
  choice, not a defect. (Still worth doing as part of the re-model, but not a
  "bug".)
- **K3-1 (SAC not unioned): HIGH → MEDIUM.** SAC activity IS reachable via the
  contract-transactions endpoint (unions `soroban_invocations_appearances` by
  contract_id); it's a cross-reference gap on the _asset_ page, not invisibility.
- **K2-2 (LP native leg): 16,552 / 701 → 11,641 pools (22.4%) / 480 impostors.**
  Our count was raw ReplacingMergeTree rows; FINAL (user-visible) is ~1.4× lower.
  Mechanism airtight; HIGH stands; % actually slightly higher than claimed.
- **K1-3 (events undecoded): reword.** Core holds (`parse_transfer` is dead code;
  no queryable from/to/amount column; API `amount` hardcoded 1). But "9.5 B
  opaque/undecoded" overstates: `signature` IS a queryable column, and
  `topics_xdr`/`data_xdr` are ScVal-decoded JSON (not opaque XDR) — from/to/amount
  are recoverable from the payload, just not promoted to named columns.
- **K3-4 (events not unioned): downgrade + correction.** G-sided transfers DO
  appear on account pages via the `transaction_participants` back-fill; the gap is
  non-G sides + mint/burn/clawback, not "all transfers absent". The earlier claim
  that this was a "deliberate documented quota decision" is **unsubstantiated —
  no such ADR/doc exists** (retracted).
- **K2-3 (non-G participants): C/B/L only; muxed-M is NOT dropped** (canonicalised
  to base G upstream, ADR 0026). Dropping C/B/L is intended for the accounts-shape
  index; the real gap is contract/CB/LP transfer participation invisibility.

**Reclassified as NOT bugs (remove from the defect count):**

- **DEX per-asset trades** — an unbuilt feature (scope/roadmap), not a defect.
- **K3-5 (Soroban-AMM pools not unioned)** — INTENDED-BY-DESIGN, product-gated
  deferral (ADR 0014 §, ADR 0017 deferred-topics).
- **K4-5 (nullable-aggregate 500 trap)** — theoretical; the guards (`ifNull`,
  `toString`→Option, non-Nullable `count()`) are already present in all live CH
  `fetch_one` sites. Keep only as a review-time watch note.
- **K4-3/K4-4 (amount=1 / fold-count)** — documented naming choice, not a
  divergence bug (a "confusing field name" note at most).

**More corrections (quick-verdict cluster):**

- **K2-8 (contract-holder orphan): MED → LOW.** The primary SAC-`ContractData`
  `BalanceValue` path for contract-held classic/native IS shipped and
  prod-validated (`state.rs:302-369,428`; wired `process.rs:420` → `balances`);
  task 0331 (done) closed it. The earlier "types 0/1/2 skipped" framing came from
  0331's **pre-implementation** notes. Residual tail only: frozen-balance flags
  not propagated + non-standard custom-storage tokens skipped (not mis-summed).
- **K1-4 (op fold < operation_count): CONFIRMED but INTENDED-BY-DESIGN** —
  documented (`dto.rs:188-194`); heavy `operation_tree` carries the full unfolded
  list. Cosmetic on the light array.
- **K1-5 (crossed-offer counterparty dropped): CONFIRMED, COMMON** — order-book
  crossing is the normal taker path (`operation.rs:118-121` drops
  `ClaimAtom::OrderBook` seller_id). Solid MED.
- **K1-6 (NFT single-owner) / K3-7 (NFT collection): LOW confirmed** — correct for
  the indexed single-owner standards (no ERC-1155 path); collection-name sourcing
  fixed by task 0340.

**Overall-thesis verdict: SHIP WITH CHANGES.** "Single participant slot is _a_
real design limitation with real consequences (offers = 1.37 B rows, zero asset
attribution)" is **solid and HIGH**. But "single-slot is _the_ single root cause"
overclaims — native (out-of-scope) and SAC-union are **separate, independently
documented choices**, not downstream effects of the asset slot. The "codebase
already models participants/pools multi-valued elsewhere" contrast IS accurate
(not cherry-picked): `op_participant_str_keys` extracts all 3 asset fields into
`transaction_participants`; `pool_ids` is a real `Array`.

**Still quota-blocked (re-derive after 13:00 UTC):** fee-bump 45% (verify it's
not head-partition-only), NFT contract-owner 22%/51%, pending NFTs 71K,
contract_ids 100%-of-Soroban-txs. Mechanisms confirmed in code; exact prod
percentages pending the CH read-quota reset.

**Net after calibration:** the genuine, confirmed HIGH core is **offers carry
zero asset attribution (1.37 B / 21.4%)** plus the read-side native/SAC
completeness gaps (now LOW/MED) and the L2 fungible-transfer decode gap. Fewer
clean HIGHs than first stated; the re-model is still justified by the offers
defect + native-first-class goal, but sized honestly at **~37.5% no-asset**, not
48%.

## Notes / open questions

- **Offers as "asset transactions"?** Product call — include (stellar.expert
  does, via its own index) or keep as separate DEX activity. Default: include.
- **Row-count blow-up** — multi-asset ops multiply rows (>6.4 B). Size the
  storage + backfill window.
- **Reference:** stellar.expert exposes full native-XLM history via its own
  per-asset index (`/explorer/public/tx?asset[]=XLM`); Horizon cannot filter
  payments/operations by asset for ANY asset (only `/trades` supports
  `native`). This is a self-indexing task, achievable with our CH pipeline.
- Related: [[project_native_two_conventions]], [[project_contract_as_holder_gaps]],
  [[m2_enrichment_plan]], [[feedback_backfill_new_crate]].
