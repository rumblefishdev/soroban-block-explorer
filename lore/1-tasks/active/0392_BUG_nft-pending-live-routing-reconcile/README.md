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
  - date: 2026-08-21
    status: active
    who: karolkow
    note: >
      **Re-verified a month on — every checkable claim in this task still holds;
      only the scale shrank.**
      Code, on `origin/develop` (`eb7e817c`): the promote half is still absent.
      `promote_or_count` exists solely inside
      `crates/backfill-runner/src/nft_reclassify.rs`, and that file is still
      present — the only drain is still a human. `route_for` is unchanged in
      behaviour (`persist/stage.rs:1648`; the body cites 1444, the line drifted,
      the semantics did not): `Token|Fungible -> Drop`, `Nft -> Hot`, everything
      else -> `Pending`.
      Prod, measured 2026-08-21 (chain tip 64,054,678): `nfts_pending` holds
      **278 rows across 67 contracts** (274 / 66 a month ago). A whole month's
      intake is **+4 rows, +1 contract** — against ~6,575 rows/day before PR #341.
      The last pending write was ledger 63,836,382, **~12.6 days ago**. Hot `nfts`
      is at 64,054,630, **48 ledgers off the tip (~4 min)**, holding 13,326 rows
      over those same 67 contracts. The frozen-surface failure is gone.
      The decisive split is unchanged from last month: across every row of
      `soroban_contracts` for those 67 contracts, duplicates included, the only
      `contract_type` ever stamped is `1` — `groupUniqArray` returns `[1]`.
      **0 carry a decisive verdict, 0 resolved-but-stranded.** Enum, for the
      record, since the ordering is not the obvious one:
      `Token = 0, Other = 1, Nft = 2, Fungible = 3`.
      So the defect is real and unfixed, but it is now a ~4-rows-a-month leak
      with an empty resolved queue — a different problem from the one this task
      was written for. The urgency numbers still quoted in the body (33 days
      frozen, 6,575 rows/day) remain stale. Restarted from scratch on
      `fix/0392_nft-pending-drain-gap`.
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

## Restart measurements — 2026-08-21

Re-derived from scratch against prod ClickHouse and, where it matters, against
the chain via the official `stellar` CLI 26.0.0. Nothing here is carried over
from earlier sessions; figures that disagree with the body above supersede it.

### F1 — the quarantine holds nobody this task can drain

Of the 67 contracts in `nfts_pending`, split by when their WASM was uploaded
relative to their first pending row:

| Bucket                                                      | Contracts | Rows |
| ----------------------------------------------------------- | --------- | ---- |
| A — WASM never observed                                     | **0**     | 0    |
| B — WASM known _before_ the first event                     | **66**    | 274  |
| C — WASM arrived _after_ the first event (late, strandable) | **1**     | 4    |

Bucket B is the whole quarantine. For those 66 the verdict _was_ available at
write time — `route_for` asked, and the answer was `Other`. They are not
deferred-for-timing; they are classifier misses. A reconcile cannot move them:
their verdict is already computed and it is not decisive.

Bucket C — one contract, 4 rows — is the entire population Step 1 was designed
for. Even it would not promote, because its verdict is also `Other`.

**So Step 1, implemented exactly as written, would move zero rows today.**

### F2 — an acceptance criterion describes an empty set

The AC _"genuinely-unresolved contracts (WASM never observed) still quarantine
correctly"_ has no members: bucket A is 0. The stated design intent of the
quarantine — hold contracts whose WASM has not been seen — is not what the
quarantine actually contains.

### F3 — the larger defect is next door, and it is silent

Contracts carrying a decisive `Nft` verdict, measured against every NFT table:

|                                       | Contracts | Events  |
| ------------------------------------- | --------- | ------- |
| have rows in `nfts` + `nft_ownership` | 67        | 25,110  |
| **have rows nowhere at all**          | **66**    | **692** |

Those 66 are not quarantined. They are absent — no hot row, no ownership row,
no pending row. The quarantine sits at the _classifier_ gate only; nothing
guards the _parser_ gate, so a known-NFT contract whose events the parser cannot
shape simply produces nothing, with no record that anything was dropped.

### F4 — cause of F3, confirmed on-chain not from our tables

`stellar contract info interface --network mainnet` on two of the 66, reading
the deployed WASM:

- `CB2SIYGHFGQMKEYQUWCTF3HCWBCPFUSRGVWXOPV3LIJR7K5LRPFXZEYK` —
  `transfer(env, domain: String, from: Address, to: Address)` and
  `owner_of(domain: String)`. Token identity is a **String**, and it is the
  **first** argument. Canonical is `(from, to, token_id)`. The shape cannot
  match, so every transfer is dropped. This contract emits a perfectly
  canonical `transfer` event name — name matching alone would not have saved it.
- `CBT5JMDOUAU3BJF7YZR42LVODLMZSQE4LIJUJNUBKEC2VZOXIF4JFBRU` —
  `owner_of(token_id: u32)`, `transfer`, `mint_badge`, `total_supply`.
  Unambiguously an NFT; mints via `mint_badge`, emits `init` / `minted`.

Event signatures across the 66, by volume: `(null)` 470 (5 contracts),
`minted` 63, `uri_upd` 58, `mint` 46, plus `identity_minted`, `mint_event`,
`transfer_event`, `set_uri_event`, `approval_for_transfer_event`. The 470
undecoded signatures are the single largest slice and are not yet explained.

### F5 — current intake

Two contracts wrote pending rows in the last month:
`CAHDANGTQY4TOXV7LYYXRTFIUKJKJUXWNRUMKRPBJG2QOGSATYTRX6HP` (5 rows, ledgers
63,636,969–63,836,382) and
`CBGMSY35IZMHNVBFQQY22PA62VWJVXIKC4TU2CTAKRGIJOACZE4EEIWW` (2 rows). The net
count moved 274 → 278; the prior 274/66 figure comes from an earlier session and
was not re-verified, so treat the delta as approximate. What is solid: two
contracts, single-digit rows, one month.

### F6 — measurement trap for whoever works this next

`soroban_contracts` is RMT and unmerged. `CAHDANG…` carries two disagreeing
rows — a pre-WASM placeholder (`contract_type` NULL, `wasm_uploaded_at_ledger` 0) and a later stamp (`63,207,509`, `contract_type` 1). Any join to
`soroban_contracts` without deduping fans out and double-counts; it inflated one
of my own intermediate counts 5 → 10 before I caught it. Dedup with
`argMax(contract_type, wasm_uploaded_at_ledger)` grouped by `id`.

### What this implies for the restart

The drain gap is real as a mechanism and unfixed in code, but its addressable
population is one contract and four rows, none of which would move. The defect
that is actually costing data is the parser's ABI coverage (F3/F4), which is
0309 / 0317 territory, not this task's. That is the input to the design
decision, not the decision itself.
