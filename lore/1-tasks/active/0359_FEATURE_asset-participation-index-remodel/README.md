---
id: '0359'
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

## Revised plan — post devils-advocate (2026-07-07)

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

> Sequenced post devils-advocate (see Revised plan): the list below is the FULL
> end-state. **Phase 0** = native surrogate + F-F (`/assets/native/transactions`
> real + LP native filter); **Phase 1** = offers by asset; **Phase 2** = the rest
> (the fan-out ADR + backfill), gated on a frontend render spec + the two CRITICAL
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
