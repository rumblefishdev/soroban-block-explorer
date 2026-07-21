---
id: '0392'
title: 'NFT pending: continuous live promote/reconcile (drain gap) + optional write-time tightening'
type: BUG
status: active
related_adr: []
related_tasks: ['0391', '0283', '0217', '0306', '0296']
tags: [priority-high, effort-medium, layer-indexer, layer-db, nft, clickhouse]
links: []
history:
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      **Settled by code, not by the single measurement — the defect is dormant, not
      gone.** The re-measurement above shows an empty quarantine, which on its own
      cannot distinguish "problem fixed" from "problem currently idle". The code
      distinguishes it: `persist/rows.rs:226-230` states that pending rows are
      promoted "via the post-backfill drain runbook — **CH has no per-row UPDATE /
      `WHERE NOT EXISTS` equivalent to PG's in-tx `promote_pending_nfts_to_hot`
      step**". Grepped it: `promote_pending_nfts_to_hot` **exists nowhere in the
      codebase** — it was a Postgres function, and Postgres was retired in 0244.
      So the live path has **zero** post-hoc promotion, and the only mechanism that
      moves a resolved contract out of quarantine is a human running
      `nft-reclassify`.
      That makes the gap arithmetic rather than speculation. `route_for` deliberately
      defers a contract whose WASM has not been observed yet — this task's own §4f
      measured that as 61% of pending, "correct defer", not a leak. Nothing
      un-defers them. Every contract whose WASM is observed after its first NFT
      event strands its rows permanently until someone drains by hand. Today's
      "0 resolved-but-stranded" means 0306's drain cleared the backlog 11 days ago
      and none of the 66 residents has resolved since — not that resolution now
      promotes.
      **Step 1 stands.** What is stale is the urgency framing (hot frozen 33 days,
      6,575 rows/day), not the defect.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      Scope pinned: **`nft-reclassify` is deleted either way**, but only after the
      replacement is verified working — not before. Two acceptable end states, and
      leaving the subcommand standing is neither: (1) Step 1's continuous reconcile
      lands and is observed promoting a real contract, then the subcommand goes; or
      (2) a cheaper monitor lands — alert when a pending contract has carried a
      resolved verdict for more than N ledgers — and the subcommand goes with the
      alert as the safety net. Deleting it before either exists would remove the
      only working drain. Keeping it after either exists re-creates the ownerless
      mop this task was spawned to end (lore 0425 clause 4).
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      **Re-measured — the premise no longer holds.** Six days after PR #341 landed,
      the numbers this task was built on have inverted. Then (2026-07-15): hot `nfts`
      frozen at ledger 62,989,407 since 2026-06-12 (33 days), ~6,575 pending rows/day
      at 91% fungible false-positive, 401 fungible-verdict and 21 Nft-verdict
      contracts stranded in quarantine. Now (2026-07-21, chain tip 63,583,789):
      hot `nfts` at **63,569,710** — it moved 580,303 ledgers and tracks the tip to
      within ~19h; `nfts_pending` holds **274 rows across 66 contracts**, last
      written 63,386,630.
      The decisive number is the verdict split of those 66 pending contracts:
      **0 with an `Nft` verdict, 0 with `Fungible`, 66 `Other`/NULL.** Not one
      resolved-but-stranded row — which is precisely what Step 1's continuous
      reconcile exists to drain. There is currently nothing to reconcile: the
      quarantine holds only genuinely-unclassifiable contracts, which is the design
      working as intended.
      What changed: PR #341 fixed the G9 prefetch (it was a 100% mechanical no-op),
      so verdicts now resolve at write time and contracts route straight to hot;
      0306's drain cleared the accumulated backlog. Step 2 is done, Step 3 is done.
      **Step 1 needs re-justification before anyone starts it** — either the drain
      gap reopens under some condition worth naming, or this task closes and the
      residual 66 unclassifiable contracts belong to 0317 (launchpad-NFT
      discriminator + monitored-UNKNOWN), not here.
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Spawned from 0391 §"Why *_pending grows unbounded" + R §4. Two sub-bugs,
      one shared root (write-time verdict resolution). Measured: hot frozen 33
      days, live path writes ~6,575 pending rows/day @ 91% fungible false-pos.
  - date: 2026-07-15
    status: backlog
    who: karolkow
    note: >
      Corrected after devil's-advocate crux test (0391 §4f). The "write-time
      fail-open leak" framing was overclaimed — of fungible pending rows with
      known WASM timing, 61% are correct defer (WASM seen at/after event), not a
      leak. Reordered: continuous reconcile is now the PRIMARY fix (Step 1);
      write-time tightening demoted to SECONDARY, gated on measuring the prefetch
      miss-rate. Reconcile gap + 33-day drain-staleness remain proven.
  - date: 2026-07-15
    status: active
    who: stkrolikiewicz
    note: >
      Promoted to active. Step 2 gate resolved by direct measurement same day:
      G9 prefetch was a 100% mechanical no-op (Nullable(Int16)-as-i16, ch0.15
      wire-type check; 20,494 failures/7d on prod) — see
      notes/R-g9-prefetch-miss-rate-measured.md. Fix + red/green e2e in PR #341
      (also unbreaks the 0320 prior-row prefetch, stale `name` column).
      Consistent with the §4f correction: fix stops only the H1 slice; Step 1
      reconcile remains primary. Steps 1 + 3 remain.
