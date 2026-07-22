---
id: '0392'
title: 'NFT pending: continuous live promote/reconcile (drain gap) + optional write-time tightening'
type: BUG
status: active
related_adr: ['0053']
related_tasks: ['0391', '0283', '0217', '0306', '0296']
tags: [priority-high, effort-medium, layer-indexer, layer-db, nft, clickhouse]
links: []
history:
  - date: 2026-07-22
    status: active
    who: karolkow
    note: >
      **Scope settled after a fourth reframing — and the honest headline is that
      the acute problem was already fixed before this session started.** PR #341
      (step 2 of this task) repaired the verdict prefetch; the quarantine has
      taken nothing since 2026-07-16 and its newest row is 12 days old. What
      remained was a dead mechanism plus its manual shovel.
      **What ships:** membership decided before the write — only a `Nft` verdict
      produces rows, `Fungible`/`Token` drop as always, and a contract with no
      decisive verdict is dropped WITH a per-contract log line
      (`0392 unclassified NFT-shaped emitter`). The verdict lookup now fails
      CLOSED, because routing that discards the unclassifiable must not treat a
      ClickHouse hiccup as "unclassifiable". Verdict source unchanged
      (`soroban_contracts.contract_type`) — Karol's call: the column is meant to
      mirror the WASM, so the answer to staleness is to refresh it, not route
      around it.
      **What deliberately does NOT ship:** the quarantine tables are NOT dropped.
      They hold 274 + 492 rows from 66 contracts, and ~28 of those are real NFT
      collections by their own interfaces. Dropping them now would delete 181 +
      183 rows of genuine NFT data against a recovery job that does not exist.
      The `DROP` moves to the classifier follow-up (0309). Also not shipping: the
      signature-based classifier rule (validated on 127,221 contracts, 0
      contradictions, 48→Nft / 82→Fungible) — 0415 showed the classifier is
      refuted at the head of the traffic distribution, so a partial patch would
      bury that. Measurements carried to 0415.
      **Two earlier designs were built, measured and rejected in review:**
      read-time visibility filter (rejected: a row in `nfts` must BE an NFT) and
      live WASM classification on the event path (rejected: one source of truth).
      Recorded in [ADR 0053](../../../2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md).
      **Ranked findings this session produced, three of which are bigger than
      this task and belong to 0415:** (1) NFT ownership disagrees with the chain
      — 4/4 sampled tokens of `CAVK536D…`; (2) parser blind to non-standard event
      names — 19 collections, 622 events; (3) classifier refuted at the head of
      traffic — 4,211 rows typed `Fungible`; (4) this quarantine, dormant 12 days.
  - date: 2026-07-22
    status: active
    who: karolkow
    note: >
      **Design changed, and the change is the fix.** Step 1 was specified as a
      continuous promote/drop — the live equivalent of `nft-reclassify`. Building
      it revealed the specification was aimed at the symptom: on the live path a
      contract's verdict is written only at deploy (`stage.rs:776`) and a WASM
      upgrade deliberately carries the old verdict forward (`stage.rs:255`), so
      for the population that is actually stuck (66 contracts, all `Other`) that
      trigger would fire approximately never.
      Root cause restated: a **mutable judgement was encoded in immutable
      physical storage** — which of two tables a row lives in. ClickHouse has no
      per-row UPDATE, so that guarantees rows must physically move whenever the
      judgement changes, and something must own the moving. Every variant that
      keeps the split (event-driven promote, scheduled sweep) keeps that owner.
      New design: ONE table, visibility as a read-time predicate on the
      contract's current verdict. Quarantine tables, `nft-reclassify`, and the
      promote/drop concept are deleted. A verdict resolving to `Nft` — live or
      via `contract-type-rebuild` — surfaces that contract's rows on the next
      read, with nothing to promote and nothing to schedule. Recorded as
      [ADR 0053](../../../2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md),
      superseding ADR 0046.
      Shipped this session: read-side filter + build-failing guard test, write-side
      collapse to two buckets, both tables out of `init.sql`, `nft-reclassify`
      deleted, docs + runbooks retired. −710 / +257 lines across 15 files. Full
      workspace green; the routing e2e re-run against a real CH 26.3 in docker.
      NOT yet done: the prod sequence (deploy API → merge rows → deploy indexer →
      re-merge → DROP TABLE), which needs per-step approval.
  - date: 2026-07-21
    status: active
    who: karolkow
    note: >
      **Taken over. Steps 2 and 3 re-verified as genuinely done; Step 1 confirmed
      necessary and is now the whole remaining task.**
      **Step 2 — done and live.** The `Option<i16>` fix (PR #341, `9cb3834e`) is on
      `origin/develop`, not on `origin/master` — but `master` last moved 2026-07-03
      and `develop` is **448 commits ahead**, so `master` is not the deploy source.
      Effectiveness proven from data instead of from branches: the last pending
      drain ran **2026-07-16 15:58:58** (`system.mutations`, `DELETE WHERE
      contract_id IN (… contract_type IN (0,2,3))`), and in the five days since,
      `nfts_pending` has received **nothing** — 274 rows total, newest at ledger
      63,386,630. With G9 still broken it would have taken ~6,575 rows/day, i.e.
      ~33,000 rows. So verdicts do resolve at write time now.
      **Step 3 — done and clean.** Of the distinct contracts in hot `nfts`, **all 66
      carry verdict `Nft` (2)** — zero `Fungible`/`Token` contamination survived the
      drain. The quarantine holds 66 contracts, all `Other`/NULL, which is a
      correctly-behaving quarantine.
      **Step 1 — still absent, and now proven so by code rather than inferred.**
      `persist/rows.rs:226-230` says promotion happens "via the post-backfill drain
      runbook — CH has no per-row UPDATE / `WHERE NOT EXISTS` equivalent to PG's
      in-tx `promote_pending_nfts_to_hot` step", and that function exists nowhere in
      the codebase (Postgres, retired in 0244). Nothing moves a contract out of
      quarantine once its verdict resolves except a human.
      Unrelated finding worth carrying out of this check: the API code reading
      `operation_asset_appearances.net_settled` is on `develop` but the column does
      not exist on prod. It is **not currently erroring** — 72h of `system.query_log`
      shows zero occurrences beyond my own probe — because the API has not been
      redeployed since that code landed. 0419 owns the `ALTER`; deploying the API
      before it runs gives `Code 47` on that endpoint.
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
designed as **defer-then-promote**, but only the _defer_ half ever ran live — the
_promote/drop_ half was specified against Postgres, which was retired in 0244, and
never reimplemented on ClickHouse. Its only remaining drain was a human running
`backfill-runner nft-reclassify`, so the NFT surface went 33 days stale.

**Resolution (2026-07-22, [ADR 0053](../../../2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md)):
the split itself is the defect.** Which table a row lives in encoded a _mutable_
judgement in _immutable_ storage, and ClickHouse has no per-row UPDATE — so
something always had to move rows, and that something is what kept failing. Both
tables are gone. All NFT-shaped rows not proven fungible go to `nfts` /
`nft_ownership`; visibility is a read-time predicate on the contract's current
verdict. A contract classified later surfaces its rows on the next read, with
nothing to promote and no job to schedule.

The sections below keep the original framing where it still explains _why_
unclassified rows must not reach the API; the superseded parts are marked.

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

### Step 1 (PRIMARY): Decide membership before the write

**Superseded specification.** This step originally read "continuous reconcile —
event-driven, per newly-classified contract". Two further reframings followed
(read-time filter, then live WASM classification), both built, measured and
rejected in review. See the 2026-07-22 history entry and
[ADR 0053](../../../2-adrs/0053_nft-membership-decided-at-write-time-from-wasm.md).

- `route_for` keeps a row only when the contract's verdict is `Nft`.
  `Fungible`/`Token` drop as they always did.
- No decisive verdict → **drop + log** (`0392 unclassified NFT-shaped emitter`,
  one line per contract per ledger). Dropping is the only outcome that can lose a
  real collection, so it is the one that is never silent.
- The verdict lookup **fails closed**: a ClickHouse error aborts the ledger for
  retry instead of being read as "unclassifiable".
- Verdict source unchanged — `soroban_contracts.contract_type`. A classifier
  change therefore needs one `contract-type-rebuild` pass to take effect
  (measured: ~73 contracts carry a stale `Other` despite a decisive WASM).
- Both `INSERT` streams, both row structs and `nft-reclassify` are deleted.
  **The tables are NOT dropped** — see the history entry.

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

- [x] (Step 1) No resolved verdict can leave rows stranded. Met by construction:
      membership is decided before the write, so there is no promotion step to
      skip. Asserted in `crates/db-clickhouse/tests/g9_verdict_routing_e2e.rs`
      against a real ClickHouse (`Fungible` → dropped, `Nft` → written,
      no verdict → dropped), verified red before green.
- [x] (Step 1) Nothing is discarded silently — every unclassifiable emitter is
      logged per contract, which doubles as the classifier's work queue.
- [x] (Step 1) No live `ALTER … DELETE` mirror of the backfill sweep — there is
      no live mutation at all.
- [x] **`nft-reclassify` deleted** with its `docs/backfills.md` row and its
      `crates/backfill-runner/README.md` entry. The operation it performed no
      longer exists. Per lore 0425 clause 4.
- [x] (Step 2 gate) 0283 prefetch miss-rate measured directly — 100% mechanical
      failure, fix shipped in PR #341 (notes/R-g9-prefetch-miss-rate-measured.md).
- [x] (Step 2) Hot-path latency not regressed — no new query on the write path;
      the read path is untouched (the API diff is one added test).
- [x] **Docs updated** — ADR 0053 (+ 0046 superseded), schema overview §4.13.1,
      `clickhouse-pilot.md` §4c-bis, indexing-pipeline, xdr-parsing,
      `docs/backfills.md`, backfill-runner README, retirement banners on runbooks
      0118 / 0217 / 0221 / 0294 (+ partial on 0228 and the 2of5 runbook).
      Per ADR 0032.
- [x] **API types regenerated** — N/A. `crates/api/**` gains only a test; no DTO,
      route or schema change. Verified: `npx nx run @rumblefish/api-types:generate`
      produces an empty diff.
- [ ] **Prod:** deploy the indexer, then watch `0392 unclassified NFT-shaped
  emitter`. No `DROP TABLE`, no migration, no point of no return.
- [ ] **Deferred to 0309:** reclaim the retained quarantine rows for contracts a
      better classifier can name, then drop both tables.

## Implementation Notes

Code (−710 / +257 across 15 files):

| Layer                                        | Change                                                                                                                                      |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `api/src/nfts/queries.rs`                    | `NFT_VISIBLE` + the predicate in 4 queries; `search/queries.rs` in 1                                                                        |
| `api/tests/nft_visibility_guard.rs`          | new — scans `api/src/**` and fails on a `FROM nfts` / `FROM nft_ownership` without the predicate; second test pins `ContractType::Nft == 2` |
| `db-clickhouse/persist/stage.rs`             | `NftRoute` 3 buckets → 2 (`Keep`/`Drop`); one dedup map instead of two                                                                      |
| `db-clickhouse/persist/rows.rs`, `writer.rs` | pending row structs + their two `INSERT` streams removed (18 streaming tables → 16)                                                         |
| `db-clickhouse/schema/init.sql`              | both tables dropped; object count 31 → 29                                                                                                   |
| `backfill-runner`                            | `nft_reclassify.rs` → `.trash/`, subcommand unwired, `repair_tier1` down from 12 columns × 5 tables to 9 × 4                                |

Verification: full workspace suite green (38 test binaries, 0 failures); clippy
and fmt clean; `g9_verdict_routing_e2e` re-run against ClickHouse 26.3 in docker
with the new `init.sql`, asserting `Fungible` → dropped, `Nft` → written,
unknown → written **and** filtered out by the API's own predicate.

## Design Decisions

### From Plan

1. **Do not mirror `nft_reclassify`'s `ALTER … DELETE` on the live path.** Held
   — there is no live mutation at all now.
2. **A never-seen contract must still be able to quarantine.** Held — its rows
   are written and invisible, which is the same observable behaviour.

### Emerged

3. **Replaced the split instead of automating the promotion.** Not what the task
   specified. Justified in ADR 0053; the deciding measurement is that the live
   verdict-resolution trigger would never fire for the stuck population.
4. **Guard test in the repo rather than a ClickHouse view.** A view would have to
   shadow the `nfts` name to protect unchanged code, which renames a live table,
   hides the predicate from readers, and freezes `FINAL` for every caller (worth
   up to 19× read amplification, task 0420). Rejected in review by the user.
5. **Kept `FINAL` in the predicate at +14 ms.** Without it, visibility means
   "some version read as `Nft`" — correct only while nothing downgrades a
   verdict. True today, not worth making load-bearing.
6. **Rewrote three routing tests instead of deleting them.** The contract they
   assert changed (`..._to_pending_bucket` → `..._keeps_..._row`); they still
   pin the same property — an unresolved contract's data is not discarded.
7. **Runbooks retired with a banner, not deleted.** They record operations that
   actually ran on prod; left as-is they would be a trap, deleted they would lose
   the history.

## Future Work

- Prod execution sequence (above) — not code, needs per-step approval.
- 19 contracts carry an `Nft` verdict and emit `mint`/`transfer` yet have no rows
  in any table — the parser never produced them. Evidence added to **0317**.
- A WASM upgrade that changes a contract's class is still silently ignored
  (`stage.rs:255` carries the verdict forward). Evidence added to **0325**.

## Notes

- Depends conceptually on 0283 (verdict prefetch) — this sharpens its fail-open.
- 0217 (archived) built the quarantine; 0306 is the one-shot prod drain; this
  task is the _live_ continuous half neither covers.
- Do NOT implement as a live mirror of `nft_reclassify`'s `ALTER … DELETE` — that
  treats the symptom and races the ingest inserts.
