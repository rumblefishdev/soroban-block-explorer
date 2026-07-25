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
- [x] #1 type-3 `/assets/{C}/transactions` empty — **FIXED (composed read).** Deep-dive re-classified it from "low value" to a **regression**: the emitter emits nothing for `InvokeHostFunction` (fan-out has zero type-3 rows) and the old `operations_appearances` contract branch was dropped, so a type-3 token's asset page went EMPTY vs prod. Fix: `fetch_transactions` gained a 2nd keyset arm over `soroban_invocations_appearances` on the token's contract surrogate. Same commit serves F-F (SAC facet of classic/native). Read-side, ships with the deploy.
- [x] #7 ADR 0032 `docs/architecture/**` updated (schema-overview §4.5.1 + shape list/tree + rewrote 10_get_assets_transactions.sql)
- [ ] #8 `LIMIT 1 BY` read-in-order — **BLOCKED**: fan-out table not on prod yet (verified 0 rows in system.tables). Analytically fine (`asset_id =` makes `(ledger, tx)` the residual PK order → read-in-order holds; `LIMIT 1 BY` collapses adjacents without a re-sort). Run `EXPLAIN indexes=1` / `read_rows` on a hot asset **after** the CREATE TABLE + backfill.
- [ ] #10 dup: `AssetRef ≈ SacAssetIdentity` — **WONTFIX (karolkow).** Real shape-redundancy, but they are two different concepts (asset an op TOUCHES vs asset a SAC WRAPS) — same shape by coincidence. Merge cost (33 refs / 8 files + permanent cross-subsystem coupling) ≫ keeping two 15-line enums → don't dedup by shape. `SmallVec` micro-perf also skipped (perf, not reduction). 4th code-copy already removed by C3.
- [x] cleanup: or-pattern arms (offers 3→1, claim-CB 2→1, LP 2→1), `present_entry` helper (dedup change→entry), `const NATIVE_ASSET_ID`, stale docs — all DONE. `SmallVec` left (perf, not reduction).

## 4. Core findings (F-A..F-F)

