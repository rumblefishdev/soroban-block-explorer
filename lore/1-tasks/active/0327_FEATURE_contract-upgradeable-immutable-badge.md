---
id: '0327'
title: 'FEATURE: contract Upgradeable/Immutable badge — WASM-import mutability detection + API field + FE chip'
type: FEATURE
status: active
related_adr: ['0032']
related_tasks: ['0320', '0325']
tags: [xdr-parser, soroban, classification, api, frontend, contract-detail]
links:
  - https://github.com/stellar/stellar-protocol/blob/master/core/cap-0046-02.md
history:
  - date: 2026-06-25
    status: backlog
    who: karolkow
    note: >
      Spawned alongside 0320. 0320 (stale wasm_hash on upgrade) established the
      research: a contract is upgradeable iff its WASM imports the host fn
      `update_current_contract_wasm`; immutable ("frozen") iff it does not. There
      is NO on-ledger immutability flag — the only signal is the import set. This
      task surfaces that as a user-visible badge on the contract page. Kept
      separate from 0320 (which was scoped down to the data-correctness fix).
  - date: 2026-06-25
    status: active
    who: karolkow
    note: Promoted to active to begin implementation.
---

# FEATURE: contract Upgradeable / Immutable badge

## Summary

Show on the contract detail page whether a Soroban contract can still upgrade
its own code (**Upgradeable**) or has given that up (**Immutable / frozen**).
Surfaced as a small status chip, styled like the existing account **deleted**
chip (task 0324).

## Background — how mutability is decided (from 0320 research)

- A contract upgrades its WASM via the host fn `update_current_contract_wasm`
  (CAP-0046-02). That host fn is an **import** in the contract's WASM (Soroban
  env spec: ledger module `"l"`, function export `"6"`).
- There is **no on-ledger immutability flag**. The authoritative signal is the
  import table: WASM imports `update_current_contract_wasm` → **Upgradeable**;
  it does not → **Immutable**. (Verified on mainnet during 0320: real frozen
  contracts exist — several top wasms lack the import.)
- Detection runs on the contract's CURRENT WASM (keyed by the live `wasm_hash`,
  which 0320 now keeps correct after upgrades).

## Scope

1. **xdr-parser** — scan a WASM's import section for `update_current_contract_wasm`
   (`"l"` / `"6"`); expose an `upgradeable: bool` on the interface extraction.
2. **Persistence** — store the flag where the contract's WASM interface lives
   (keyed by `wasm_hash`), so it re-resolves correctly after an upgrade.
3. **API** — add the mutability field to the contract detail DTO + query
   (3-state: `Upgradeable` / `Immutable` / `Unknown` when WASM not available).
   Regenerate `libs/api-types` (CI gate).
4. **Frontend** — render the chip on the contract detail page, matching the
   account `deleted` chip pattern. `Unknown` → render nothing.
5. **Backfill** — populate the flag for already-ingested contracts (in-CH or
   re-derive from stored WASM interface metadata; no S3 re-parse if the import
   set is already captured, else a parse pass).

## Acceptance criteria

- [x] Parser returns `upgradeable` for a WASM with/without the import — unit tests
      (synthetic + multi-import skip) plus a real upgradeable + real frozen mainnet
      WASM fixture (`crates/xdr-parser/tests/upgradeable_real_wasm.rs`).
- [x] API contract detail returns the 3-state field; OpenAPI + api-types regen'd.
- [x] FE shows the mutability chip; nothing on Unknown. Labels are
      **"Self-upgradeable" / "No self-upgrade"** (not "Upgradeable/Immutable") —
      they state exactly what the import scan proves and avoid overclaiming
      immutability for proxy/delegate or renounced-admin contracts.
- [x] Spot-check sample of mainnet contracts matches their real import set — 50 top
      WASMs fetched live from Soroban RPC, shipped parser vs an independent scanner:
      0/50 disagreements (covers ~95% of WASM contracts).
- [x] Flag stays correct across a WASM upgrade — keyed by `wasm_hash`, re-resolves
      on the new hash; the API-side per-hash lookup is covered by
      `queries_ch::map_upgradeable` and the across-upgrade flip by
      `tests/upgradeable_real_wasm.rs::upgrade_reresolves_flag_on_new_wasm`.
      HARD DEPENDENCY: 0320's prod backfill (ops task 0326) must run first, else
      ~1,351 already-upgraded contracts read the flag off their stale
      (deploy-time) `wasm_hash`.

## Backfill (scope item 5)

`wasm_interface_metadata` is `ReplacingMergeTree` keyed by `wasm_hash`; existing
rows lack the `upgradeable` key → read as Unknown (no chip). The bit is NOT
derivable from current CH data — adversarial deep-dive (devils-advocate) measured
every CH-only proxy and all fail: `executable_update` event history misses 69% of
upgradeable contracts (capability ≠ history); function-name heuristics cap at ~92%
(structural floor: obfuscated ABIs + governance/factory false-positives);
`ContractCodeEntryExt.nImports` is a non-discriminating count and isn't persisted.
Only the raw WASM import set works → backfill must re-read the bytecode.

Backfill = `backfill-runner upgradeable-backfill`: read rows missing the key,
**scoped to wasm_hashes that are the current code of a live contract** (the only
ones a contract page reads; all RPC-resolvable), fetch the WASM per `wasm_hash`
from Soroban RPC (`getLedgerEntries` / `LedgerKey::ContractCode`), run the shipped
parser, re-INSERT the merged JSON (Replacing dedups). **Hard-failing**: writes the
resolved rows then errors if ANY target WASM couldn't be resolved (or any bad hex),
rather than silently leaving gaps; idempotent re-run retries only the still-missing.
Validated: all 2,673 in-use WASMs resolve on RPC (0 missing), shipped parser vs an
independent scanner = 0 disagreements. Code lands in this PR; the prod run is a
separate ops step (mirrors 0320 → 0326).

## Docs updated (ADR 0032)

- [x] `docs/architecture/backend/backend-overview.md` — `GET /contracts/:id` documents the 3-state `upgradeable` field + its CH-JOIN derivation.
- [x] `docs/architecture/xdr-parsing/xdr-parsing-overview.md` §5.4 — import-scan / mutability-bit responsibility added.
- [x] `docs/architecture/database-schema/endpoint-queries-clickhouse/11_get_contracts_by_id.sql` — header query LEFT JOINs `wasm_interface_metadata`, projects `upgradeable_has`/`upgradeable_val`. No new column/table (reuses `metadata` JSON).
- [x] frontend data contract — `libs/api-types` regenerated (`upgradeable?: boolean | null`); chip in `web/src/pages/ContractDetailPage.tsx`.
