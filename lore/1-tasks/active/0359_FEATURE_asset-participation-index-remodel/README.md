---
id: '0359'
title: 'Asset-participation index re-model — native XLM first-class + complete per-asset activity (offers, all path-payment legs)'
type: FEATURE # fundamental data-model fix: schema + ingestion + XDR re-parse backfill + query rewrites
status: active
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
  - date: 2026-07-06
    status: backlog
    who: karolkow
    note: >
      Renumbered 0357 → 0359. Three sessions independently grabbed id 0357 on
      2026-07-06; the develop rebase surfaced the collision. PERF
      launch-readpath task keeps 0357 (already public on origin/develop, ref'd
      by 0355); this FEATURE moves to 0359. Inbound refs in 0348 updated.
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Added "Code-verified cross-validation of Stanisław's second-slot proposal".
      Checked every code-verifiable claim of his follow-up against source (all
      CONFIRMED): projection picks one leg (stage.rs:1757), offers/claimable hit
      the `_` fallthrough (stage.rs:1806), raw parser keeps all legs in details
      JSON (operation.rs:216-266,344). Surfaces the one real divergence —
      second-slot vs fan-out — and the open call for karolkow: native = positive
      surrogate (fan-out) or read-side fix enough (second slot).
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      karolkow ruled: NO half-measures/forward-only stopgaps; fundamental fix +
      complete BACKWARD data, backfill done together. Added "Completeness ceiling
      + external cross-validation" section: one path-payment op touches up to 7
      assets (MAX_PATH_LENGTH=5) → any fixed N slots is structurally incomplete;
      Horizon treats path hops as first-class (`path[]`), can't filter ops by
      asset; stellar.expert per-asset tx is UI/internal-index only (no public
      API). Independent options analysis (fixed-slot REJECT / array viable-but-
      against-grain / fan-out RECOMMEND) → fan-out with a role column is the only
      complete+correct+fast+role-aware model. Flagged 2 deeper completeness needs:
      claimable-claim asset resolution (CB-id join) + result-side claim-atom
      sourcing (authoritative over declared op body).
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Round-2 external cross-validation (more sources). Hubble/stellar-etl (SDF's
      own BigQuery warehouse) uses a 2-endpoint-slot model (source_asset + asset
      columns) + path as nested details.path metadata — hops NOT indexed as op
      participants; industry indexers (Mercury/SubQuery) decode Soroban EVENTS for
      token flow (corroborates L2). Refinement: op→hop attribution is redundant
      with a complete trades/claim-atom stream → drop it. Real kill-shot for fixed
      slots = result-side claim atoms give UNBOUNDED asset-participations per op
      (crossed offers), so fan-out mandatory regardless of declared path. Recorded
      role enum (sent/received/sold/bought/traded/trustline/escrowed/released/
      admin/lp_a/lp_b); "traded" grain is where hops live.
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Converted to folder task (README + notes/). README grew past ~500 lines;
      split into R-audit-inventory, R-external-cross-validation,
      S-diagnosis-calibration, S-design-options. Also ran a 6-way red/blue-team on
      the modeling options (fixed-slot DIES; all others CONDITIONAL and converge on
      a role-tagged fan-out) — recorded in S-design-options.
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Added S-field-comparison-fat-thin: 4-way field matrix (our frontend vs
      THIN vs FAT vs stellar.expert) with body-vs-result provenance and a tiered
      capture recommendation. Frontend verified THIN (AssetTransactions renders
      a tx-summary row, no per-leg amount/role); stellar.expert (reference)
      renders per-asset amount + role + hops + a Trades tab. Key framing: the
      XDR re-parse is the cost, not the columns — op-body per-leg fields are
      ~free to capture in the one backfill; result/meta (realized amounts,
      ClaimAtom trades) is the real cost fork. Next: a devils-advocate
      adversarial pass over every decision/assumption before the ADR.
  - date: 2026-07-07
    status: backlog
    who: karolkow
    note: >
      Ran a 7-agent devils-advocate pass (S-devils-advocate). Core diagnosis
      HOLDS against canonical XDR/Horizon; verdicts 6× "ship with changes",
      1× "rethink packaging", 0× "ship as-is", 0× "rethink core". Outcome:
      SEQUENCE the work (Phase 0 native surrogate + F-F, no backfill → Phase 1
      offers → Phase 2 full fan-out gated on a frontend render spec) and SPLIT
      the epic (Layer-2 soroban_events + fee-bump/NFT/search into sibling tasks,
      to be spawned on develop). Two CRITICAL pre-ADR gates: content-addressed
      leg_index + differential test; scope split. Corrected overstated cost
      claims in S-field-comparison (ADD COLUMN is metadata-only; Tier-2 result
      already deserialized; ZSTD 20-40x doesn't apply to Decimal128 amounts).
      README Revised-plan section supersedes the "all in one epic" framing.
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      karolkow decisions: (1) Road B — iterative/phased, but committed through
      the full fan-out (no stopping at plasters). (2) Historical completeness =
      YES: every phase that lost data re-parses its backward history; asset pages
      must match mature explorers years back. (3) NO separate ADR — design
      answers recorded in the task (added G-schema-and-roles: before/after row
      shape, role source = op field slot, op-type already stored, old table stays
      as companion index, op-type→role mapping). Retracted the "gate Phase 2 on a
      frontend render spec" — it was a forward-only stopgap conflicting with the
      no-half-measures principle; Phase 2 fan-out + full backfill is now COMMITTED
      regardless of current FE.
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      Added R-prod-evidence-cross-validation: direct prod-CH evidence via chq +
      per-example cross-validation against Horizon + stellar.expert (both links
      each). Key measured numbers: operations_appearances = 6.405 B rows; in the
      last 300k ledgers 57.7% of ops have empty asset_code, offers (types 3/4/12)
      = 28.06 M rows at 100% empty, native payments 23.45 M, path-payments 36.1%
      native-dest. Cleanest core-bug example = a manage_sell_offer (XLM↔AQUA)
      storing zero asset; richest multi-asset = a 10-op path payment touching 12
      distinct assets, of which we keep only the dest legs.
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      Live-inspected the running front (localhost:4200) for tx df80d042 (a
      path-payment self-swap 1 XLM→bubba via TF). Normal mode renders a
      MISLEADING "Sent 1 XLM to [self]" (humanizeOp.ts uses sendAmount/sendAsset,
      drops the received bubba + TF hop); advanced mode is a raw internal-field
      dump. Ran /ux-expert → audit + redesign (one progressive operation card:
      true headline + Sent/Received/Route + expandable Trades/Events/XDR).
      Recorded in S-tx-render-audit. SEPARATE FE/UX concern from 0359's data
      model → spawn its own FE task on develop; the humanizeOp path-payment
      mislabel is a shippable correctness fix on its own. Also bounded the
      backfill scope to the Soroban era (min ledger 50,457,424).
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      Extended analysis (live front + prod): checked more op types (sell offer,
      claim-CB) in both views vs stellar.expert. Findings — normal humanizeOp
      only handles 4 op types (everything else = "X processed"); advanced already
      re-parses full details for most types, so the render fix is humanization,
      not new data; EXCEPTION claim/clawback-CB asset is missing even in advanced
      (body has only balanceId → needs meta), a gap shared with Phase-2 and even
      stellar.expert's simple render. Per-tx render does NOT need the fan-out
      (just humanization); per-asset pages + Trades tab DO. Delivered: (B) a
      per-op-type render spec in S-tx-render-audit; (C) the complete 25-op
      type→role mapping in G-schema-and-roles (clawback/authorize/create-account
      roles added; inflation skip + sponsorship N/A recorded; claim-CB asset from
      meta; PoolShare 3-entity keying = the one open sub-decision).
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      Resolved the last two design opens: (a) PoolShare/LP keying — not a 3-entity
      puzzle, pool is its own dimension so LP = 2 asset rows (lp_a/lp_b) + a
      pool_id column; (b) leg_index — content-addressed via a fixed XDR-order
      enumeration + one shared lib (live+backfill) + a blocking differential test.
      Added the archive-on-demand refinement to S-field-comparison (per-leg
      amounts are a performance choice for inline-amount lists, not completeness).
      Reconciled README + S-design-options + S-diagnosis for consistency. Wrote
      G-spawn-plan: 0359 stays the fan-out core (Phase 1→2); spawn Phase 0,
      Layer-2, contract-holder, fee-bump, search, FE-render as children (on
      develop, related_tasks 0359); every F-*/K-* finding mapped to a home.
      0359 is spawn-ready.
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      Ran /devils-advocate on the spawn plan (ship-with-changes). The by-finding
      split under-modeled 3 shared substrates → revised G-spawn-plan: added F0
      (shared emission lib + ONE archive-re-parse harness, spawn first; 0359/#2/#7
      depend on it); named 0359 the asset read-query owner with composition
      fan-out ∪ SAC ∪ soroban-events (the three rewrites must compose, not
      overwrite); un-folded F-C into its own sibling #7 (different table + own
      backfill = scope creep if folded); marked Phase 0 read-path interim, FE
      claim-CB line meta-dependent, priorities provisional.
  - date: 2026-07-08
    status: active
    who: karolkow
    note: >
      Activated (backlog → active). Note: the earlier commit 04a00699
      ("activate task") moved the file to active/ but did NOT flip the frontmatter
      (a `git add` pathspec error left the status edit unstaged); this commit sets
      status: active for real. Implementation proceeds on feat/0359.
  - date: 2026-07-08
    status: active
    who: karolkow
    note: >
      Governing decision recorded (new "## Plan" section, authoritative). ONE
      task, step by step, ZERO plasters/hotfixes — rule: if a step is subsumed by
      a later step, it is NOT built. Reaffirms the 2026-07-06 single-task call,
      SUPERSEDING the G-spawn-plan 7-sibling split AND the Phase-0 interim. Dropped
      on the no-plaster rule: Phase-0 native read-side (→ native = positive
      surrogate straight in the fan-out) + the F-F cheap-win OR-branch (→ full SAC
      union ships as the lasting stream). Recorded the independence map (stages
      A–F independent/semi vs the F0-bound core chain) + the 12 ordered fundamental
      steps. Banners added to the now-superseded "Revised plan" + acceptance-criteria
      phasing note. Committed on feat/0359 (not develop — implementation branch).
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

