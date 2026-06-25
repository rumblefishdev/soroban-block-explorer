---
id: '0327'
title: 'FEATURE: contract Upgradeable/Immutable badge — WASM-import mutability detection + API field + FE chip'
type: FEATURE
status: backlog
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

- [ ] Parser returns `upgradeable` for a WASM with/without the import (unit test
      with a real upgradeable + a real frozen mainnet WASM).
- [ ] API contract detail returns the 3-state field; OpenAPI + api-types regen'd.
- [ ] FE shows Upgradeable/Immutable chip; nothing on Unknown.
- [ ] A spot-check sample of mainnet contracts matches their real import set.
- [ ] Flag stays correct across a WASM upgrade (re-resolves on new `wasm_hash`).

## Docs updated (ADR 0032)

- [ ] `docs/architecture/api/**` — new contract-detail field — _or N/A_
- [ ] `docs/architecture/xdr-parsing/**` — import-scan responsibility — _or N/A_
- [ ] `docs/architecture/schema/**` — if a new column/table stores the flag — _or N/A_
- [ ] frontend data contract — _or N/A_
