---
id: '0320'
title: 'BUG: soroban_contracts keeps stale wasm_hash after WASM-upgrade — stale code hash, interface & classification on contract pages'
type: BUG
status: active
related_adr: []
related_tasks: ['0295', '0316', '0283', '0243', '0325']
tags:
  [
    xdr-parser,
    soroban,
    clickhouse-rmt,
    classification,
    soroban-events,
    priority-normal,
    effort-medium,
  ]
links:
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-02.md
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-05.md
  - https://developers.stellar.org/docs/build/guides/conventions/upgrading-contracts
history:
  - date: 2026-06-23
    status: backlog
    who: karolkow
    note: >
      Split out of 0295 (was "bug-1" there). Parser + CH writer deferred to this
      task. Shares the read-modify-write writer pattern with 0316 (the RMT
      whole-row limitation class).
  - date: 2026-06-24
    status: active
    who: karolkow
    note: Activated to start implementation.
  - date: 2026-06-24
    status: active
    who: karolkow
    note: >
      Deep research (docs CAP-0046-02/-05 + mainnet RPC/CH cross-check) + two
      devil's-advocate passes. REVISED the whole approach: detect upgrades from
      the already-ingested `executable_update` SYSTEM event (not by diffing
      `updated` ContractInstance entries). Measured prevalence: 1,362 contracts
      upgraded (4,691 events), all non-SAC. Severity raised low→normal (wasm_hash
      + interface are user-visible). Open decisions for human listed below.
  - date: 2026-06-24
    status: active
    who: karolkow
    note: >
      Converted to directory; serialized research into notes/R + notes/S.
      Class-change measured: 0 net across 1,362 contracts (2 transitions on one
      round-trip contract) → fix is "update wasm_hash field", quarantine path is
      dead-code for real data. Decisions D1-D5 resolved (CH-only, ship history,
      cache self-heals @45s TTL, 0320-right+invariant over 0316-first, priority
      normal).
  - date: 2026-06-24
    status: active
    who: karolkow
    note: >
      D4 locked = option C (0320-right + invariant + narrow wasm_hash carry-forward
      audit; reject clobber-then-fix and 0316-first). Rare class-flip handling +
      verify-real-vs-parse-artifact spun out to 0325. 0320 scope now = update
      wasm_hash + verdict only.
  - date: 2026-06-24
    status: active
    who: karolkow
    note: >
      C refined (C'): 0320 only does its OWN write correctly via a sibling prefetch
      (stage.rs has verdicts, not full rows → SELECT deployer/deployed_at/name/is_sac
      for upgraded contract_ids, like fetch_prior_contract_verdicts). Other-writer
      clobber audit + engine change (CoalescingMergeTree/SimpleAggregateFunction) +
      removing this prefetch-read all moved to 0316 (its Phase-0 recon gates whether
      the big engine change is worth it vs keeping read-modify-write).
---

# BUG: WASM-upgrade leaves stale wasm_hash (+ interface + classification)

## Summary

When a Soroban contract upgrades its WASM (`update_current_contract_wasm`),
`soroban_contracts.wasm_hash` keeps the **deploy-time** hash forever. The API
serves that stale hash AND the stale interface (`fetch_wasm_interface` joins
`wasm_hash → wasm_interface_metadata`), so the contract page shows the wrong
code hash and wrong function list. Classification (derived from the interface)
is also stale, though it rarely flips in practice.

**Measured on prod (2026-06-24): 1,362 contracts have upgraded, 4,691 upgrade
events, all non-SAC.** Includes the network's single busiest contract
(`CDL74RF5…`, 582M invocations, on its 18th WASM version). Not "rare/low" —
it's a user-visible correctness bug on the most-viewed contracts.

## Research findings (docs + mainnet — verified, not from repo/spec assumptions)

Primary-source confirmed (CAP specs, 3-0 adversarial verify) **and**
cross-checked against mainnet (Soroban RPC ground-truth + prod CH):

1. **Upgrade mechanism** — contract calls `update_current_contract_wasm(new_hash)`
   on itself; new WASM must be pre-uploaded; swap applies after the invocation.
   **SACs cannot upgrade** (no WASM executable) — they are 311k/424k of all
   contracts, structurally out of scope. [CAP-0046-02]
2. **Same `contract_id`** — wasm_hash is not an input to address derivation. There
   is **no "new instance"** on upgrade; modeling one would misrepresent the chain.
   [CAP-0046]
3. **In-place mutation + a SYSTEM event** — the upgrade mutates `executable.wasm_hash`
   in the single CONTRACT_DATA instance entry and emits
   `["executable_update", old_executable, new_executable]`. `SCContractInstance`
   has only `{executable, storage}` — **no immutability field exists**. [CAP-0046-05]
4. **No on-ledger immutability flag** — "non-upgradeable" = the code never exposes an
   upgrade path. Detectable only heuristically; "has emitted `executable_update`" is a
   definitive _positive_ (upgradeable), but immutability is never a clean boolean.
   ("renounce-owner ⇒ immutable" theory refuted.) [CAP-0046-02]
5. **Interface can change across upgrade** (NFT-like ↔ token-like is possible) →
   classification is time-varying per `contract_id`. Empirically rare:
   the upgrade we inspected stayed `Other→Other`; only 110 Nft + 4,065 Fungible
   verdicts exist total. [SEP-0049 / upgrade guide]
6. **TTL archival/restore** reactivates the **same** entry without changing
   `contract_id` or `wasm_hash` (P23 auto-restore). The `executable_update` event
   only fires on a real executable change → **immune to restore noise** (unlike the
   `updated`-entry-diff approach). [CAP-0046-12, CAP-0066]

**We already ingest the signal.** `soroban_events` holds `executable_update` as
decoded JSON: `topics = [Symbol("executable_update"), [Symbol("Wasm"), old], [Symbol("Wasm"), new]]`.
The new-hash of a contract's **latest** event = its current on-chain executable,
validated against Soroban RPC for the 3 busiest upgraders (CDL74RF5/CCABO2IQ/CA6PUJLB
all match exactly). **0 of 1,362** upgraded contracts have a current wasm missing
from `wasm_interface_metadata` → reclassification has the data it needs; no
"upgrade → Other" regression.

**Class never net-changes (the scope-defining measurement).** Classified every
wasm in all upgrade chains (production rule, computed in CH, full coverage):
across **4,691 transitions, only 2 changed class** — `Other→Fungible` then
`Fungible→Other` on a single contract (`CDCN2D4O…`, 37 upgrades) that reverted.
**Per-contract net deploy→current: 0 class changes** (922 Other, 426 Fungible,
14 Nft). **0 NFT/asset flips ever.** ⇒ at current state the fix is "update the
`wasm_hash` field" for 1,362/1,362. The rare class-flip handling (reclassify +
NFT quarantine promote/drop) and verifying that one flip is real vs a parse
artifact are **deferred to [[0325]]** — out of scope here. See
[notes/R-soroban-upgrade-research.md](notes/R-soroban-upgrade-research.md).