---

# NFT pending: continuous live promote/reconcile + write-time tightening

## Summary

The `nfts_pending` / `nft_ownership_pending` quarantine (built by **0217**) was
designed as **defer-then-promote**, but only the _defer_ half runs live — the
_promote/drop_ half exists exclusively as the one-shot backfill
`backfill-runner nft-reclassify`. As a result pending grows without bound and
NFT collection/detail pages lag reality. This task makes **reconcile continuous
on the live path** — promote/drop each contract's pending rows once its verdict
resolves — **without** mirroring the backfill's brute `ALTER … DELETE` sweep. A
write-time routing tightening is a _secondary, measurement-gated_ add-on, not the
primary fix (see Context — most fungible pending is correct defer, not a leak).

Measured on prod (2026-07-15, see [0391 R §4](../0391_RESEARCH_nft-token-flow-coverage-audit/notes/R-nft-coverage-measured-state.md)):
hot `nfts` frozen at ledger `62,989,407` (**2026-06-12**, last manual drain) for
33 days; live writes ~6,575 pending rows/day, **91% Fungible-verdict**; 401/401
fungible-verdict pending contracts confirmed real fungible assets; 21
`Nft`-verdict collections / 559 tokens stranded.

## Context

The mechanism has two parts. **Proven (0391 §4a–4e):** the promote/drop half of
the defer-then-promote design never runs live — only the one-shot backfill does
it — so pending accretes without bound and NFT pages lag by however long since
the last manual drain (33 days as of 2026-07-15).

**Unresolved (0391 §4f crux test):** a first pass blamed a write-time _fail-open
leak_ — `route_for` (`stage.rs:1444`) sends `Other|NULL|uncached→Pending`, and
the 0283 G1/G9 prefetch (`persist.rs:225,394`) is best-effort and falls through
to Pending on miss. But of the fungible-verdict pending rows with **known**
WASM-observation timing, the **majority (61%) were correct defer** (WASM observed
at/after the event → unclassifiable at ingest → _legitimately_ pending), and 72%
have no recorded upload ledger at all. Write-time fail-open (H1) is therefore a
minority (~11% overall, ≤39% of timing-known) and **unproven** as the dominant
cause. Implication: continuous reconcile is the reliable fix; a write-time change
cannot prevent the H2 defer rows, and would only help H1 — which must be
justified by measuring the prefetch miss-rate first. Either way, do NOT mirror
the backfill `ALTER … DELETE` on the live path (treats the symptom, races the
ingest inserts).

`Fungible`/NFT `transfer` events are byte-identical in shape (`from,to,i128` vs
`from,to,token_id`) — the parser cannot distinguish them; only WASM
classification can. So a genuinely-never-seen contract MUST still be able to
quarantine. This task does not try to make pending zero — it makes pending hold
_only_ genuinely-unresolved contracts, and reconciles them once resolved.

## Implementation Plan

### Step 1 (PRIMARY): Continuous reconcile — event-driven, per newly-classified contract

- When a contract's verdict first resolves to `Nft`/`Fungible`/`Token` (i.e. its
  WASM becomes observed / `contract-type-rebuild`-equivalent runs), promote
  (`Nft` pending→hot) or drop (`Fungible|Token`) **that one contract's** pending
  rows.
- Scope to the contract, not a full-table sweep. This is the live equivalent of
  the `nft-reclassify` promote/drop, triggered by classification, not by cron.
