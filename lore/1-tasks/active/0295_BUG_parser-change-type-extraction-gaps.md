---
id: '0295'
title: 'BUG: xdr-parser change-type extraction gaps — WASM-upgrade not reclassified + AccountMerge balance tombstone'
type: BUG
status: active
related_adr: []
related_tasks: ['0283', '0228']
tags:
  [xdr-parser, extraction-completeness, layer-data, priority-low, effort-small]
links: []
history:
  - date: 2026-06-16
    status: backlog
    who: karolkow
    note: >
      Spawned from 0283 future work. Bundles two confirmed parse-time
      extraction gaps with the IDENTICAL shape — the parser drops a
      LedgerEntryChange variant for a given entry type, leaving stale/missing
      derived state. Both low severity, not launch-blocking.
  - date: 2026-06-23
    status: active
    who: karolkow
    note: Activated for implementation.
---

# BUG: xdr-parser change-type extraction gaps

## Summary

The XDR parser silently drops certain `LedgerEntryChange` variants for certain
entry types, leaving stale or missing derived state. Two confirmed cases share
the same mechanism (lossy extraction at parse — recoverable only by re-parse):

1. **WASM-upgrade never reclassified.** `extract_contract_deployments` takes
   only `change_type == "created"` (`state.rs:59`) and drops `updated`
   ContractInstance entries → a contract that upgrades its wasm keeps its stale
   `wasm_hash`/verdict forever. Classification is function-NAME based so most
   upgrades preserve the interface (invisible), hence low severity.
2. **AccountMerge balance tombstone.** `extract_account_states` drops `removed`
   for accounts → a merged account leaves a stale native-balance row with no
   zero tombstone (trustlines DO zero out). The API serves the stale balance and
   a native-XLM aggregate can be inflated.

## Context

Spawned from **0283**. The WASM-upgrade fix was REJECTED-as-naive there: simply
flipping the `created`-only filter to include `updated` fabricates wrong
deployer/`deployed_at_ledger` and CLOBBERS the real deploy row under
`ReplacingMergeTree(wasm_uploaded_at_ledger)` — it needs writer merge-discipline,
not a filter flip.

**AccountMerge — operator note:** the current handling (task 0228 accepts merged
accounts as a "skeleton floor") is to be **reworked** into a proper tombstone:
emit a `balance=0` native row at the merge ledger (account_id from the `removed`
change key), so the balance and aggregates are correct.

## Implementation Plan

### Step 1 — WASM-upgrade

Parser handles `updated` ContractInstance + writer merge-discipline (must NOT
outversion/clobber the real deploy identity under RMT — pin or merge) + cache
invalidation so the new verdict takes effect.

### Step 2 — AccountMerge tombstone

On a `removed` account change, emit a `balance=0` native row at the merge ledger
(account_id from the change key). Rework the 0228 skeleton-floor accordingly.

## Acceptance Criteria

- [ ] ~~Upgraded contracts re-classify~~ — **deferred to [[0320]]** (WASM-upgrade is the same RMT whole-row class; needs read-modify-write writer, split out of 0295)
- [x] Merged accounts show balance 0 — parser `removed` tombstone done (fix-forward); existing ~522k ghosts backfill **deferred to [[0321]]**
- [x] Unit test for the AccountMerge tombstone — `removed_account_emits_zero_native_tombstone` (the WASM-upgrade unit tests move with [[0320]])
