---
title: 'Spawn plan — task decomposition + coverage map (ready for develop)'
type: generation
status: developing
spawned_from: notes/S-devils-advocate.md
spawns: []
tags: ['spawn-plan', 'decomposition', 'coverage-map', 'ready']
links: []
history:
  - date: 2026-07-08
    status: developing
    who: karolkow
    note: >
      Spawn-readiness review of the whole 0359 hub. Every F-*/K-* finding mapped
      to a home (nothing lost); concrete child-task specs; spawn order. Spawn the
      children on develop (new-tasks-on-develop convention), each with
      related_tasks: [0359].
---

# Spawn plan — 0359 → children

Spawn each child on **develop** (per [[feedback_new_tasks_on_develop]]), each with
`related_tasks: ["0359"]`. Keep bundling sane — do NOT micro-split
([[feedback_task_scope]]).

## 0359 stays = the fan-out core (Layer-1)

New `operation_asset_appearances`, role-tagged, native surrogate, per-op-type role
emission via a shared library, `leg_index` content-addressed + differential test,
bounded op-type backfill (Soroban era), `/assets/:id/transactions` rewrite (native-
inclusive), LP asset rows (F-B), offers (F-E). Increments: **Phase 1** (offers) →
**Phase 2** (full fan-out). Phase 1/2 are increments of the SAME work — not
separate task files.

## Shared infrastructure (cuts across children — own/build FIRST)

