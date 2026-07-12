---
title: 'Progress tracker — full 0359 inventory (core + all audits + follow-ups)'
type: generation
status: developing
spawned_from: '0359'
spawns: []
tags: ['progress', 'tracker', 'inventory', 'checklist']
links: []
history:
  - date: 2026-07-11
    status: developing
    who: karolkow
    note: >
      Single place to measure task progress. Every finding from R-audit-inventory
      (F-A..F-F, K1-K4), the /devils-advocate pass, Stanisław's PR #324 review, and
      G-architecture-audit, with a status. Living doc — tick boxes as work lands.
---

# 0359 progress tracker

**Legend:** `- [x]` done · `- [ ]` open (in scope) · 🧊 deferred (own follow-up) ·
❌ killed (never) · 🔀 alias of another id. Cross-refs in parens.

## 1. MVP core (the fan-out that shipped)

- [x] `operation_asset_appearances (asset_id, ledger, tx)` table — RMT, partitioned, `ORDER BY (asset_id, ledger, tx)`
- [x] Emitter `asset_appearances.rs` — pure presence, BODY grain
- [x] Emitter — META grain (claim-CB / clawback-CB / LP from `op_changes`)
- [x] Live-ingest wiring (staging appends `op_asset_rows`, zero change to other tables)
- [x] Read swap `fetch_transactions` — asset-leading seek + `max(sequence)` fence + `LIMIT 1 BY`
- [x] Native = first-class surrogate `ids::asset_id(0,"",0,0)`
- [x] Merged develop → stellar-xdr 27 (curr→root)

## 2. Devils-advocate concerns

- [x] C1 — drop B3 / result grain (redundant; atoms ⊆ body)
- [x] C2 — meta-V5 wildcard closed (`op_meta_changes` exhaustive V0..V4)
- [x] C3 — asset-code unify, injective (no `<invalid>` collision)
- [x] C4 — claim-CB `[State, Removed]` test
- [x] C5 — failed-tx: **accept + document (parity)** decided; documented in the emitter module doc (no emission change)
- [x] C6 — native page served: `row.id==0` guard replaces the identity gate; native has a non-zero surrogate + `native_asset_singleton` row → F2 fixed

## 3. PR #324 review (Stanisław — 10 findings + cleanup)

- [x] #3 asset-code divergence (🔀 = C3)
- [x] #4 `lp_pool_assets` matches `op.liquidity_pool_id` (write-side, pre-backfill)
- [x] #5 failed-tx decision (🔀 = C5)
- [x] #6 per-tx `HashSet<asset_id>` dedup before push (write volume)
- [x] #2 native rows written **and now served** (read gate removed via C6; keep-writing is by design)
- [x] #9 `row.id==0` guard replaces the removed early-return (🔀 = C6)
- [ ] #1 type-3 `/assets/{C}/transactions` empty — 🧊 separate ADR (🔀 = F-F)
- [x] #7 ADR 0032 `docs/architecture/**` updated (schema-overview §4.5.1 + shape list/tree + rewrote 10_get_assets_transactions.sql)
- [ ] #8 `LIMIT 1 BY` read-in-order — **BLOCKED**: fan-out table not on prod yet (verified 0 rows in system.tables). Analytically fine (`asset_id =` makes `(ledger, tx)` the residual PK order → read-in-order holds; `LIMIT 1 BY` collapses adjacents without a re-sort). Run `EXPLAIN indexes=1` / `read_rows` on a hot asset **after** the CREATE TABLE + backfill.
- [ ] #10 dup: `AssetRef ≈ SacAssetIdentity`, `asset_ref ≈ sac::asset_to_identity` (L1; 4th code-copy already removed by C3) — **stale-doc part DONE** (operation.rs + queries.rs comments fixed)
- [ ] cleanup: or-pattern arms (L1) + micro-perf `const NATIVE_ASSET_ID`/`SmallVec` (L1) — **stale docs DONE**

## 4. Core findings (F-A..F-F)

- [x] F-A asset single-slot (native empty + multi-leg loss) — THE core
- [ ] F-B LP can't match native XLM leg (16 552 pools / 21.7% invisible) — 🧊
- [ ] F-C account tx list drops roles (crossed-offer counterparty, claimants, inflationDest, revoke-target, mint/burn recipients) — 🧊 (code in stash)
- [ ] F-D contract-held classic/native orphaned when SAC un-sighted — 🧊
- [x] F-E offers unindexed by asset
- [ ] F-F SAC-contract activity not unioned into asset page (native 3.9M XLM-SAC) — 🧊 (🔀 = K3-1; MVP is single-arm)

## 5. K1 — participation-loss

- [x] K1-1 asset single-slot (🔀 = F-A)
- [x] K1-2 offer carries no asset / path stores only destAsset
- [ ] K1-3 fungible transfer from/to/amount never decoded (transfer 4.51B / mint 434M / burn 62.8M / clawback 54.3M) — 🧊 (L2)
- [ ] K1-4 tx `operations[]` folds identical ops → len < operation_count
- [ ] K1-5 account roles dropped (🔀 = F-C) — 🧊
- [ ] K1-6 NFT single current-owner slot (mitigated by /transfers) — 🧊
- [ ] K1-7 `soroban_events` RMT key excludes payload (latent row loss)

## 6. K2 — absence-modeling

- [x] K2-1 native asset transactions empty — **fixed end-to-end**: native surrogate + seek + query + C6 read gate removed (`native_asset_singleton` provides the `assets` row)
- [ ] K2-2 LP native XLM leg unmatchable (🔀 = F-B) — 🧊
- [ ] K2-3 `transaction_participants` drops non-G participants (C/B/L/M) — 🧊
- [ ] K2-4 fee-bump fee-source unattributed (~45% of txs) — 🧊
- [ ] K2-5 NFT contract-owner NULL (22% NFT / 51% transfer rows) — 🧊
- [ ] K2-6 pending NFTs invisible (71K) — 🧊
- [ ] K2-7 mint/burn/clawback participants never registered — 🧊
- [ ] K2-8 balances contract-holder orphaned (🔀 = F-D) — 🧊
- [ ] K2-9 search: no asset findable by name — 🧊

