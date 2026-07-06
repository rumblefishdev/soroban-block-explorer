---
id: '0355'
title: 'PERF: nftdetail — resolver swap (deferred from 0354, needs NFT test data)'
type: PERF
status: backlog
related_adr: []
related_tasks: ['0354', '0345']
tags:
  [priority-medium, effort-small, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Deferred from 0354 — same id-IN resolver swap, but the local 25k-ledger DB has 0 NFTs so it cannot be locally diffed there.'
---

# PERF: nftdetail — resolver swap

## Summary

`nftdetail` (`GET /nfts/{contract}/{token}`) read **~27M rows/request** in the
2026-07-03 prod load test — the same whole-dimension `JOIN accounts` /
`JOIN soroban_contracts` anti-pattern that 0344/0345/0354 removed elsewhere.
It was deferred from 0354 only because the local test DB has **0 NFTs**, so the
before/after byte-identical diff can't be done there.

## Context

`nfts::fetch_by_composite` (`crates/api/src/nfts/queries_ch.rs`): after the `cid`
CTE resolves the contract StrKey → surrogate and the `nfts n FINAL` PK seek on
`(contract_id, token_id)` (tiny), two joins build whole-dimension hashes:

```
LEFT JOIN soroban_contracts sc ON sc.id = n.contract_id       -- whole ~25M
LEFT JOIN accounts          own ON own.id = n.current_owner_id -- whole ~23M
```

(The code comment "one owner id is a cheap `idx_acc_id` bloom probe" is WRONG —
a JOIN cannot use the bloom skip index; only `WHERE id IN (lit)` does, which is
exactly the shipped `common/ch.rs` resolver.)

## Implementation

- Select the surrogates (`n.contract_id`, `n.current_owner_id`) instead of the
  joined StrKeys; drop the `sc` / `own` joins.
- Resolve in Rust via the shared `resolve_contracts` (contract_id) and
  `resolve_accounts` (owner) — a single-row detail, so ≤1 id each.
- Preserve `nullIf('')` for `owner_account` (LEFT-join-miss → None) and the
  `sc.contract_id` output. `n.contract_id` is Int64 (non-null); check
  `n.current_owner_id` nullability (unowned NFT → NULL owner) → `Option<i64>`.
- Check the sibling NFT fns (`fetch_list`, transfers) for the same join and fold
  them in if present.

## Acceptance Criteria

- [ ] `nftdetail` resolves owner + contract via id-IN; no whole-`accounts`/`soroban_contracts` read
- [ ] Output byte-identical — verified on a DB range that CONTAINS NFTs (backfill a
      Soroban-NFT ledger range locally) OR a careful prod before/after diff
- [ ] `n.current_owner_id` nullability typed correctly (no RowBinary schema mismatch — cf. the 0344 `source_id` bug)
- [ ] Query-only, no schema change
