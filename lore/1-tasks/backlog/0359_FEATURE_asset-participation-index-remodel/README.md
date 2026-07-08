---
id: '0359'
title: 'Asset-participation index re-model — native XLM first-class + complete per-asset activity (offers, all path-payment legs)'
type: FEATURE # fundamental data-model fix: lean participation index + ingestion + one XDR re-parse backfill + query rewrites
status: backlog
related_adr: ['0044', '0051'] # 0044 operations_appearances schema; 0051 SAC-as-facet / native surrogate convention
related_tasks: ['0348', '0331', '0334', '0243', '0333', '0199'] # 0348 = F2 origin; 0331/0334 = balances native-surrogate precedent; 0243/0333 = assets CH queries + bloom idx; 0199 = LP analytics owns per-op AMOUNTS
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
      SEQUENCE the work and split the epic. Two CRITICAL pre-ADR gates:
      content-addressed leg_index + differential test; scope split. Corrected
      overstated cost claims in S-field-comparison (ADD COLUMN is metadata-only;
      Tier-2 result already deserialized; ZSTD 20-40x doesn't apply to
      Decimal128 amounts).
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      karolkow decisions: (1) Road B — iterative/phased, but committed through
      the full fan-out (no stopping at plasters). (2) Historical completeness =
      YES: every phase that lost data re-parses its backward history; asset pages
      must match mature explorers years back. (3) NO separate ADR — design
      answers recorded in the task (G-schema-and-roles: before/after row shape,
      role source, op-type→role mapping).
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      Added R-prod-evidence-cross-validation (chq prod stats + per-example
      Horizon/stellar.expert cross-validation) and G-spawn-plan (child-task
      decomposition). Measured: operations_appearances = 6.405 B rows, Soroban
      era only (ledgers 50,457,424–63,376,009, ~13 M ledgers); offers 100% empty
      asset; a 10-op path payment touches 12 distinct assets, only dest legs kept.
      Live-inspected the front: humanizeOp renders a MISLEADING "Sent 1 XLM" for a
      path-payment self-swap (S-tx-render-audit). Recorded the complete 25-op
      type→role mapping + content-addressed leg_index (G-schema-and-roles).
  - date: 2026-07-08
    status: backlog
    who: karolkow
    note: >
      REWRITE around one principle — the participation index stores KEYS
      (findable/sortable), never DISPLAY. Retracted thin/fat as a SCHEMA concern:
      render (thin/fat/role-inline) is a deferred FE/read-query decision vs Figma,
      reversible with NO re-backfill — EXCEPT inline list amounts (archive-only →
      one flagged upgrade path). Payload columns of the old fat table go to their
      real owners: amount → 0199 analytics / archive-on-drill-in; source/
      destination → account-participation; contract_id → contract index; pool_ids
      → LP snapshots — none forced into the asset index. Verified vs prod:
      transaction_participants is only account→tx (3 cols, no op/direction) so it
      does NOT already cover the account columns; disk 95.6% full (78 GiB free) →
      the backfill migrates PARTITION-BY-PARTITION (migrate→validate→drop old
      partition→reclaim), never holding 2× copies; net size honest-TBD, NOT a
      promised "frees space". Structure: NOT split into sibling tasks (G-spawn-plan
      SUPERSEDED) — everything stays in 0359 as ordered STEPS, S3 re-parse as the
      SINGLE FINAL step (one sweep emits all consumers, never repeated). Native
      folds INTO the fan-out (positive surrogate), not a separate interim read-side
      hack — no plasters.
---

# Asset-participation index re-model

## Summary

`operations_appearances` is a **fat, denormalised per-operation table** that,
among other jobs, powers the per-asset activity list. It stores the ASSET
dimension as a **single slot** per op (`asset_code` + `asset_issuer_id` +
`contract_id`), one row per operation — so an op that touches more than one asset
records only ONE of them, and native XLM is modelled as _absence_ (empty string).
Result: swaps, offers, and path-payment legs silently vanish from most assets'
pages, and native shows "No transactions".

The fix: replace the single asset slot with a lean **per-(operation,
participating-asset, role) fan-out index** — so every asset an op touches becomes
findable, native is a first-class positive surrogate, and each other consumer of
the old fat table gets its own lean index. The lost legs live in **no ClickHouse
column** (only one asset was ever stored), so recovering history needs **one XDR
re-parse from the S3 archive** — the single, final, expensive step.

## Core principle — index = keys, not display

