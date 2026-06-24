---
id: '0320'
title: 'BUG: xdr-parser drops WASM-upgrade — upgraded contracts keep stale wasm_hash + classification'
type: BUG
status: active
related_adr: []
related_tasks: ['0295', '0316', '0283']
tags:
  [
    xdr-parser,
    soroban,
    clickhouse-rmt,
    classification,
    priority-low,
    effort-medium,
  ]
links: []
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
---

# BUG: WASM-upgrade not re-classified

## Summary

`extract_contract_deployments` records a contract's `wasm_hash` only on the
`created` ContractInstance change (`change_type != "created"` skip). When a
contract upgrades its WASM (a new executable hash on an `updated` ContractInstance),
the parser drops it → `soroban_contracts.wasm_hash` keeps the stale deploy-time
hash forever, and the classification verdict derived from it is never revised.

Low severity: classification is function-NAME based (ADR 0031) and most upgrades
preserve the interface, so the verdict rarely changes — but it is a silent
extraction gap.

## Why it shares 0316's infra (the RMT whole-row limitation)

`soroban_contracts` is `ReplacingMergeTree(wasm_uploaded_at_ledger)` — whole-row
replace, no per-column UPDATE. Updating just `wasm_hash` without clobbering
`deployer` / `deployed_at` / `name` requires a read-modify-write: read the prior
row, carry the identity columns forward, write a full row with the new `wasm_hash`
and a bumped version. This is the SAME RMW pattern 0316 evaluates for the
clobber-on-reference case. The naive "just include `updated` in the filter" flip
was rejected in 0283 — it fabricates a wrong deployer / deploy-ledger and clobbers
the real deploy identity.

## Implementation (drafted under 0295, reverted from code pending this task)

1. **Parser** — sibling fn `extract_contract_wasm_upgrades(changes) ->
Vec<(contract_id, wasm_hash)>`, modelled on `extract_contract_data_name_writes`:
   scan `updated` ContractInstance entries, return `(contract_id, wasm_hash)`. SAC
   instances carry a `stellar_asset` executable (no wasm) → skipped via
   `extract_wasm_hash`. The function + 2 unit tests (red→green) were written under
   0295 and reverted on defer; re-apply here.
2. **Indexer wiring** — collect upgrades in `process.rs` alongside the name-write path.
3. **CH writer RMW** — per upgrade: read prior `soroban_contracts` row, swap
   `wasm_hash`, bump `wasm_uploaded_at_ledger`, preserve deployer/deployed_at/name,
   re-run WASM-spec classification (the existing reclassify path keys off wasm_hash).
4. Cache invalidation so the new verdict takes effect.

## Acceptance Criteria

- [ ] Upgraded contracts re-classify (verdict reflects post-upgrade wasm), no deploy-row clobber
- [ ] Unit tests: parser detection + writer RMW preserves deploy identity
- [ ] Validate against a known upgraded contract on prod
