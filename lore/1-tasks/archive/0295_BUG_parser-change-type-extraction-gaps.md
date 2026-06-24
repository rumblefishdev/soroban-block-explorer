---
id: '0295'
title: 'BUG: xdr-parser change-type extraction gaps — WASM-upgrade not reclassified + AccountMerge balance tombstone'
type: BUG
status: completed
related_adr: []
related_tasks: ['0283', '0228', '0316', '0320', '0321']
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
  - date: 2026-06-23
    status: completed
    who: karolkow
    note: >
      Bug-2 (AccountMerge native tombstone) shipped — PR #276, 268 xdr-parser
      lib tests green, 5-agent review (correctness/simplify/adversarial/senior)
      no correctness issues. Scope split during impl: bug-1 (WASM-upgrade
      re-classify) → 0320, broader RMT whole-row clobber → 0316, existing-ghost
      backfill → 0321. Native ghost impact measured ~12.4M XLM / ~522k accounts
      (prod CH + Horizon 404).
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

## Implementation Notes

- Added a `removed` arm to pass-1 of `extract_account_states` (`crates/xdr-parser/src/state.rs`): on a `removed` account `LedgerEntryChange` (the only delete path on Stellar = AccountMerge), emit an `ExtractedAccountState` with native `balance=0` at the merge ledger; `account_id` from `change.key` (removed entries carry no `data`). Touches only the native balance.
- The balances table is `ReplacingMergeTree(last_updated_ledger)`, so the zero stamped at the (higher) merge ledger supersedes the stale positive row.
- Tests: split `skip_state_and_removed_accounts` → `skip_state_only_account` + new `removed_account_emits_zero_native_tombstone`. 268 lib tests green.
- PR #276.

## Design Decisions

### From Plan

1. **Native balance=0 tombstone at the merge ledger** — exactly Step 2 of the plan; the parser-side rework of the 0228 "skeleton floor".

### Emerged

2. **Scope split: bug-1 → 0320, clobber → 0316, backfill → 0321.** The original task bundled bug-1 (WASM-upgrade) + bug-2. During impl, bug-1's correct fix needs a CH read-modify-write writer (the naive filter-flip clobbers deploy identity, rejected in 0283), so it was deferred to **0320**. While verifying the tombstone doesn't clobber identity columns, found a broader pre-existing RMT whole-row clobber (`accounts.home_domain` etc., ~38.8k accounts) → spawned umbrella **0316**. Fix-forward only; the ~522k existing native ghosts need a one-shot backfill → **0321** (DB-only `backfill-runner` subcommand, no S3 re-parse).
3. **Touch only the native balance; leave identity columns to the existing participant path.** The tombstone carries `sequence=-1`/`home_domain=None` (guarded on PG; on CH the pre-existing whole-row clobber is the 0316 concern, not introduced here).

## Issues Encountered

- **`operations_appearances` includes failed-tx ops** (no success flag in that table). A failed AccountMerge leaves a type=8 op but the account stays alive — so merge/create detection from that table has false positives. The native-ghost query is robust because the `last_updated <= merge_ledger` filter screens alive accounts (they keep transacting), and 9/9 sampled strict ghosts were Horizon 404; flagged a `successful=1` join as the bulletproof option for 0321. The parser tombstone itself is immune — a failed tx produces no `removed` ledger change.
- **home_domain clobber discovery** — proven on prod (USDC issuer `circle.com` → NULL across RMT versions); tracked in 0316, not in scope here.