The participation index stores **only what makes a row findable and sortable**
(the keys), never what it displays. Everything shown on screen is joined at read
time from the table that already owns it, or fetched from the S3 archive on
drill-in. This is what keeps the index lean and the schema decoupled from Figma.

- **Index columns** = `asset_id` (native = positive surrogate), `role`,
  `leg_index`, `ledger_sequence`, `transaction_id`, `application_order`,
  `op_type`. Nothing else.
- **Row display** (source, status, time, op-type chip) = JOIN to `transactions`
  at read time.
- **Fat detail** (per-leg amounts, route, trades, price, counterparty) = the
  tx-detail view already re-parses it from the **S3 archive** on click (ADR 0029).
  Never stored in this index.
- **Payload of the old fat table goes to its real owner, not this index:**
  `amount` → **0199** analytics (volume/TVL/fees) + archive-on-drill-in;
  `source_id`/`destination_id` → account-participation index (direction);
  `contract_id` → contract-holder index; `pool_ids` → `liquidity_pool_snapshots`.
- **Render (thin/fat/role-inline) is deferred**, decided later against Figma as a
  read-query/FE change — reversible, **no re-backfill**. The ONE exception that
  would touch storage is inline list **amounts** (archive-only, un-joinable
  per-row): a single flagged upgrade path (`ADD COLUMN amount` + one narrow
  archive pass), NOT built now.

Why this matters: the fundamental fix (complete, correct asset→operation mapping)
must not wait on, or be shaped by, an unresolved frontend render decision. Build
the correct index; tune the render whenever.

## Disk — a constraint to respect, not a benefit to promise

Prod ClickHouse is **95.6% full (78 GiB free of 1.72 TiB)**; `operations_appearances`
is 93 GiB / 6.4 B rows. The fan-out adds rows (multi-asset ops) but drops the fat
payload columns; whether the net is smaller depends on where the payload lands —
**net size is honest-TBD, do NOT claim it frees space**. The backfill is safe
regardless: the table is partitioned (~32 partitions × ~3 GiB), so migrate
**partition-by-partition** (migrate one → validate → drop the old partition →
reclaim → next), never holding two full copies. The real bloat risk is Step 6's
contract/soroban-events indexes (additive over 9.6 B rows) — size each against the
78 GiB headroom before running.

## Steps

Everything lives in **this one task**, done in order. The expensive S3 re-parse is
the SINGLE FINAL step (one sweep emits rows for every consumer built below) so it
is never repeated. Each step is a complete fundamental fix for its slice — no
plasters.

1. **FE honesty (no data change).** Fix `humanizeOp.ts` — a path-payment renders a
   misleading "Sent 1 XLM" (uses `sendAmount`/`sendAsset`, drops the received leg
   - hops); it only humanises 4 op types. Correct the verb/labels per op type.
     Pure frontend, shippable alone. Spec in [S-tx-render-audit](notes/S-tx-render-audit.md).
2. **Shared emission library (the correctness gate).** One deterministic
   `emit_participations(op_details, op_result)` consumed by BOTH live-ingest AND
   the backfill. Includes the complete **25-op type→role** mapping and a
   **content-addressed `leg_index`** (hash of `(source, atom_ordinal_in_own_xdr_vec,
asset_sold, asset_bought, amount_sold, amount_bought)` — NOT iteration order).
   **Blocking gate: a differential test** parsing fixtures through both paths and
   asserting byte-identical rows. Design in [G-schema-and-roles](notes/G-schema-and-roles.md).
3. **Fan-out asset index + live emission (forward).** New
   `operation_asset_appearances` (keys only, per Core principle); native = positive
   surrogate `hash64("native")`, golden-pinned so live and backfill agree. Live
   ingest emits via the Step-2 lib. New ledgers are correct immediately.
4. **Companion lean indexes + live emission (forward).** The other consumers of the
   old fat table, each keys-only for its own concern, all via the Step-2 lib:
   account-participation direction (crossed-offer counterparty, claimants,
   inflation-dest, revoke-target); contract-as-holder (types 0/1/2 + NFT owner);
   search support (asset-by-name, SAC C-address resolve — no re-parse needed).
5. **Read-query rewrites.** `/assets/:id/transactions` reads the fan-out,
   native-inclusive, keyset pagination, JOINing `transactions` for display fields,
   and **unions the SAC-invocation stream** (F-F — `queries_ch.rs:222` already loads
   the unused `sac_contract_surrogate`) so native shows its ~3.9 M XLM-SAC transfers
   and every classic asset shows its SAC activity. Account/contract pages read their
   Step-4 indexes. Regenerate API types if any DTO/route shape changes.