## Status snapshot (2026-07-09)

**Core (steps 1–7) BUILT + pre-backfill hardening DONE** — all UNCOMMITTED on
`feat/0359_asset-participation-index-remodel`; full-workspace `cargo test` +
`cargo clippy -D warnings` green.

- ✅ **Done:** **pure-presence** asset-appearance emitter (`asset_appearances.rs`)
  - `operation_asset_appearances` table `(asset_id, ledger_sequence,
transaction_id)` — the EXACT `transaction_participants` shape with `asset_id`
    for `account_id` + live-ingest / staging / writer + 2-arm
    `/assets/:id/transactions` read + token-event participant registration +
    account counterparties (F-C) + contract passive-receipt arm. Bugs fixed:
    arm-A pagination (`LIMIT 1 BY`), missing commit-fence. Hardening: `meta.rs`
    (no-wildcard `TransactionMeta`, fail-loud on non-Soroban meta), `asset_code.rs`
    (one canonical normalizer, no `<invalid>` sentinel).
- 🔑 **Decision (karolkow 2026-07-09): PURE PRESENCE** — dropped `role` +
  `application_order` + the whole `ParticipationRole` enum. A role is a
  per-OPERATION property but the asset page lists TRANSACTIONS (deduped per tx),
  and the tx-detail view re-parses the archive to describe ops; other explorers
  (Etherscan Transfers, Horizon `/trades`+`/payments`) facet at the endpoint,
  not via a stored per-tx role. If a typed/operation view is ever wanted →
  re-backfill then (same "re-parse when needed" model as the provenance drop).
- ⏳ **Remaining (none is pre-backfill):** independent stages A–E
  (contract-as-holder, fee-bump 404, search, K4 hygiene, FE render) + F (L2
  `soroban_events` from/to/amount columns — participant registration done,
  columns cut pending a reader); refactors R1 (feed engine) / R2 (OpFacts IR,
  post-backfill) / R3 (dead-schema cleanup); **OPS** (manual `CREATE TABLE`,
  Soroban-era backfill, Horizon validation, docs/architecture, amount-column
  decision).
- ❌ **Dropped from scope (karolkow):** provenance (`parser_version`) — a
  parser bug means a FULL re-backfill, not targeted re-heals; table-grain shrink;
  disk mitigation.
- ▶️ **Next:** granular commits — cosmetic/docs first, big logical last; no push
  yet.

## Design decision (current)

**A single unified, `role`-tagged fan-out participation table**
(`operation_asset_appearances`, one row per (op, asset, role)) is the mandated
core — the only model that is complete (unbounded N incl. result-side claim
atoms), correct (role disambiguates hop / endpoint / sold / bought / traded),
fast (asset-leading seek), and native-first (positive surrogate). Six modeling
options were red/blue-teamed against prod code: fixed-slots **DIED**; all others
are conditional and collapse into "you need the fan-out anyway".

> **Updated by the Revised plan below (post devils-advocate + prod evidence).**
> Fan-out stays the end-state, but: fat/thin is **resolved (THIN)**; `leg_index`
> determinism is **specced** (content-addressed, [G-schema-and-roles](notes/G-schema-and-roles.md));
> there is **no separate ADR** (design lives in this task); the fan-out's
> justifications were **softened** (native does not justify it — it is
> read-side-fixable; "only complete model" is via the read-time union, not the
> fan-out alone; the real driver is unbounded result-side trades + hot-key seek);
> and the work is **sequenced** (Phase 0/1/2) and **scoped to the Soroban era**.

