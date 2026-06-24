---
id: '0320'
title: 'BUG: soroban_contracts keeps stale wasm_hash after WASM-upgrade — stale code hash, interface & classification on contract pages'
type: BUG
status: active
related_adr: []
related_tasks: ['0295', '0316', '0283', '0243']
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
   `prior_wasm_verdicts[new_hash]` (the existing 0283 live-G1 map). On a verdict
   **flip**, re-run the NFT quarantine promote/drop (`reclassify_contracts_from_wasm`
   companion) — the original task omitted this.
2. **Backfill (backfill-runner subcommand, CH-only)** — per upgraded contract, take
   the latest `executable_update.new_hash`, RMW as above. ~1,362 contracts, all
   data already in CH. No S3 re-parse (unlike 0321).
3. **Invariant guard (audit-harness)** — assert
   `soroban_contracts.wasm_hash == latest executable_update.new_hash` for every
   contract that emitted one. Doubles as the acceptance test and the tripwire for
   clobber-back regressions.

## Dependencies / coupling

- **0316 is a hard dependency, not a sibling.** `soroban_contracts` is
  `RMT(wasm_uploaded_at_ledger)` with ≥5 writers. Mutate-in-place silently regresses
  unless every writer carries `wasm_hash` forward (the 0316 whole-row-clobber
  discipline). Do not ship step 1/2 without it.
- **Backend scope = ClickHouse.** The approach rests on `soroban_events` +
  `stage.rs prior_wasm_verdicts` (CH). The PG reclassify path (`write.rs:240`) is
  separate and has no events table; treat PG contracts as retired by **0243** (PG↔CH
  migration) — confirm before relying on it.

## Open decisions (need human)

- **D1 — Backend.** Confirm CH is the contracts source of truth and PG path is
  out of scope (retired by 0243). If PG must also be fixed, that's a separate
  instance-diff approach.
- **D2 — History surface.** `soroban_events` already retains the full old→new
  upgrade chain. Ship "upgrade history / upgradeable: yes" on the contract page from
  events (finding #4), or defer? (Cheap positive; immutability detection stays
  deferred either way.)
- **D3 — Cache.** Backfill writes CH directly, bypassing the API cache
  (`contracts/cache.rs`). Self-healing via TTL, or explicit invalidation/restart
  after backfill? Need to confirm the cache's TTL behaviour.
- **D4 — Priority.** Re-rated low→normal here (user-visible interface staleness on
  top contracts). Confirm.

## Acceptance Criteria

- [ ] Live: an `executable_update` event RMWs `soroban_contracts.wasm_hash` +
      contract_type, preserving deploy identity (no clobber); verdict flips re-run
      the NFT quarantine promote/drop
- [ ] Backfill: all 1,362 existing upgraded contracts corrected in-CH (no S3 re-parse)
- [ ] Audit-harness invariant: `wasm_hash == latest executable_update.new_hash` for
      every contract emitting one — green
- [ ] Validate ≥20 contracts' corrected hash against Soroban RPC ground-truth
- [ ] 0316 writer discipline confirmed (no co-writer clobber-back)

## Superseded notes

The 0295 draft (`extract_contract_wasm_upgrades` scanning `updated` ContractInstance
entries + 2 unit tests, reverted on defer) is **superseded** by the event approach —
it depended on the unconfirmed `updated`-entry XDR shape and would have collided with
TTL-restore noise. Do not re-apply.