## 7. K3 — two-hop-not-unioned

- [ ] K3-1 asset SAC not unioned (🔀 = F-F) — 🧊
- [ ] K3-2 fee-bump `inner_tx_hash` never indexed → hard 404 — 🧊
- [ ] K3-3 tx `contract_ids[]` drops nested (100% of Soroban txs) — 🧊
- [ ] K3-4 account/asset pages not unioned with soroban_events transfers — 🧊
- [ ] K3-5 Soroban-AMM pools not unioned into /liquidity-pools — 🧊
- [ ] K3-6 search SAC C-address doesn't resolve to wrapped asset — 🧊
- [ ] K3-7 NFT collection activity not unioned on contract page — 🧊

## 8. K4 — aggregate/detail divergence

- [ ] K4-1 contract invocations KPI 7d vs all-time — 🧊
- [ ] K4-2 tx `operation_count` vs folded `operations[]` — 🧊
- [ ] K4-3 events amount=1 / archive-vs-non-archive diverge — 🧊
- [ ] K4-4 `invocations.amount` = fold-count not token value — 🧊
- [ ] K4-5 nullable-aggregate decode 500 trap (systemic) — 🧊
- [ ] K4-6 LP `share_percentage` stale (unconfirmed) — 🧊

## 9. Layers + workstreams

- [ ] L1 classic appearance index = this task — partial (core done, rest 🧊)
- [ ] L2 `soroban_events` (9.5B) token-flow re-model — 🧊 own epic
- [ ] WS1 Layer-1 core — partial · WS2 L2 🧊 · WS3 contract-holder read-union 🧊 · WS4 fee-bump 🧊 · WS5 search 🧊 · WS6 DEX/trades 🧊 · WS7 aggregate hygiene 🧊

## 10. Plan — stages A-F + ordered steps 1-12

- [ ] Stage A contract-holder reads — 🧊
- [ ] Stage B fee-bump 404 — 🧊
- [ ] Stage C search — 🧊
- [ ] Stage D FE humanizeOp render — 🧊
- [ ] Stage E aggregate hygiene (K4) — 🧊
- [ ] Stage F L2 events — 🧊
- [x] Step 1 foundation (emitter) · [x] 2 schema · [x] 3 live ingest
- [ ] Step 4 backfill (OPS pending)
- [ ] Step 5 read rewrite — partial (single-arm done; SAC-union 🧊)
- [ ] Steps 6-12 (L2 / account-roles / contract-holder / fee-bump / search / hygiene / FE) — 🧊

## 11. Architecture audit (G-architecture-audit, 28-item)

- [ ] Adoption #1 central `meta.rs` (6 sites, silent-V5) — partial (local `op_meta_changes` done; full `meta.rs` 🧊 stash)
- [ ] ~~Adoption #2 provenance `ingest_runs` / `parser_version`~~ — ❌ killed
- [ ] Adoption #3 shared commit-fence builder — partial (fence in query; builder open)
- [ ] R1 shared tx-feed engine — 🧊
- [ ] R2 typed OpFacts IR (post-backfill) — 🧊
- [ ] R3 ops cleanup: dead `account_balances_current`, `idx_oa_asset_issuer_id` bloom, legacy asset columns — 🧊
- [ ] MAJOR: poison-pill quarantine
- [ ] MAJOR: partition-pinned filtered global lists
- [ ] MAJOR: overscan×4 without refill
- [x] MAJOR: 3-way asset-code normalization (🔀 = C3)
- [ ] MAJOR: sibling wildcards — `op_meta` done; `emit_asset_participations` / `extract_counterparties` / `claim_atoms` canary 🧊
- [ ] MINOR: ledgers `LIMIT 1 BY`
- [ ] MINOR: cursor-to-filter binding
- [ ] MINOR: dead dictionary + `idx_tx_hash_bloom`
- [ ] MINOR: HashMap iteration order (state.rs / nft.rs) — determinism
- [ ] MINOR: muxed-id dropped in details JSON
- [ ] MINOR: u256/i256 as raw hex
- [ ] MISSING pattern: automated verify-range vs Horizon (G-role-crossref = the contract) — 🧊

## 12. Killed / reverted (do NOT revisit)

- ❌ role + `application_order` + `ParticipationRole` enum (pure presence)
- ❌ `leg_index`
- ❌ result grain (B3) — DA C1
- ❌ amount column — definitively rejected (follows the role rejection; recipe stays in README only as history)
- ❌ provenance (`parser_version`)

## 13. OPS / backfill (pending)

- [ ] Manual `CREATE TABLE operation_asset_appearances` on prod (init.sql fresh-only)
- [ ] Backfill `backfill-runner Run` Soroban era (from ledger 50,457,424) — SAME rollout as read swap
- [ ] Validate sample assets (incl. native) vs Horizon / stellar.expert

## 14. Process

- [ ] ADR 0032 `docs/architecture/**` update (🔀 = PR #7)
- [ ] api-types check post-merge (Cargo.lock touched)
- [ ] Legacy `operations_appearances.asset_code`/`asset_issuer_id` — NOT dead (read by `fetch_operations` + audit-harness); retire only after migrating that reader
- [ ] Pre-backfill quick-win: sort HashMap output (state.rs / nft.rs)
- [x] Pre-backfill quick-win: `op_meta_changes` V0..V4 exhaustive (🔀 = C2)
