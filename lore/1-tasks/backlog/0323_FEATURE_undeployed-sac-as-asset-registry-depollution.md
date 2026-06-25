---
id: '0323'
title: 'FEATURE: model un-deployed SACs as assets, not soroban_contracts rows (registry depollution / T3)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0294', '0218', '0283']
tags:
  [
    clickhouse,
    sac,
    contract-classification,
    layer-data,
    api,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Spawned from 0294. The fundamental fix for the orphan/skeleton pollution of
      /v1/contracts ("T3"): an un-deployed SAC is an ASSET, not a contract, so it
      must not get a soroban_contracts row. Mainnet-verified (Soroban RPC) that
      the orphans genuinely have no on-ledger contract instance.
---

# FEATURE: model un-deployed SACs as assets, not soroban_contracts rows

## Problem

A classic asset's SAC has a deterministic `C…` address computed by SHA256 from the
asset — reserved for _every_ asset, deployed only if someone explicitly runs
`CreateContract`. Post-P23 (CAP-67 unified events) a classic payment emits a Soroban
event **under that reserved SAC address even when no instance was deployed**. Our
indexer's rule "a `contract_id` seen in an event ⇒ a contract exists" then writes a
`soroban_contracts` row for it. Result: `/v1/contracts` is polluted by **~311k**
un-deployed-SAC rows that are not contracts:

- **~307k `is_sac=true` asset-override skeletons** (`stage.rs:548-579`) — 72,345 with
  `deployed_at_ledger IS NULL` **+ 234,902 with the `=0` sentinel**. (A NULL-only count
  sees just the 72k; the bulk is the `=0` sentinel subset — same undercount the 0294
  README flagged.)
- **~4.3k `is_sac=false` orphan stubs** (Pass-2 FK stub, `stage.rs:~1390`); 5,558 by the
  event-emitting predicate.

The two buckets are the SAME entity (an un-deployed SAC) split only by which ingest path
touched it first: seen-as-asset (trustline) → crypto-derived → `is_sac=true` skeleton;
seen-only-as-event-emitter → generic FK stub → `is_sac=false`. Real deployed SACs
(3,906, `deployed_at_ledger>0`) and real WASM contracts (107,460) are NOT in scope —
they keep their rows.

**Mainnet ground truth (2026-06-23, Soroban RPC `getLedgerEntries` + invoke, positive
control USDC):** the orphan SACs have NO ContractData instance ("no matching contract
data entries were found" — not even archived). They are genuinely never deployed.
Deployed SACs (USDC) DO have an instance and we capture their deploy correctly
(`deployed_at_ledger` set). So this is a **data-model error, not a missed deploy**.

The false-NFT half of the original 0294 bug is already fixed (the detection-stage SAC
gate in `detect_nft_events`). This task is the registry-model half.

## Correct model

`soroban_contracts` = **deployed contract instances only**. An un-deployed SAC is an
**asset** (already representable in the `assets` table; its `C…` strkey is re-derivable
from the asset at any time via SHA256). Nothing is lost by not storing it as a contract
row — the fact is fully derived.

## Design

**1. Writer — stop writing un-deployed-SAC rows** (`crates/db-clickhouse/src/persist/stage.rs`):

- **Pass-2 FK stub (~1390):** `referenced` is filled from operations, **events**,
  invocations, assets, nfts. Skip a `cid` that is a crypto-proven SAC emitter THIS
  ledger — build that set in-stage from the ledger's events via the shared
  `sac_override_from_event_topics` gate (the same gate as the 0294 detection fix).
  **Pure, no DB lookup** (the event carries the asset; `derive_sac(asset)==emitter`).
  A _deployed_ SAC (USDC) emitting here is also skipped, harmlessly — it already has its
  real deploy row from its deploy ledger (RMT version > 0).
- **Asset-override skeleton (~557):** remove. Under a genesis-complete index a deployed
  SAC always has its real deploy row (site 516); an un-deployed one must not have a row.
  The skeleton's old routing job is now done by the detection gate. (Re-validate nothing
  depends on the is_sac=true skeleton before deleting.)
- Real deploys (site 516) unchanged. `is_sac` column unchanged (deployed SACs keep it).

**2. API joins — 3 INNER → LEFT** (the only real risk surface; each is a _latent bug_
today — a real event/invocation/NFT referencing a no-row contract silently vanishes):

- `crates/api/src/transactions/queries_ch.rs:821` (tx events) → LEFT
- `crates/api/src/transactions/queries_ch.rs:851` (tx invocations) → LEFT
- `crates/api/src/nfts/queries_ch.rs:277` (NFT detail) → LEFT
- (All other CH reads are already LEFT / subquery / `FROM`-registry — verified.)

**3. Assets** — ensure the un-deployed SAC's asset is present in `assets` (today: 0 of
the orphans are in `assets`) so its activity has a home.

**4. strkey display — decision: option (a)** (accept graceful degradation). The surrogate
`id` is a one-way hash (`ids::contract_id = hash64(strkey)`), so the `C…` strkey lives
ONLY in the soroban_contracts row. With the row gone, the 3 LEFT-joined references show
a blank contract field. Accepted as a minor, correct degradation (these are asset
activity, not contract activity). **Future (option c):** resolve those references to the
ASSET (surrogate→`assets`→`CODE:ISSUER`) so the UI shows "UNI transfer" instead of a
blank contract — out of scope here.

## Implementation notes (wiring — verified 2026-06-24)