Full six-option analysis + the softened verdicts →
[S-design-options](notes/S-design-options.md) and [S-devils-advocate](notes/S-devils-advocate.md).

## Plan (karolkow, 2026-07-08) — one task, fundamental-only, ZERO plasters

**Governing decision — AUTHORITATIVE. Supersedes the "Revised plan" phasing and
the G-spawn-plan sibling split below.** Reaffirms the original 2026-07-06
"everything in one task" call ([R-audit-inventory](notes/R-audit-inventory.md)),
overriding both the 7-sibling spawn ([G-spawn-plan](notes/G-spawn-plan.md)) and the
Phase-0 interim:

- **One task, step by step.** No sibling spawn. All F-A..F-F + K1–K4 + L2 +
  fee-bump + search + FE render + hygiene live here. See [[feedback_task_scope]].
- **Only fundamental fixes — no hotfixes / plasters.** Rule (karolkow): **if a
  step would be subsumed by a later step, it is NOT built.** Removed on that rule:
  - **Phase 0 native read-side interim** — subsumed by the fan-out → **dropped**.
    Native becomes a positive surrogate directly in the fan-out table.
  - **F-F cheap-win OR-branch** (patch on the old single-slot) — subsumed by the
    full SAC union → **dropped**; the full `soroban_invocations_appearances` union
    ships as the lasting read stream.
- **F0 kept** — shared emission lib + `leg_index` gate + one archive-re-parse
  harness. Foundation, not a plaster.
- **Backfill** bounded to the Soroban era (~13 M ledgers, min 50,457,424).

### Independent stages (start anytime — no dep on the fan-out core or each other)

| id  | Stage                                      | Problem (TL;DR)                                                                                                                                                                                                              | Fix (TL;DR)                                                                                                                              |
| --- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| A   | Contract-as-holder/owner (F-D, K2-8, K2-5) | Contract holding classic/native orphaned when its SAC un-sighted (`HAVING max(sac_deployed)=1`, persist.rs:297) → under-counts supply/holders. NFT contract-owner NULL (22% NFT, 51% transfer rows, nfts/queries_ch.rs:174). | Read-side union `soroban_contracts` in balances-holders + NFT owner/transfer. Data intact, read-only.                                    |
| B   | Fee-bump (K3-2, K2-4)                      | `inner_tx_hash` never indexed → hard **404** on inner-hash lookup. Fee-source **~45% txs** unattributed (envelope.rs:238; stage.rs:455/753).                                                                                 | Index `inner_tx_hash` in transaction_hash_index; attribute fee_source/fee_charged per account.                                           |
| C   | Search (K2-9, K3-6)                        | No asset findable by name ("USD Coin"→USDC, "lumens"→XLM); SAC C-address doesn't resolve to wrapped asset (search/queries_ch.rs:588,592).                                                                                    | By-name asset search + SAC C-address→asset resolve.                                                                                      |
| D   | FE render (humanizeOp)                     | Normal one-liner **factually misleading** ("Sent 1 XLM" for a bubba swap); humanizeOp handles only PAYMENT/PATH_PAYMENT/INVOKE/CREATE_ACCOUNT, rest → "X processed" (humanizeOp.ts:52-61).                                   | Per-op-type human headline + progressive detail; path-payment uses `result` for received. Claim-CB line waits on meta; rest independent. |
| E   | Aggregate/detail hygiene (K4-\*, K2-6)     | KPI 7d-window vs all-time (K4-1); operation_count vs folded operations[] (K4-2); nullable-aggregate 500 trap (K4-5); NFT pending **71K** invisible (K2-6).                                                                   | KPI-window alignment, fold-vs-count consistency, nullable-aggregate sweep, pending-NFT promotion.                                        |

### Semi-independent (own decode + backfill; unions in only at the read query)

| id  | Stage                                                 | Problem (TL;DR)                                                                                                                                                                                                                               | Fix (TL;DR)                                                                                                                                                                          |
| --- | ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| F   | L2 `soroban_events` decode (K1-3, K2-3/2-7, K3-3/3-4) | `soroban_events` (**9.5 B**) — transfer/mint/burn from/to/amount **never decoded** to columns (dead `parse_transfer`, event_filters.rs:44), amount hardcoded 1; non-G participants dropped; `contract_ids[]` drops nested (100% Soroban txs). | Decode from/to/amount → columns; index participants incl. contracts; union into asset + account pages. Own backfill via the shared harness; couples only at the composed read query. |

### Core — one interlocked chain (NOT independently splittable)

All routed through **F0** (shared emission lib + `leg_index` gate + one harness)
and the **composed read query** — so these are one sequence, not separate stages:

- **Fan-out** `operation_asset_appearances` + native surrogate (F-A, F-E)
- **F-B** LP native leg (lp_a/lp_b rows + pool_id)
- **F-F** full SAC-invocations union (lasting read stream)
- **F-C** account roles → `transaction_participants` (shares F0 lib)
- **Read rewrite** `/assets/:id/transactions` = `fan-out ∪ SAC ∪ soroban-events`
- **Soroban-era backfill**

### Ordered fundamental steps

Steps 1–5 + 7 = the core chain (F0-bound). Steps 6, 8–12 = the independent /
semi-independent stages (A–F), run as capacity allows.

1. **Foundation + gate** — shared lib `emit_asset_participations(details, result)`;
   deterministic `leg_index` (fixed XDR-order enumeration, no HashMap); full 25
   op-type → role map; **differential test** live↔backfill byte-identical
   (blocking gate — nothing ships until green). [G-schema-and-roles](notes/G-schema-and-roles.md).
2. **Schema** — `operation_asset_appearances` RMT
   `(asset_id, ledger_sequence, transaction_id, application_order, role, leg_index)`
   - `pool_id`, native surrogate, drop amount-fold. Inline-amount columns = a THIN
     perf choice (default: skip).
3. **Live ingest** — wire the lib into the `stage` write path (rows per-(op,asset,role) forward).
4. **Backfill** (new crate — [[feedback_backfill_new_crate]]) — one archive-re-parse
   harness over the Soroban era, emitting for ALL consumers (fan-out + L2 +
   participants) in a SINGLE sweep, not N full passes.
5. **Read rewrite** — composed `fan-out ∪ SAC ∪ soroban-events`, native surrogate,
   keyset pagination, drop empty-native early-return. F-F full union + F-B land here.
6. **L2 `soroban_events` decode** — from/to/amount columns, participant index,
   contribute the stream to step 5.
7. **Account roles (F-C)** — crossed-offer counterparty / claimants / inflationDest
   / revoke-target → `transaction_participants` (own dedup + backfill via the lib).
8. **Contract-as-holder (F-D, K2-5)** — read-side `soroban_contracts` union.
9. **Fee-bump** — `inner_tx_hash` index + fee_source attribution.
10. **Search** — by-name asset + SAC C-address resolve.
11. **Hygiene (K4)** — KPI-window, fold-vs-count, nullable-aggregate sweep, NFT pending.
12. **FE render** — per-op-type headline + progressive detail.