The split is by-finding, but three substrates cut ACROSS the children. Treating
children as independent while they share these = integration pain + duplicated
cost (devil's-advocate 2026-07-08).

- **F0 · shared emission lib + one archive-re-parse harness** (FEATURE / M,
  foundational, spawn FIRST). One deterministic `emit_participations(op_details,
op_result)` lib (the `leg_index` gate) consumed by 0359 AND #2 AND #7 — built
  once, not re-implemented per table. One archive-re-parse harness that emits for
  ALL consumers (fan-out rows + soroban-events decode + participants) in a SINGLE
  sweep over the Soroban era, not N separate full re-parses. **0359, #2, #7
  depend on F0.**
- **Asset-page read-query ownership.** Phase 0 (SAC/F-F union), 0359 (fan-out),
  and #2 (soroban-events union) all rewrite `/assets/:id/transactions`. The final
  query = `fan-out ∪ SAC ∪ soroban-events`; **0359 owns the composition** +
  keyset pagination across streams; #1/#2 contribute streams to it. The three
  rewrites must COMPOSE, not overwrite.
- **Backfill coordination.** Every backfilling child re-parses the same ~13 M
  Soroban-era ledgers. Route them through F0's single harness or sequence them;
  never let 0359 + #2 + #3 each run an independent full archive pass.

## Children to spawn

| #   | Task                                 | Type / effort           | Scope                                                                                                                                                                                                           | Dep                                                        | Priority                            | Backing notes                                              |
| --- | ------------------------------------ | ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ----------------------------------- | ---------------------------------------------------------- |
| 1   | **Phase 0 — native read-side + F-F** | FEATURE / S-M           | drop the native early-return; wire `sac_contract_surrogate` (OR-branch) + union SAC invocations. No new table, no backfill                                                                                      | none — ship first                                          | **HIGH** (flagship symptom, cheap)  | S-field-comparison, S-diagnosis, R-prod-evidence           |
| 2   | **Layer-2 — soroban_events decode**  | FEATURE / XL epic       | decode transfer/mint/burn from/to/amount (dead `parse_transfer`); index participants incl. contracts; union into asset + account pages. 9.5 B rows, own backfill                                                | independent                                                | medium                              | R-audit-inventory (K1-3, K2-3/2-7, K3-4)                   |
| 3   | **Contract-as-holder/owner union**   | FEATURE / M             | F-D/K2-8 (contract-held classic/native orphan when SAC un-sighted) + K2-5 (NFT owner NULL). Read-side, data intact                                                                                              | none                                                       | medium-low                          | R-audit-inventory ws.3, S-diagnosis (K2-8 = residual tail) |
| 4   | **Fee-bump completeness**            | FEATURE / M             | index `inner_tx_hash` (K3-2, hard 404) + attribute fee_source/charged per account/asset (K2-4, ~45% of txs)                                                                                                     | none                                                       | medium (404 user-visible)           | R-audit-inventory ws.4                                     |
| 5   | **Search completeness**              | FEATURE / S-M           | asset by-name (K2-9) + SAC C-address resolve (K3-6)                                                                                                                                                             | none                                                       | low-medium                          | R-audit-inventory ws.5                                     |
| 6   | **FE — transaction render**          | FEATURE / M (+ BUG fix) | per-op-type human headline + progressive detail (replace misleading normal one-liner + raw advanced dump). **Quick-fix:** `humanizeOp` path-payment mislabel ("Sent 1 XLM"→"Swapped X→Y"), shippable standalone | independent of 0359 data (**except** claim-CB line = meta) | medium (mislabel = correctness bug) | S-tx-render-audit (+ per-op spec)                          |
| 7   | **Account participation roles**      | FEATURE / M             | F-C/K1-5 — crossed-offer counterparty, claimants, inflation-dest, revoke-target into `transaction_participants` (its own dedup + backfill)                                                                      | **F0**                                                     | medium                              | R-audit-inventory (K1-5), S-diagnosis                      |

**Caveats (devil's-advocate 2026-07-08):** Phase 0's `/assets/:id/transactions`
rewrite is **interim** — Phase 2 subsumes the native branch on the fan-out; only
the F-F/SAC union survives as a lasting stream. FE render (#6) is independent
EXCEPT the claim/clawback-CB headline line (needs meta extraction from
0359/Layer-2) — ship the rest first. Priority/effort here are **provisional** —
confirm against a real traffic/support signal before locking the order.

## Fold / small siblings (flagged, not forced)

- **Account participation roles** (F-C / K1-5) — moved OUT of "fold": it is now
  **its own sibling #7** (above). It writes `transaction_participants` — a
  DIFFERENT table with its own dedup + backfill; folding it into 0359 = a second
  write path, the scope creep we split the epic to avoid. It consumes F0's shared
  emission lib (the parse yields the counterparty), but the participants write +
  backfill are its own task.
- **NFT pending promotion** (K2-6) — small task, or the 0309/0340 area.
- **Aggregate/detail hygiene** (K4-\*) — small sibling: KPI-window alignment
  (K4-1), fold-vs-count (K4-2/3), nullable-aggregate 500 sweep (K4-5). Low.

## Coverage map — every finding has a home

| Finding                                         | Home                                     |
| ----------------------------------------------- | ---------------------------------------- |
| F-A, F-E, K1-1/1-2 (single slot, offers)        | **0359**                                 |
| F-B, K2-2 (LP native leg)                       | **0359** (LP rows)                       |
| K2-1 native (data)                              | **0359**; native read → **Phase 0 (#1)** |
| F-F, K3-1 (SAC union)                           | **Phase 0 (#1)**                         |
| F-C, K1-5 (account roles)                       | **#7** (own sibling; consumes F0 lib)    |
| F-D, K2-8, K2-5 (contract holder/owner)         | **#3**                                   |
| K1-3, K2-3/2-7, K3-3, K3-4 (soroban token flow) | **#2 Layer-2**                           |
| K3-2, K2-4 (fee-bump)                           | **#4**                                   |
| K2-9, K3-6 (search)                             | **#5**                                   |
| K2-6 (NFT pending)                              | small task / 0309-0340                   |
| K4-\* (hygiene)                                 | small sibling                            |
| render normal/advanced                          | **#6 FE**                                |

## Readiness checklist

**Decided (locked):** fan-out end-state · THIN · Road B (committed through the full
fan-out) · historical completeness bounded to the Soroban era · no separate ADR ·
native read-side (does not gate the schema) · bounded op-type backfill · complete
25-op type→role mapping · PoolShare keying · `leg_index` derivation · per-leg
amounts = a performance choice (inline-amount list), not completeness.

**Build-time open (NOT spawn blockers):** exact sizing on FINAL · the differential
test implementation (it IS the gate) · ZSTD ratio re-measure for any fat columns ·
whether the asset-page list shows inline amounts (→ whether to index `amount`).

## Spawn order

0. **Foundational FIRST — F0:** shared emission lib + single archive-re-parse
   harness + assign 0359 as the asset-page read-path owner. 0359/#2/#7 depend on it.
1. **Now (parallel with F0):** Phase 0 (#1, interim read-path) + the `humanizeOp`
   fix (from #6) — cheap wins, zero risk.
2. **Core:** 0359 Phase 1 (offers) → Phase 2 (full fan-out) on F0; gate = the
   `leg_index` differential test; 0359 owns the composed asset read-query.
3. **Parallel as capacity allows:** #3 contract-holder, #4 fee-bump, #5 search,
   #6 render (minus claim-CB line), #7 account roles (on F0).
4. **Separate:** #2 Layer-2 — its own epic, on F0's harness, contributes a stream
   to the composed asset query.

**Verdict (post devil's-advocate): spawn-ready WITH the shared-infra fixes.**
Coverage is complete (no orphans), but before spawning: build **F0** first (shared
emission lib + one archive-re-parse harness), name the asset read-query owner +
composition, and keep **F-C as its own sibling (#7)**, not folded. Phase 0 is
interim; FE ships minus the claim-CB line; priorities provisional.

## Devil's-advocate revisions (2026-07-08)

A /devils-advocate pass returned **ship with changes** — coverage complete, but the
by-finding split under-modeled shared substrates. Applied:

1. Added **F0** (shared emission lib + one archive-re-parse harness) as a
   foundational first task; 0359/#2/#7 depend on it (Concerns 1, 4).
2. Named the **asset read-query owner** (0359) + composition
   `fan-out ∪ SAC ∪ soroban-events` — the three rewrites must compose, not
   overwrite (Concern 2).
3. **Un-folded F-C** into its own sibling #7 — it writes a different table with
   its own backfill; folding = scope creep (Concern 3).
4. Marked **Phase 0 read-path interim** — subsumed by Phase 2 except the F-F union
   (Concern 5).
5. Marked **FE render's claim-CB line** meta-dependent, rest independent (Concern 6).
6. Flagged **priority/effort provisional** pending a real signal (Concern 7).