- **Pass-2 skip needs `net_id`.** `prepare_with_sac_overrides` already receives the ledger
  `events` (`StageInputs.events` ✓) but NOT a network passphrase / `net_id`. Thread it in
  (`process.rs` already derives `net_id` for `detect_nft_events` — pass the same). **Cleaner
  alternative:** build the crypto-proven-SAC emitter set at DETECTION (`process.rs`, where
  topics-JSON + `net_id` already exist) and pass that `HashSet<strkey>` into `StageInputs`,
  instead of re-deriving from `ExtractedEvent` inside stage.
- **Skeleton removal = delete the `for ov in sac_overrides` push at `stage.rs:553-578`**
  (the asset-path override). See "Redundancy" below — `sac_overrides` likely becomes fully
  dead afterwards (its only other use is the Pass-2 suppression at 1340, replaced by the gate;
  SAC assets come from `contract_deployments`, not the override).
- **Two skeleton encodings.** `is_sac=true` un-deployed rows exist as BOTH
  `deployed_at_ledger IS NULL` (~72k, current override writes `None`) AND `=0` (~235k, an
  older path). Cleanup MUST use `coalesce(deployed_at_ledger,0)=0`; confirm no live path
  still emits the `=0` variant (else they regrow).

## Redundancy created by T3 — must also clean up (else stranded)

1. **~87M false `nfts_pending` rows** (the i128 amounts mis-read as token_ids for these
   un-deployed SACs — the bulk of pending). The removed `sac-orphan-relabel` batch USED to
   drain them (flip → `nft-reclassify` drop). With the batch gone AND these contract rows
   deleted, **nothing drains them → permanently stranded junk.** T3 MUST drop
   `nfts_pending` (+ `nft_ownership_pending`) for the crypto-proven-SAC contracts in the
   same cleanup pass as AC#6. The live gate only stops NEW ones. **New AC.**
2. **`sac_overrides` plumbing likely goes fully dead.** Only consumers: the skeleton write
   (`stage.rs:553-578`, removed) and the Pass-2 suppression (`stage.rs:1340`, replaced by
   the gate). SAC `assets` rows come from `contract_deployments` (`stage.rs:968`), NOT from
   `sac_overrides`. So after T3 either (a) repurpose the override `identity` to satisfy
   AC#3 (put un-deployed-SAC assets into `assets`), OR (b) if AC#3 sources the asset from
   the event topic, DELETE the whole chain: `derive_sac_overrides_from_assets` (process.rs),
   `ParseOutput.sac_overrides`, `StageInputs.sac_overrides`, and collapse
   `prepare` / `prepare_with_sac_overrides` back to one entrypoint.
3. **`stage.rs:1340` + comment 1333-1339 go stale** ("override exists → suppress stub"
   rationale is gone) — replace with the crypto-proven-SAC skip (= the Pass-2 gate above).

## Blockers checked

- **"need a DB lookup to know if deployed"** — NOT for SACs: the event self-identifies
  (crypto-match in-stage). No blocker.
- **strkey resolution** — the one real wrinkle; resolved by option (a) above.
- No other blockers found (join audit complete; only 3 INNER joins, all latent bugs).

## Supersedes / validated (2026-06-24)

- **Supersedes the 0294 relabel batch.** The `sac-orphan-relabel` CLI (flip orphans to
  `is_sac=true`) was removed from PR #272 — it was the old "mark the SAC" model, redundant
  with this row-removal. **OPS task 0315 (run the batch on prod) is retired by this task.**
  History cleanup is AC#6 here (delete the ghost rows), not a flip.
- **Gate proven on prod cohorts** (the in-stage `sac_override_from_event_topics` skip relies
  on it): deployed SACs **556/556 skipped**, orphan SACs **5,558/5,558 skipped**, real NFTs
  **0/52 skipped** (52/52 kept). The skip never loses a real NFT (disjoint preimage:
  `derive_sac(Asset)` can't equal a WASM `derive_sac(Address(deployer,salt))`).
- **Residual bucket:** ~1,297 rows are `is_sac=false, deployed=0, wasm NULL, contract_type
NULL` — un-deployed SACs that emit NO SAC-control event (not gate-confirmable) plus
  generic phantom stubs. The `coalesce(deployed,0)=0 AND wasm_hash IS NULL` cleanup predicate
  covers them; confirm none are real pre-window deploys before deleting.

## Acceptance Criteria

- [ ] Writer no longer creates `soroban_contracts` rows for un-deployed SACs (Pass-2 skip + skeleton removal); unit-tested (a SAC-event-only ledger writes no contract row for it)
- [ ] `/v1/contracts` returns only deployed instances (no un-deployed-SAC rows) — verified
      on prod counts. **Cleanup predicate is `coalesce(deployed_at_ledger,0)=0`, NOT
      `IS NULL`** — `IS NULL` misses the ~235k `=0`-sentinel skeleton rows.
- [ ] The 3 INNER joins are LEFT; a tx/event/NFT referencing a row-less contract no longer
      vanishes (regression test)
- [ ] Un-deployed-SAC assets present in `assets`
- [ ] Deployed SACs (e.g. USDC) unaffected — still listed with `is_sac=true` + deploy
- [ ] Historical cleanup decided (delete existing un-deployed-SAC rows — guard the predicate
      against pre-window-deployed real SACs; or leave + let them age out)
- [ ] **~87M false `nfts_pending` + `nft_ownership_pending` rows for crypto-proven-SAC
      contracts dropped** (the batch that drained them is gone; the gate only stops new ones)
- [ ] **Dead `sac_overrides` plumbing removed** (or `identity` repurposed for the assets AC) —
      no orphaned `derive_sac_overrides_from_assets` chain left behind

## Docs updated

- `docs/architecture/database-schema/*` (soroban_contracts = deployed instances) and
  `docs/architecture/xdr-parsing/*` (no contract row for un-deployed SACs) — when implemented.