6. **Validate forward data BEFORE the expensive step.** Spot-check the live-emitted
   rows for a sample of assets (incl. native) against Horizon operation objects;
   `traded` roles vs Horizon `/trades`; re-derive the loss counts on FINAL (not raw
   RMT rows).
7. **THE ONE backfill — S3 re-parse (final, expensive, never repeated).** Bounded to
   the Soroban era (~13 M ledgers), re-parsing the affected op types, emitting rows
   for the fan-out AND the Step-4 companion indexes in a **single sweep** via the
   Step-2 lib. New backfill crate ([[feedback_backfill_new_crate]] — do NOT extend
   backfill-runner). Migrate **partition-by-partition** (Disk section).
8. **Post-backfill validation + cutover.** Full cross-validation vs Horizon /
   stellar.expert years back; then deprecate/drop the old single-slot
   `operations_appearances`; update `docs/architecture/**` (schema + the
   `10_get_assets_transactions.sql` query doc, per ADR 0032).

## Owned elsewhere (out of scope for this index)

- **Per-op amounts / volume / TVL / fee_revenue** → task **0199** (LP analytics,
  its own extraction) + the tx-detail archive fetch. Not this index.
- **Soroban EVENTS token-flow decode** (transfer/mint/burn over the 9.5 B-row
  `soroban_events`) is a co-equal concern (see [R-audit-inventory](notes/R-audit-inventory.md)).
  It contributes a stream to the composed asset read-query (Step 5) but its decode +
  its own backfill are large enough to track as their own effort if capacity forces
  it — flagged, not folded blindly.

## Acceptance criteria

- [ ] Design recorded in task (no separate ADR) + `docs/architecture/**` updated
      (schema + query docs)
- [ ] Shared `emit_participations` lib live; differential test green (byte-identical
      live vs backfill rows)
- [ ] `operation_asset_appearances` live, keys-only; native = positive surrogate
- [ ] Live ingest emits one row per participating asset per op (fan-out + companion
      indexes)
- [ ] `/assets/:id/transactions` native-inclusive, no early-return, unions SAC
      invocations; account/contract pages read their indexes
- [ ] Issued-asset lists include offers + both path-payment legs; native shows real
      activity
- [ ] One S3 re-parse backfill complete + validated (Soroban era), migrated
      partition-by-partition; old `operations_appearances` dropped
- [ ] Validation vs Horizon / stellar.expert for a sample incl. native (see
      [[reference_chq_clickhouse_cli]])
- [ ] humanizeOp path-payment mislabel fixed (Step 1)
- [ ] API types regenerated if shape changed

## Notes

- [R-audit-inventory](notes/R-audit-inventory.md) — root cause + full findings (F-A..F-F, K1–K4)
- [R-external-cross-validation](notes/R-external-cross-validation.md) — Horizon / Hubble / stellar.expert / indexers
- [R-prod-evidence-cross-validation](notes/R-prod-evidence-cross-validation.md) — prod-CH (`chq`) stats + 4 cross-validated examples, both links each
- [S-diagnosis-calibration](notes/S-diagnosis-calibration.md) — calibrated thesis, red-team corrections, second-slot code check
- [S-design-options](notes/S-design-options.md) — 6 modeling options → convergence on the role-tagged fan-out
- [S-field-comparison-fat-thin](notes/S-field-comparison-fat-thin.md) — field matrix + archive-on-demand refinement (feeds the deferred render decision)
- [S-devils-advocate](notes/S-devils-advocate.md) — 7 adversarial agents; 13 challenged claims; leg_index + diff-test gate
- [S-tx-render-audit](notes/S-tx-render-audit.md) — /ux-expert audit of the tx render; humanizeOp root cause; per-op-type spec (Step 1 + future render)
- [G-schema-and-roles](notes/G-schema-and-roles.md) — row shape, 25-op type→role mapping, content-addressed leg_index, PoolShare keying
- [G-spawn-plan](notes/G-spawn-plan.md) — **SUPERSEDED**: child-task split retracted; all work now lives in this task as Steps

## Open questions

- **Offers as "asset transactions"?** Product/Figma call — include (stellar.expert
  does) or keep as separate DEX activity. Default: include.
- **Inline list amounts?** The one deferred render decision that would touch storage
  (Core principle) — decide against the asset-page Figma before adding an `amount`
  column.
- Related: [[project_native_two_conventions]], [[project_contract_as_holder_gaps]],
  [[m2_enrichment_plan]], [[feedback_backfill_new_crate]], [[feedback_fundamental_complete_backward_data]].