Cross-cutting each step: `docs/architecture/**` (ADR 0032), API-types regen on any
shape change, validate vs Horizon / stellar.expert.

### Step 6 status (2026-07-08) — L2 events decode: 6a decoder BUILT

- **6a (done):** `event_filters.rs::parse_token_event` — the FULL SEP-41 decode
  the transfer-only path never had: `TokenEventKind::{Transfer, Mint, Burn,
Clawback}` with official topic shapes ([transfer, from, to] / [mint, admin,
  to] / [burn, from] / [clawback, admin, from]), C-address (contract)
  participants first-class (the K2-3 gap), amount from numeric data OR the
  post-P23 unified-event map shape (`{amount, to_muxed_id}`), `approve`
  deliberately excluded (allowance ≠ movement). 7 new TDD tests; crate 322
  green; clippy clean. `parse_transfer` kept (API E10 consumer).
- **6b (done):** `soroban_events` gains APPEND-ONLY decode columns `from_id` /
  `to_id` (`ids::address_id` surrogates — accounts AND contracts first-class) +
  `amount Nullable(Int128)` (raw token units; i128 rows precedent:
  total_supply/shares). NO `kind` column — `signature` already carries it
  (recorded lean call). Staging decodes via `parse_token_event` at the event
  push (pure fn of topics+data → live/backfill identical). Non-token events →
  NULLs (recorded skip). Column-pin + 3-case staging test (transfer G→C, burn,
  non-token); sweep 626 green, workspace clippy clean. **OPS:** prod = 3×
  `ALTER TABLE soroban_events ADD COLUMN` (metadata-only, instant) + the one
  era re-parse to source values.
- **6c (done):** participants indexing fixed at staging — EVERY token-event
  kind (was: transfer only → K2-7 mint/burn/clawback now register) and BOTH
  address kinds (was: G-only → K2-3 contract participants now land in
  `transaction_participants` via the shared `ids::address_id` surrogate space,
  golden-pinned `address_id(G)==account_id(G)`). Contracts do NOT leak into
  `accounts` stub rows (guarded by test). Legacy `transfer_participants`
  DELETED (unused after the switch — delete-on-the-fly policy). Semantic note:
  `transaction_participants.account_id` now holds ADDRESS surrogates (G and C);
  column name kept (rename = pointless 2.16B-row churn), documented here.
  Sweep 627 green, workspace clippy clean. **OPS:** historical contract/mint/
  burn participants arrive with the same one era re-parse.
- **6d (done):** the K3-4 read unions, resolved by VERIFYING what actually
  needs a new arm: (a) **asset page — NO events arm** (recorded decision: the
  token contract emitting an event is always in the call graph, so the
  invocations arm already yields those (ledger, tx) keys; a fourth arm would
  dedup to nothing); (b) **account pages — already covered by 6c** write-side
  (token-event participants land in `transaction_participants`, the account
  read path's existing driver); (c) **contract page — the REAL gap, fixed**:
  `fetch_invocation_appearances` is now a 2-arm keyset merge — invocations ∪
  `transaction_participants` by the contract's address surrogate
  (`address_id(C)==contract_id(C)`), catching PASSIVE token receipt (transfer
  TO a contract never enters the call graph). Caller enrichment stays
  invocation-arm-only. `merge_tx_keys` promoted to `common/ch.rs` (shared with
  the assets union; tests moved with call-site qualification). Sweep 627
  green, workspace clippy clean.

**Step 6 COMPLETE (6a–6d).** Historical decode columns + participants arrive
via the one era re-parse.

### Step 7 status (2026-07-08) — account roles (F-C / K1-5) BUILT, tests green

`ExtractedOperation.counterparties: Vec<String>` — parser-emitted
(`operation.rs::extract_counterparties`, body + success-gated result in hand):
**crossed-offer SELLERS** from result order-book claim atoms incl. the ancient
`ClaimAtom::V0` ed25519 shape (the common taker path — previously discarded),
**CB claimants** (previously only their COUNT survived), **inflationDest**,
**revoke-sponsorship targets** (signer account + owned-entry owner for
Account/Trustline/Offer/Data keys; CB/LP/contract keys have no single owner —
recorded N/A). Staging registers them into `transaction_participants` +
`accounts` stubs (G-gated). Fixed XDR order → deterministic live vs backfill;
historical rows via the same one era re-parse. 4 parser tests + 1 staging
test; sweep 632 green, workspace clippy clean.

### Indexer pattern/anti-pattern catalog (2026-07-08,)

5-agent panel (web reference research: Horizon processors/verifyRange,
stellar-etl, TheGraph deterministic-halt, Firehose/Substreams + 3 code hunters

- judge, every claim code-verified). 28-item catalog on the flow-map §7.
  **Judge verdict: closer to canon than most hand-rolled indexers** — we HAVE the
  4 hardest properties (single live/backfill path; extract-once archive;
  idempotent replay commit-marker+RMT; total panic-free value parsing). Distance
  = 3 meta-disciplines:

* **CRITICAL — silent protocol-version absorption:** 6 independent
  `TransactionMeta` wildcards (event.rs:116, ledger_entry_changes.rs:100,
  contract.rs:55, invocation.rs:439/553, operation.rs×2). A Protocol-24 V5
  meta compiles clean and yields ZERO events/changes/participations while txs
  - commit marker still land. `ledger_version` extracted, never consulted.
    **Adoption #1:** central `meta.rs` accessor (V0-V2 explicit legacy, V3|V4
    real, NO wildcard) + UnsupportedMetaVersion error.
* **MAJOR — no parser/run provenance:** no row says which code wrote it; every
  emit change (incl. 0359) → unlabeled stale rows; full re-parse is the only
  healing (paid 2×: 0261, 0359). **Adoption #2:** `ingest_runs` audit table +
  `parser_version` LowCardinality column on facts — stamp during THIS backfill
  (nearly free now).
* **MAJOR — fence-by-convention failed:** 0359 asset arms shipped WITHOUT the
  `max(sequence)` commit fence (accounts/contracts have it) → pre-commit head
  keys → INNER JOIN drops → silent pagination truncation at the live head.
  **FIXED same day** (fence added to both arms); proves R1 (feed engine) must
  own the fence by construction. **Adoption #3.**
* Other MAJOR (pre-0359, separate tasks): partition-pinned filtered global
  lists (false end-of-list at 500k boundary); overscan×4 without refill; NO
  poison-pill quarantine (a permanently-failing ledger stalls live tail
  forever); 3 divergent asset-code normalizations. **MAJOR in 0359 scope:**
  sibling wildcards behind the exhaustive gate (emit*asset_participations /
  extract_counterparties / claim_atoms `*=>` — a NEW op type ships with zero
  participations silently; fix = explicit variant listing).
* MINOR×5 + pattern scorecard (5 HAVE / 7 PARTIAL / 1 MISSING: no automated
  verify-range vs Horizon — G-role-crossref is the contract to automate).

