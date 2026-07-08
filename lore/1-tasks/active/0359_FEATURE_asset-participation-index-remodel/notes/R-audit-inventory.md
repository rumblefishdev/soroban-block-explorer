---
title: 'Audit inventory — root cause & full findings'
type: research
status: mature
spawned_from: '0359'
spawns: []
tags: ['clickhouse', 'audit']
links: []
history:
  - date: 2026-07-06
    status: mature
    who: karolkow
    note: 'Full prod-CH audit (root cause + F-A..F-F + K1–K4). Extracted from the 0359 README on folder conversion (2026-07-07); content dated 2026-07-06.'
---

# Audit inventory — root cause & full findings

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

## System-wide audit (2026-07-06) — full findings inventory

A 6-agent parallel audit (NFT, ledger/tx, DEX/pools, Soroban-events, cross-cutting
absence/two-hop sweep, aggregate/detail) against prod CH found this is **not an
assets-only problem** — the same four bug classes recur across the whole
explorer, and there is a **second under-modelled layer** (`soroban_events`,
9.5 B rows — larger than `operations_appearances`) whose fungible-token flow is
almost entirely undecoded. **All clusters below are in-scope for THIS task (0359)
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