- [x] F-A asset single-slot (native empty + multi-leg loss) — THE core
- [ ] F-B LP can't match native XLM leg (16 552 pools / 21.7% invisible) — 🧊
- [x] F-C account tx list drops roles — classic-op roles DONE (crossed-offer seller, claimants, inflationDest, revoke-target) via typed `extract_counterparties`; God-Payload full-replaced. Residuals split out: mint/burn recipients → K2-7 (L2 events), fee-bump fee-source → K2-4 (done)
- [ ] F-D contract-held classic/native orphaned when SAC un-sighted — 🧊
- [x] F-E offers unindexed by asset
- [x] F-F SAC-contract activity unioned into asset page (native 3.9M XLM-SAC) — DONE via the composed-read arm B (🔀 = K3-1; shipped with #1)

## 5. K1 — participation-loss

- [x] K1-1 asset single-slot (🔀 = F-A)
- [x] K1-2 offer carries no asset / path stores only destAsset
- [ ] K1-3 fungible transfer from/to/amount never decoded (transfer 4.51B / mint 434M / burn 62.8M / clawback 54.3M) — 🧊 (L2)
- [ ] K1-4 tx `operations[]` folds identical ops → len < operation_count
- [x] K1-5 account roles dropped (🔀 = F-C) — typed `extract_counterparties` (all op-body roles + crossed-offer seller) + issuers from `asset_appearances`; string-`details` extractor deleted
- [ ] K1-6 NFT single current-owner slot (mitigated by /transfers) — 🧊
- [ ] K1-7 `soroban_events` RMT key excludes payload (latent row loss)

## 6. K2 — absence-modeling

- [x] K2-1 native asset transactions empty — **fixed end-to-end**: native surrogate + seek + query + C6 read gate removed (`native_asset_singleton` provides the `assets` row)
- [ ] K2-2 LP native XLM leg unmatchable (🔀 = F-B) — 🧊
- [ ] K2-3 `transaction_participants` drops non-G participants (C/B/L/M) — 🧊
- [x] K2-4 fee-bump fee-source unattributed (~45% of txs) — typed `ExtractedTransaction.fee_source` (via `envelope_fee_source`, muxed→G); staging registers the payer beside the inner source. Parse-side only, no `transactions` column
- [ ] K2-5 NFT contract-owner NULL (22% NFT / 51% transfer rows) — 🧊
- [ ] K2-6 pending NFTs invisible (71K) — 🧊
- [ ] K2-7 mint/burn/clawback participants never registered — 🧊
- [ ] K2-8 balances contract-holder orphaned (🔀 = F-D) — 🧊
- [ ] K2-9 search: no asset findable by name — 🧊

## 7. K3 — two-hop-not-unioned

- [x] K3-1 asset SAC unioned (🔀 = F-F) — DONE (composed-read arm B)
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
- [x] **God-Payload / dual-extraction inconsistency (subset of R2)** — ACCOUNTS done: `ExtractedOperation` now carries a typed `counterparties` field beside `asset_appearances`, `op_participant_str_keys` (the account string-matcher) deleted, issuers derive from `asset_appearances`. Staging no longer string-matches accounts out of `details`. 🧊 remaining: CONTRACT participants (C-address, `OpTyped::from_details`) still string-matched → folds into K2-3 / R2 end-state (details-JSON as a derived view).
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
- [ ] #8 read-in-order check — `EXPLAIN indexes=1` / `read_rows` on a hot asset. BLOCKED until the table has data (analytically holds: `asset_id =` makes `(ledger, tx)` the residual PK order). Optional pre-deploy: local CREATE TABLE + sample insert to confirm the PLAN (not prod scale)

## 14. Process

- [ ] ADR 0032 `docs/architecture/**` update (🔀 = PR #7)
- [ ] api-types check post-merge (Cargo.lock touched)
- [ ] **SEPARATE TASK (not 0359): migrate `fetch_operations` off legacy asset columns.** `operations_appearances.asset_code`/`asset_issuer_id` are NOT dead — they power the **per-op asset** in the tx operations list ([queries.rs:829](../../../../crates/api/src/transactions/queries.rs)) + audit-harness. The fan-out is per-(asset, tx), NOT per-op → no drop-in replacement. Migrate that reader (new per-op source), THEN drop the columns from init.sql. The 0359 deploy does NOT need this.
- [ ] Pre-backfill quick-win: sort HashMap output (state.rs / nft.rs)
- [x] Pre-backfill quick-win: `op_meta_changes` V0..V4 exhaustive (🔀 = C2)

## 15. Roadmap — everything left, in order

Write-side for **classic operations is COMPLETE** (assets + all account roles + fee-bump payer). What remains:

**A. Deploy 0359 (OPS — immediate, gated on explicit go)**

1. `CREATE TABLE operation_asset_appearances` on prod (§13)
2. Backfill Soroban era (from ledger 50,457,424) + validate vs Horizon/stellar.expert (§13)
3. #8 read-in-order check — after data lands (§13)

**B. Separate implementation epics (own tasks, post-deploy)**

- **L2 — Soroban event side** (the big one): decode `soroban_events` from/to/**amount** (K1-3) · mint/burn/clawback participants (K2-7) · fix RMT key dropping payload (K1-7). All write into the SAME `transaction_participants` / asset index — different SOURCE (events), 9.5B rows.
- **K2-3** — non-G participants (contract C / pool L / balance B); today filtered by `starts_with('G')`.
- **R2 — typed OpFacts IR**: kills the 2nd God-Payload (`OpTyped::from_details`, the op-COLUMN string extractor); `details`-JSON becomes a derived view. Subsumes the `fetch_operations` migration below.
- **Migrate `fetch_operations` off legacy asset columns** → then drop them from init.sql (§14).
- **Read-side unions** (land anytime): F-B LP native leg · F-D contract-holder · K3-\* two-hop unions · K2-9 search.
- NFT gaps: K2-5 contract-owner NULL · K2-6 pending invisible · K1-6 single-owner slot.

**C. Process**

- api-types check if `Cargo.lock` / `crates/api` touched (§14).
- Quick-win: deterministic HashMap output ordering (state.rs / nft.rs) (§14).

## 16. Pre-backfill adversarial review (2 devils-advocate agents)

- [x] **Accounts write-side — no row-corruption bugs.** Verified: muxed→G (no M-leak), crossed-seller success-gating (no phantom crossings), determinism live=backfill (RMT all-cols in ORDER BY), FK consistency, exhaustive matches, strict superset of old (zero under-inclusion).
- [x] **Assets write-side — no blocks-backfill bugs.** Verified: `asset_id` parity emitter↔`assets` key, native surrogate, meta-grain id-match, per-tx dedup, index alignment. 3 doc fixes applied (V0..V2 justification factually wrong; `asset_code_str` injectivity overclaim; orphan-FK note) — commit `12d213d2`.
- [x] **Decision 1c — issuer NOT a participant.** Dropped the issuer-from-`asset_appearances` participant loop: **redundant** with the asset index (issuer derivable from `asset_id`) and would flood a popular issuer's account page. Asset activity lives on its asset page; issuer's account page = own activity only. No read-side issuer-union either.
- [x] **#2 SetOptions signer — keep as-is** (inflationDest emitted, signer not; inflation is dead on mainnet → near-zero in the P20+ window).
- [x] **#3 failed-tx body roles — accepted.** Body-grain account roles (destinations, CB claimants, revoke targets, inflationDest) register for failed txs too, consistent with the asset-side failed-tx policy (§2 C5) and the old behaviour; the crossed seller is correctly excluded (result is success-gated). The issuer-on-failed-tx part of the finding dissolved with 1c.
- [x] **VERDICT: classic-op write-side COMPLETE + adversarially verified → backfill-ready.** Only Soroban-side participants (K2-7 / K2-3) remain, and they ride the separate L2 backfill — not a repeat of this one.