### Deep architecture analysis (2026-07-08, karolkow: "za płytko — wzorce, od zera")

Knowledge graph built with /graphify (AST 58 files + 3 semantic extractors:
1333 nodes / 2521 edges / 65 communities; `graphify-out/graph.html`). Three
hypotheses confirmed by graph edges, full analysis on the flow-map artifact §6:

- **God Payload (the MAIN structural debt, pre-dates 0359):** `details: JSON`
  is the de-facto inter-layer contract — parser serializes typed XDR to strings
  ("CODE:ISSUER"), staging recovers via 4 string-matchers
  (`OpTyped::from_details`, `op_participant_str_keys`, `split_asset_ref`,
  `gross_volume_a_by_pool`). The 0359 root cause (native → `(None,None)`) was a
  bug OF this pattern. 0359 added a typed side-channel (participations,
  counterparties) NEXT TO it → claim atoms now live in 3 representations, asset
  identity in 3 encodings (`concept_dual_extraction` node).
- **Arms→merge→hydrate read pattern:** 2 implementations + accounts precursor;
  page assembly TRIPLICATED across api modules (7 similarity edges).
- **Inverted-index family:** 5 instances, no shared abstraction; graph bonus
  finding: **`account_balances_current` is DEAD — nothing writes it** (drop in
  ops phase).

**Architect verdict:** full from-zero rewrite NO (billions-row tables stay;
read-path "oddities" are scars of real prod incidents — 10 rationale nodes).
Strangler refactor YES, 3 self-contained steps (each a candidate follow-up
task): **R1** shared tx-feed engine in api (kills triplication, arms become
declarations; no data changes) → **R2** typed `OpFacts` IR in the parser,
details-JSON becomes a DERIVED view, staging drops all string-matching (kills
the God-Payload bug class; pure code refactor, NO re-parse needed, can land
after backfill) → **R3** ops-phase cleanup (dead table, 0334 bloom, old asset
columns). Framing: 0359 fixed the DATA MODEL fundamentally; R2 fixes the
LAYER CONTRACT the same way.

### Review cut EXECUTED (2026-07-08, karolkow: "wykonaj cięcie 1-5 + minory")

All verdict items applied; diff shrank 2677→2285 insertions (+147 more
pre-existing dead lines deleted); 608 tests green, workspace clippy clean:

1. **Pagination bug FIXED** — arm A now dedups server-side (`LIMIT 1 BY
(ledger, tx)` before `LIMIT`); Rust collapse + overfetch deleted.
2. **leg_index DELETED** everywhere (struct, sort key, ordinal fn, doctrine
   docs, tests). Table key: `(asset_id, ledger, tx, application_order, role)`;
   identical (asset, role) legs collapse in RMT — unobservable by any reader.
   **`type` column DELETED**; **`role` KEPT** (karolkow-approved insurance).
3. **soroban_events decode columns DELETED** (from_id/to_id/amount + the 2nd
   parse_token_event call). Participant registration (consumed) stays — parse
   runs ONCE per event now.
4. **Emitter refactored to typed XDR** — `emit_asset_participations(&OperationBody,
result, changes)`; parse_asset_ref + malformed-string tests gone; bonus:
   claim-CB now matches the op's `balanceId` typed (review minor fixed, tested
   with a 2-CB fixture).
5. **Tautology tests DELETED** — differential test (→ .trash) + the f(x)==f(x)
   determinism test. OPS runbook note: the "differential gate" step is VOID —
   both paths are one function by construction.
6. Minors: stale "3-arm" docs fixed, arm naming unified, claim_atoms double-doc
   merged, merge_tx_keys doc placement fixed, **dead Transfer/parse_transfer/
   is_transfer_event API deleted** (zero consumers; stale "API E10" doc claim).
   DEFERRED (recorded): dual participant-extraction unification; 0334 bloom +
   old asset columns drop (ops, post-backfill); tx-detail contract participants
   (FE).

### Adversarial architecture review (2026-07-08, karolkow request) — VERDICT

4 independent fresh-eyes reviewers (code-bloat / data-model-from-scratch /
pipeline / YAGNI lenses; judge = verified in code after the judge agent hit a
session limit). **Unanimous: the skeleton is sound — NOT plaster-on-plaster**
(fan-out asset-leading table, native surrogate, one shared emitter, G+C
participant registration, keyset-merged arms = minimal correct architecture;
old early-return DELETED not patched). Over-engineering is real but
concentrated in 5 spots + 1 real bug:

1. **CRITICAL bug (4/4): arm A pagination silently truncates** — the
   overfetch×8 + Rust collapse was copied from the OLD table (where the
   predicate wasn't the leading key) and lost the escalation re-fetch; a
   many-row tx (100-op airdrop) eats the window → short page → finalize_page
   sees "no more" → older history unreachable. FIX: SQL dedup (`LIMIT 1 BY`),
   delete collapse machinery (~40 lines).
2. **leg_index (4/4)** — protects duplicates NO query can observe (the only
   reader DISTINCTs to tx); O(n²) ordinal + "byte-identical" doctrine +
   sort-key slot → DELETE.
3. **soroban_events from/to/amount columns (4/4)** — zero readers + DOUBLE
   parse_token_event per event in hot ingest → DELETE columns + 2nd parse;
   keep participant registration (consumed). ADD COLUMN returns when an
   endpoint exists.
4. **Emitter re-parses its own JSON (3/4)** — takes details strings while the
   call site holds the typed OperationBody; key rename = silent data loss →
   REFACTOR to &OperationBody; parse_asset_ref + malformed-string tests die.
5. **Differential test = tautology (4/4)** — same pure fn called twice in one
   process; cannot fail → DELETE (fixture tests already pin determinism).

6. MINORs: stale "3-arm" comments, dead re-export, dual participant-extraction
   mechanisms (merge later), dead 0334 bloom + old asset columns (ops-phase
   cleanup, post-backfill), claimed_cb_asset ignores balanceId (3-line fix),
   contract participants invisible on tx-detail (FE, separate).

Net: ~300–450 lines deletable while FIXING a real bug; same user-facing
product. Full map + verdict: claude.ai/code/artifact/0d4868da (flow-map).

**CORE CHAIN COMPLETE: steps 1–7 all BUILT.** Remaining: independent stages
A (contract-as-holder reads), B (fee-bump), C (search), E (hygiene), D/FE
(render sweep), then OPS (schema apply + era re-parse + Horizon validation per
[G-role-crossref](notes/G-role-crossref.md)) + docs/architecture + API-types
regen check + the deferred amount decision.

### Pre-backfill hardening (2026-07-09, karolkow "jedź zbiór przed-backfillowy")

Adoption #1 + asset_code unification LANDED (uncommitted; full-workspace
`cargo test` + `cargo clippy -D warnings` green). DB-mitigation / disk-shrink
thread REMOVED from task scope (karolkow) — no grain change, role + app_order
stay.