**Caveats the research could NOT confirm** (so we do NOT depend on them): the exact
`LedgerEntryChange` variant on upgrade (single `updated` vs `state`+`updated` pair)
was never empirically pinned — the original parser plan depended on it. The
event-based approach sidesteps this entirely.

## Revised implementation plan (event-based — supersedes the 0295 parser draft)

Detect upgrades from `executable_update` events, not from `updated` ContractInstance
diffs. This is already-ingested, carries both hashes, is restore-noise-immune, and
is **backfillable in-CH (no S3 re-parse)**.

1. **Live path (CH `stage.rs`)** — when an `executable_update` event is staged for a
   contract, RMW `soroban_contracts`: write a new row with the new `wasm_hash`,
   `wasm_uploaded_at_ledger = upgrade_ledger` (higher RMT version → wins the merge),
   **carry forward** deployer/deployed_at/name/is_sac, and set `contract_type` from
   `prior_wasm_verdicts[new_hash]` (the existing 0283 live-G1 map). Class **flips**
   (verdict differs across the upgrade) → **[[0325]]**; current data never flips net,
   so 0320 just writes the new hash + verdict.
   - **Where the carry-forward values come from:** `stage.rs` currently only has
     prior _verdicts_ (`prior_wasm_verdicts` / `prior_contract_verdicts`), NOT the
     full row. Add a **sibling prefetch** for the upgraded contract_ids —
     `SELECT deployer_id, deployed_at_ledger, name, is_sac FROM soroban_contracts
FINAL WHERE contract_id IN (…)` — idiomatic, same shape as
     `fetch_prior_contract_verdicts`. Cheap: only contracts with an `executable_update`
     in the batch (rare). This read is 0320's stop-gap; 0316 may remove it (see below).