- Decide the trigger point: at deploy/upgrade when WASM is classified, vs a
  lightweight scheduled reconcile keyed on `soroban_contracts` verdict changes
  since last run.
- This is the reliable fix: it handles the H2 majority (correct-defer rows that
  no write-time change can catch) as well as the H1 slice.

### Step 2 (SECONDARY, gated): Write-time tightening — gate RESOLVED, fix shipped

- **Gate resolved by direct measurement (2026-07-15,
  [R-g9-prefetch-miss-rate-measured](notes/R-g9-prefetch-miss-rate-measured.md)):**
  the G9 prefetch miss-rate was **100% mechanical** — the fetch itself failed on
  every row-returning call (`contract_type` read as bare `i16` vs
  `Nullable(Int16)`, rejected by clickhouse 0.15 RBWNAT validation; 20,494 prod
  failures/7d, single error string, since indexer resume 2026-06-29). G9 never
  delivered a verdict; the `ClassificationCache` never held anything.
- **Fix shipped in PR #341:** `Option<i16>` (one line) — the existing
  cache-backed prefetch design was already correct and now actually runs, so no
  new per-event query was added (cost guard satisfied by construction). Same PR
  unbreaks the 0320 prior-row prefetch (SELECTed the 0304-dropped `name` column
  → Code 47) and adds a `CLICKHOUSE_URL`-gated e2e asserting Fungible→Drop /
  Nft→Hot / unknown→Pending + the upgrade-row write (red/green verified).
- **Consistency with §4f:** no contradiction — with G9 dead, ALL cross-ledger
  rows fell to Pending regardless of WASM timing. The fix stops only the H1
  slice (verdict knowable at event time, ≤39% of timing-known + some share of
  the 72% NULL); the H2 correct-defer majority still quarantines by design and
  is exactly what Step 1's reconcile drains. Post-deploy, re-run the R §4c
  split to measure the residual intake.

### Step 3: One-shot cleanup of the accumulated backlog

- The ~280k existing fungible false-positives + stranded `Nft` rows still need a
  single drain to clear the 33-day backlog. That is **0306**'s prod
  reclassify run — coordinate, don't duplicate. This task ensures the backlog
  does not re-accumulate after 0306 drains it.

## Acceptance Criteria

- [ ] (Step 1, primary) Newly `Nft`-classified contracts' pending rows promote to
      hot, and `Fungible|Token` pending rows drop, without a manual
      `nft-reclassify` run — verified: hot `nfts` max ledger tracks the chain tip
      instead of freezing (re-run R §4a).
- [ ] (Step 1) Genuinely-unresolved contracts (WASM never observed) still
      quarantine correctly — pending is not forced to zero.
- [ ] **`nft-reclassify` is deleted — either way, but only after the replacement
      is verified working.** Together with its row in `docs/backfills.md` and its
      entry in `crates/backfill-runner/README.md`. "Either way" means: whether the
      replacement is Step 1's continuous reconcile or the cheaper
      resolved-verdict-stuck-in-pending alert, the subcommand goes once that
      replacement has been _observed_ doing its job on a real contract — not on the
      strength of an empty quarantine. Deleting it earlier removes the only working
      drain; keeping it later re-creates the ownerless mop this task exists to end.
      Per lore 0425 clause 4.
- [x] (Step 2 gate) 0283 prefetch miss-rate measured directly — 100% mechanical
      failure (wire-type bug), fix shipped in PR #341
      (notes/R-g9-prefetch-miss-rate-measured.md).
- [ ] (Step 2, shipped) hot-path latency not regressed (no new query added —
      satisfied by construction); daily fungible-verdict pending intake drop
      measured post-deploy (re-run the R §4c split).
- [ ] **Docs updated** — `docs/architecture/**` ingestion-pipeline + XDR-parsing
      sections describe the routing + reconcile (per ADR 0032). Update in PR.
- [ ] **API types regenerated** — N/A unless the fix touches `crates/api/**`
      (routing/ingest is `crates/db-clickhouse` + `crates/indexer`).

## Notes

- Depends conceptually on 0283 (verdict prefetch) — this sharpens its fail-open.
- 0217 (archived) built the quarantine; 0306 is the one-shot prod drain; this
  task is the _live_ continuous half neither covers.
- Do NOT implement as a live mirror of `nft_reclassify`'s `ALTER … DELETE` — that
  treats the symptom and races the ingest inserts.