- **`xdr-parser/src/meta.rs` (NEW) — central `TransactionMeta` accessor.** The
  ONLY place matching `TransactionMeta` variants; exhaustive, NO wildcard, so a
  future `V5` (Protocol 24) breaks compile in ONE file instead of being silently
  absorbed (the CRITICAL). 7 duplicated dispatch sites (event /
  ledger_entry_changes / contract / invocation×2 / operation×2) migrated to its
  accessors; the duplicate `soroban_return_value` deleted. `transaction.rs` now
  flags a non-Soroban (pre-V3) meta as `parse_error` (fail-loud + `warn`, not a
  silent empty record). Sibling emitters: `emit_asset_participations` made
  exhaustive over `OperationBody` (a new op type can no longer ship with zero
  participations — the exact offers-stored-zero bug class); `claim_atoms`'
  unguarded `OperationResultTr` wildcard covered by a compile canary.
- **`xdr-parser/src/asset_code.rs` (NEW) — one shared `asset_code_str`.** Killed
  the 3-way normalization fracture (`<invalid>` sentinel vs U+FFFD lossy vs
  cut-at-first-NUL). Policy: bytes up to first NUL; valid UTF-8 → string, else
  `0x`+hex — no shared sentinel (distinct malformed codes → distinct ids).
  Conformant codes byte-identical → NO surrogate-id churn; only malformed codes
  re-key. Routed all 4 sites (operation / participations / ledger_entry_changes
  / sac); nothing downstream depended on `<invalid>`.

**Pre-backfill set COMPLETE.** Provenance (`ingest_runs` / `parser_version`) is
DROPPED from scope (karolkow 2026-07-09): no version stamping — if a parser bug
is ever found, re-run the FULL backfill (the run itself made faster over time),
not targeted version-mismatch re-heals. Same call-shape as the disk-mitigation
drop. So the only correctness-critical pre-backfill work (meta.rs + asset_code)
is done; nothing else must ride the re-parse.

### Step 5 status (2026-07-08) — read rewrite BUILT, tests green

`/assets/{id}/transactions` = a keyset-merged 3-arm union
(`api/src/assets/queries.rs::fetch_transactions`):
**A** fan-out `operation_asset_appearances` (asset_id-leading PK seek; native
first-class) ∪ **B (REMOVED, karolkow 2026-07-08)** — no transitional legacy arm: delete
legacy code on the fly, the era re-parse fills the fan-out from scratch;
**OPS WARNING recorded in the fn doc**: until the backfill runs, classic
history shows only post-deploy ledgers (SAC stream is complete) — run the 0359
backfill in the same rollout ∪
**C→B** `soroban_invocations_appearances` keyed on the SAC surrogate (F-F: XLM's
~3.9 M SAC invocations) or the token contract itself (type-3). Same cursor on
every arm; k-way merge + dedup + truncate in Rust (`merge_tx_keys`), then the
existing header/aggregate join-back (`fetch_tx_page`). **The native
early-return is GONE** (handler computes `ids::asset_id` — native =
`hash64("native")`, so `/assets/native/transactions` returns SAC activity NOW
and full classic history after the backfill). Dead `asset_predicate_present` /
`AssetIdentity` removed. No wire-shape change → no API-types regen. 6 new
merge/collapse unit tests (dedup across arms, DESC/ASC, tiebreak, truncate,
adjacency-fold); sweep 619 green, workspace clippy clean.

**Step 4 revision (karolkow):** the targeted `asset-participations-backfill`
bin was DROPPED (moved to .trash) — downloading dominates backfill cost and the
standard `backfill-runner Run` (sync+parse+write, resumable) now populates the
new table automatically via the step-3 wiring; step 6's L2 decode will need the
same era pass anyway → ONE full re-parse covers all consumers (the F0
single-sweep idea). The env-gated differential test STAYS
(`backfill-runner/tests/participations_differential.rs`) — it pins parse+stage
determinism, independent of any bin.

### Amount column — REVERTED (karolkow, 2026-07-08)

Built end-to-end, then **reverted on karolkow's call** the same day (table stays
THIN: find-keys + type only). The semantics below are kept as the recipe if the
decision flips. **Consequence, recorded:** amounts were capturable ~free inside
the one era re-parse; adding them later requires ANOTHER era pass to SOURCE the
values (`ADD COLUMN` itself stays metadata-only). Decide before the ops-phase
backfill runs — flipping after it means a second full pass.

#### (reverted recipe) Amount column — was BUILT, tests green