2. **Backfill (backfill-runner subcommand, CH-only)** — per upgraded contract, take
   the latest `executable_update.new_hash`, RMW as above. ~1,362 contracts, all
   data already in CH. No S3 re-parse (unlike 0321).
3. **Invariant guard (audit-harness)** — assert
   `soroban_contracts.wasm_hash == latest executable_update.new_hash` for every
   contract that emitted one. Doubles as the acceptance test and the tripwire for
   clobber-back regressions.

## Dependencies / coupling

- **Backend scope = ClickHouse** (D1). The approach rests on `soroban_events` +
  `stage.rs prior_wasm_verdicts` (CH). The PG reclassify path (`write.rs:240`) is
  separate and has no events table; PG is retired by **0243** — out of scope.
- **0316 owns the systematic part (D4).** `soroban_contracts` is
  `RMT(wasm_uploaded_at_ledger)` with ≥5 writers; a co-writer that rewrites an
  already-upgraded row carrying the old `wasm_hash` would silently regress it.
  **0320 does NOT audit/fix the other writers** — that, plus the engine question
  (`CoalescingMergeTree` / `SimpleAggregateFunction` to drop read-first everywhere)
  and **removing 0320's stop-gap prefetch**, all move to **0316** (gated by its
  Phase-0 "is it even worth it" recon). 0320 only guarantees _its own_ write is
  correct (carry-forward) and ships the **invariant as a tripwire** — if a co-writer
  clobbers an upgraded row, it goes red and feeds 0316.

## Decisions (resolved 2026-06-24)

- **D1 — Backend: ClickHouse only.** PG retired (0243). ✓
- **D2 — Ship upgrade history + "upgradeable: yes".** Source = `soroban_events`
  chain (count + old→new list); "upgradeable" = emitted ≥1 `executable_update`.
  Immutability (hard negative) stays deferred. ✓
- **D3 — Cache self-heals.** `contracts/cache.rs` = moka, fixed **45s TTL** (Lambda,
  per-instance). No explicit invalidation needed; ≤45s staleness after backfill. ✓
- **D4 — Sequencing: option C (locked, refined).** 0320 ships its own RMW correctly
  (sibling prefetch → carry-forward → write full row) + the audit-invariant tripwire.
  It does **not** touch the other writers. The systematic clobber audit, the engine
  change, and removing 0320's prefetch-read all belong to **0316** (its Phase-0 recon
  decides if the engine change is even worth it; if only 1–2 cases, read-modify-write
  stays the permanent answer). Rejected B (ship known clobber) and A (block on full 0316). ✓
- **D5 — Priority: normal** (was low). ✓

See [notes/S-event-based-decision.md](notes/S-event-based-decision.md) for rationale
and the wasm-row data-model (interfaces append-only, pointer overwritten, history in events).

## Acceptance Criteria

- [ ] Live: an `executable_update` event RMWs `soroban_contracts.wasm_hash` +
      contract_type (carry-forward all identity columns, no clobber). Class flips → [[0325]]
- [ ] Backfill: all 1,362 existing upgraded contracts corrected in-CH (no S3 re-parse)
- [ ] Audit-harness invariant: `wasm_hash == latest executable_update.new_hash` for
      every contract emitting one — green (also the clobber-back tripwire)
- [ ] Validate ≥20 contracts' corrected hash against Soroban RPC ground-truth (done in
      research: 28/28 — re-run post-fix)
- [ ] D2: contract page shows upgrade history + upgradeable flag from events

## Superseded notes

The 0295 draft (`extract_contract_wasm_upgrades` scanning `updated` ContractInstance
entries + 2 unit tests, reverted on defer) is **superseded** by the event approach —
it depended on the unconfirmed `updated`-entry XDR shape and would have collided with
TTL-restore noise. Do not re-apply.