`Participation.amount: Option<i64>` + `operation_asset_appearances.amount
Nullable(Int64)` (last column; not in sort key). Semantics per role: declared
(payment/sendAmount/destAmount/offer size/escrow/clawback/startingBalance),
REALIZED from the result (strict-send delivered `last.amount`, account-merge
moved balance, claim-atom fills both legs), meta-recovered (claim/clawback-CB
entry amount, **LP legs = reserve deltas of the State→Updated pool-entry pair**
— exact, not the body's max/min bounds). `None` = unknown/not-applicable
(trustline, authorize, an offer's other leg, strict-receive sent total).
Supersedes the earlier "THIN = skip amounts" default — capture is ~free in the
same pass; the LIST can still render thin. 493 tests green (amounts pinned
per-role in ~20 tests + reserve-delta test), clippy clean.

### Step 4 status (2026-07-08) — backfill worker BUILT, tests green

- **Bin:** `backfill-runner/src/bin/asset-participations-backfill.rs` (pattern:
  `pool-ids-backfill`). Local-archive iteration (64k partitions), watermark
  resume, dry-run, throttle, per-partition PartitionWriter commit/abort.
  **Zero duplicated emission logic** — calls the same `parse_ledger` →
  `stage::prepare` as live ingest and takes `op_asset_rows` whole; idempotent
  re-runs (deterministic rows + RMT replace on the identical sort key).
- **Differential gate:** `backfill-runner/tests/participations_differential.rs`
  — env-gated on a real archive ledger (`XDR_FIXTURE` / `.temp` fallback,
  repo's fixture-test pattern): live vs backfill pass byte-identical
  (`OperationAssetAppearanceRow: PartialEq`), rows non-empty, every
  `asset_id != 0`, every role decodes. Skips without a fixture (pure-function
  determinism already unit-tested). **OPS RUNBOOK (blocking): run this test
  against a synced archive ledger BEFORE starting the production backfill.**
- Unit tests: archive path-layout pin (drift would silently read nothing),
  partition boundaries, watermark round-trip + corrupt-content fallback.
- **Verification:** 492 tests green (domain/xdr-parser/db-clickhouse/
  backfill-runner); workspace clippy `-D warnings` clean.

### Steps 2+3 status (2026-07-08) — schema + live-ingest wiring BUILT, tests green

- **Role dictionary:** `domain::ParticipationRole` (`participation_role.rs`),
  `#[repr(i16)]`, discriminants FROZEN 0=payment…12=lp_b, golden-pin +
  round-trip + serde tests (snake_case — avoids the latent `lowercase` serde↔
  as_str mismatch found in older enums). Parser re-exports it as `Role`.
- **Schema:** `operation_asset_appearances` in `schema/init.sql` — RMT,
  `PARTITION BY intDiv(ledger_sequence, 500000)`, `ORDER BY (asset_id,
ledger_sequence, transaction_id, application_order, role, leg_index)`,
  columns + `type` (denormalised op type). THIN; no pool_id (recoverable via
  `operations_appearances.pool_ids` join — no S3 re-parse needed to add later).
  Table-count guard test updated 27→28.
- **Threading:** `ExtractedOperation.participations` populated inside
  `extract_operations` (op body + `op_result` + NEW `op_meta_changes(tx_meta,
i)` per-op changes accessor; 0-based meta index). Failed-tx policy: body legs
  always (parity with operations_appearances), `traded` only on success
  (tx_op_results gate), meta-recovered assets naturally absent on failure.
- **Staging + writer:** `OperationAssetAppearanceRow` (column-pin test),
  `participation_asset_id` (native = `ids::asset_id(0,"",0,0)` first-class;
  credit = code:issuer-surrogate), rows built in the ops loop (no fold — the
  grain IS the row), writer slot + commit end. Backfill parity free (same
  `parse_ledger`).
- **Verification:** 488 tests green across domain/xdr-parser/db-clickhouse/
  backfill-runner; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **Ops note (step "ops later"):** prod CH needs the manual `CREATE TABLE`
  (init.sql is fresh-install only — no migration mechanism); add to the ops
  runbook when the ops phase starts.

### Step 1 status (2026-07-08) — foundation function BUILT, tests green

`crates/xdr-parser/src/participations.rs` — `emit_asset_participations(op_type,
details, op_result, op_changes) -> Vec<Participation>` (asset, role, leg_index).
TDD throughout (every behavior red→green); **27 unit tests**, crate suite 312
green, clippy `-D warnings` clean. Covered: offers (sold/bought — the flagship
zero-asset bug), payment (ONE row per asset — karolkow), path-payment endpoints
(sent/received; hops via trades), **order-book claim atoms → traded both legs**
(the recon gap — extraction generalized in `operation.rs::claim_atoms`),
trustline, escrowed, clawback, authorize (set_trustline_flags), create-account +
account-merge (native), claim/clawback-CB **asset recovered from same-op
`LedgerEntryChanges`**, LP deposit/withdraw **pool assets recovered from the pool
entry in op changes** (lp_a/lp_b), deterministic same-input-same-output +
body-legs-before-trade-legs ordering tests, malformed/missing-asset robustness.

**Deferred (recorded, not silent):** `allow_trust` (deprecated op; asset is
code-only, issuer = op source — needs the source account passed in);
`Participation.pool_id` link column (write layer reads it from details);
the full live-vs-backfill byte-identical differential test (needs the backfill
path to exist — step 4 gate; determinism covered at the function level now).

### Recon (2026-07-08, code-verified) — corrections to the notes

A read-only code recon before building step 1 confirmed the diagnosis but found
five things the notes got wrong or under-stated:

- **Shared function home = `crates/xdr-parser`** (owns `format_asset`,
  `claim_lp_atoms`, all XDR `Asset` handling). A backfill crate **already exists**
  (`crates/backfill-runner`, with sibling bins `pool-ids-backfill`,
  `metadata-backfill`) and already depends on `xdr-parser` — so the op-participations
  backfill is a NEW BIN there, and the shared emit fn lives in `xdr-parser`
  (both live-ingest and backfill-runner already depend on it). Re-check the
  [[feedback_backfill_new_crate]] rule here: it was enrichment-specific; classic
  re-parse backfills already live in `backfill-runner` as sibling bins.
- **Order-book trades are NOT extracted today — a real scope addition to step 1.**
  `claim_lp_atoms` (`operation.rs:100-122`) keeps ONLY liquidity-pool atoms;
  `ClaimAtom::OrderBook` (the real cross-asset offer trades — the unbounded
  `traded` grain the whole fan-out rests on) is **dropped at `operation.rs:118-121`**
  and consumed nowhere. So "the parser already has all legs" is true only for the
  op BODY (sendAsset/destAsset/path/selling/buying — all present in `details`), NOT
  for result-side order-book trades. Step 1 must GENERALIZE atom extraction to yield
  order-book atoms too — genuinely new parser work, not a projection tweak.
- **Native surrogate is a stable i64 but NEGATIVE.** Pinned golden
  `ids::asset_id(0,"",0,0) == -6_959_166_271_784_855_184` (`ids.rs:199`). The
  concept (stable `hash64("native")`, lower 64 bits of cityhash-128) is right; the
  "positive surrogate" wording across the notes is loose — sign is irrelevant, it's
  just a stable key.
- **`ids::asset_id(asset_type, code, issuer_id, contract_id)` takes ALREADY-HASHED
  surrogates**, not StrKeys (`ids.rs:138`). The parser emits StrKeys, so the write
  path must `ids::account_id(issuer_strkey)` first (as the existing fold does at
  `stage.rs:972`).
- **Offers are dropped harder than "a `_` arm".** They aren't in
  `OpTyped::from_details`'s match at all → both `selling` AND `buying` yield zero
  asset on the appearance row. Parser output (`details`) DOES carry both legs.
- **Parsed op = JSON, not a typed enum.** `extract_op_details` emits
  `(OperationType, serde_json::Value)`; assets live as strings `"native"` /
  `"CODE:ISSUER_STRKEY"` in `details`. The emit fn reads from that JSON + the
  `OperationResult` for atoms. `OperationAppearanceRow` (`rows.rs:309`) stores
  `asset_code` + `asset_issuer_id` separately and never calls `ids::asset_id` — the
  new participations table would be the first op-side consumer of the unified
  `asset_id` surrogate.

## Revised plan — post devils-advocate (2026-07-07) — SUPERSEDED by the Plan above

> **SUPERSEDED (karolkow, 2026-07-08) by "## Plan" above.** The Phase-0 interim and
> the sibling-split are DROPPED (no plasters, one task). Kept for history / rationale.

A 7-agent adversarial pass ([S-devils-advocate](notes/S-devils-advocate.md))
confirmed the diagnosis but found the single-epic packaging and several
justifications overreach (native-first-class does not justify fan-out; "hops
redundant" is conditional; the loss counts are unverified; the FE is THIN so
nothing renders the fan-out grain today). **The fan-out stays the likely
end-state, but the work is now SEQUENCED and the epic is SPLIT.** This supersedes
the "all in one epic / build the full fan-out first" framing below.

**Decisions (karolkow, 2026-07-08):**

- **Road B — iterative / phased** (easier to build step by step), NOT one
  big-bang. But each phase is a complete fundamental fix for its slice, and the
  sequence is **committed through the full fan-out** — no stopping at plasters.
- **Historical completeness = YES.** Every phase that lost data (offers,
  path-payment legs) includes its backward re-parse; asset pages must be complete
  matching mature explorers (stellar.expert shows per-asset amount + role per
  operation, which we don't). Native is the exception — its data already exists
  correctly, so Phase 0 needs no backfill.
- **Backfill scope = the covered range only (Soroban era).** Measured 2026-07-08:
  `operations_appearances` holds only ledgers **50,457,424 – 63,376,009**
  (~Feb 2024 → now = the Soroban era); pre-Soroban Stellar history is NOT in CH,
  so "complete backward data" is bounded to that range — the re-parse window is
  ~13 M ledgers, not all of Stellar. See
  [R-prod-evidence-cross-validation](notes/R-prod-evidence-cross-validation.md).
- **No separate ADR.** The design (table shape, role mapping, keying, backfill)
  is recorded in this task + its notes ([G-schema-and-roles](notes/G-schema-and-roles.md))
  instead. If the evergreen-docs gate ever needs a formal ADR, the material is here.

**Sequencing (ship by confirmed value):**

- **Phase 0 — now, ZERO backfill.** Native positive surrogate (read-side) + F-F
  SAC-union cheap win (`assets/queries_ch.rs:222` already loads the unused
  `sac_contract_surrogate`). Closes the flagship symptom
  (`/assets/native/transactions` empty) + surfaces native's ~3.9 M XLM-SAC
  transfers. No new table, no re-parse.
- **Phase 1 — narrow.** Offers indexed by asset (the one other confirmed HIGH,
  ~1.37 B). Check whether an offers-only fan-out / single offer-asset leg
  suffices before committing the universal table.
- **Phase 2 — the full role-tagged fan-out + full historical backfill
  (COMMITTED).** The unified per-(op, asset, role) table + re-parse of ALL
  affected history (path-payment legs + the remaining op types). Capture per-leg
  amount + the trade grain in the same pass — cheap, `OperationResult` is already
  deserialized (`operation.rs:100-171`). **NOT gated on the frontend** — the
  earlier "gate on a render spec" was retracted (karolkow, 2026-07-08): backward
  completeness is built regardless of what the page renders today. The FE stays
  THIN for now; the DATA is complete so a future fat render needs no re-backfill.

**Two CRITICAL gates before building Phase 2 (design recorded in this task — no separate ADR):**

1. **Split Layer-2 out.** `soroban_events` (9.5 B) token-flow decode is a
   co-equal epic (per [R-audit-inventory](notes/R-audit-inventory.md)) — own task
   - acceptance gate. Fee-bump / NFT / search / aggregate-hygiene → sibling tasks
     referencing 0359 but shipping independently. Keep 0359 = Layer-1 classic
     participation + the F-F cheap win. **(Sibling task files to be spawned on
     develop, per the new-tasks-on-develop convention — not created on this
     feature branch.)**
2. **`leg_index` = content-addressed + differential test.** Derive from a stable
   hash of `(source: body|result, atom_ordinal_in_own_xdr_vec, asset_sold,
asset_bought, amount_sold, amount_bought)` — NOT iteration/assembly order. One
   shared lib called by both live-ingest and backfill, plus a byte-identical
   differential test across both paths. Until it exists, fan-out correctness is
   UNPROVEN and silent-corruption-prone.

**Also before the ADR (non-blocking):** total op-arm → role mapping table (add
`clawback` + a trustline-flag-target role; decide create-account / inflation;
resolve PoolShare 3-entity keying — it is in the sort key); state the
Horizon-endpoints vs `/trades` parity contract; re-derive loss counts on FINAL;
evaluate a bounded op-type-targeted backfill (only ledgers touching the defective
op types); enumerate native's TWO keys (classic-leg surrogate + native-SAC
surrogate) in the union predicate + golden-pin `hash64("native")` on the backfill
path.

## Sub-work (original scoping — superseded by the Revised plan above; Layer-2 + siblings now split out)

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

> **SUPERSEDED (karolkow, 2026-07-08) — the Phase 0/1/2 sequencing below is DROPPED;
> see "## Plan" above.** No Phase-0 interim, no gate-on-FE-spec. The list stays the
> FULL end-state; ordering is now the "Ordered fundamental steps" in the Plan.
> (Original note:) Sequenced post devils-advocate: **Phase 0** = native surrogate +
> F-F; **Phase 1** = offers; **Phase 2** = the rest gated on a render spec + the two
> gates. Don't treat these as one atomic gate.

- [ ] Design recorded in task (no separate ADR — karolkow) + `docs/architecture/**` updated (schema + query docs)
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

## Notes

- [R-audit-inventory](notes/R-audit-inventory.md) — root cause + full findings audit (F-A..F-F, K1–K4 clusters, cleared items, workstreams)
- [R-external-cross-validation](notes/R-external-cross-validation.md) — Horizon / Hubble / stellar.expert / indexers; completeness facts (7-asset ceiling, claim-atom trade grain)
- [S-diagnosis-calibration](notes/S-diagnosis-calibration.md) — calibrated thesis, red-team corrections, Stanisław corroboration + code-verified second-slot check
- [S-design-options](notes/S-design-options.md) — 6 modeling options, red/blue verdicts, convergence on the role-tagged fan-out, open ADR decisions
- [S-field-comparison-fat-thin](notes/S-field-comparison-fat-thin.md) — 4-way field matrix (our FE vs THIN vs FAT vs stellar.expert) + body/result provenance + tiered capture recommendation (fat/thin decision input)
- [S-devils-advocate](notes/S-devils-advocate.md) — 7 adversarial agents vs every decision (core solid, packaging overreaches); 13 challenged claims, the sequence-don't-build-ahead recommendation, 2 CRITICAL pre-ADR gates
- [G-schema-and-roles](notes/G-schema-and-roles.md) — design answers in lieu of an ADR: before/after row shape, where roles + op-type come from, old-vs-new table relationship, op-type → role mapping, more examples
- [R-prod-evidence-cross-validation](notes/R-prod-evidence-cross-validation.md) — direct prod-CH (`chq`) stats + 4 cross-validated examples (our DB ↔ Horizon ↔ stellar.expert), both links each; inferred stellar.expert schema
- [S-tx-render-audit](notes/S-tx-render-audit.md) — /ux-expert audit of the transaction-detail render (normal misleading "Sent 1 XLM" + advanced raw dump); root cause in `humanizeOp.ts`; redesign wireframe + spec; **separate FE task** (on develop)
- [G-architecture-audit](notes/G-architecture-audit.md) — serialized 28-item pattern/anti-pattern catalog + R1-R3 strangler plan + adoptions #1-3
- [G-role-crossref](notes/G-role-crossref.md) — role ↔ XDR field ↔ Horizon effect (official grounding + per-arm ops validation contract)
- [G-spawn-plan](notes/G-spawn-plan.md) — spawn-readiness review: child-task specs (Phase 0, Layer-2, contract-holder, fee-bump, search, FE render), full finding→home coverage map, spawn order

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
