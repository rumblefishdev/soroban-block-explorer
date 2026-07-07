---
id: '0355'
title: 'PERF: nftdetail — resolver swap (deferred from 0354, needs NFT test data)'
type: PERF
status: completed
related_adr: []
related_tasks: ['0354', '0345', '0357']
tags:
  [priority-medium, effort-small, layer-clickhouse, milestone-3, phase-launch]
milestone: 3
links: []
history:
  - date: 2026-07-03
    status: backlog
    who: fmazur
    note: 'Deferred from 0354 — same id-IN resolver swap, but the local 25k-ledger DB has 0 NFTs so it cannot be locally diffed there.'
  - date: 2026-07-06
    status: active
    who: stkrolikiewicz
    note: >
      Activated as the first fix in the 0357 launch perf cluster. Re-confirmed on
      the 2026-07-06 prod load test: nftdetail p95 ~7-9 s, ch read_rows ~24.7M/req
      (whole-dimension JOIN). The "needs NFT test data" blocker is stale — prod
      has ~12.8k NFTs, so verify via a prod before/after diff (AC already permits).
  - date: 2026-07-06
    status: completed
    who: stkrolikiewicz
    note: >
      Implemented + merged (PR #314, develop af774044). Swapped the two
      whole-dimension JOINs for an input echo (contract) + resolve_accounts
      (owner). Verified byte-identical on prod (6/6 sampled NFTs); read_rows
      24.18M -> ~103k (~235x). cargo check + clippy -D warnings + high code-review
      all clean. Live on the next compute deploy (rides with the loadTesting
      rollback). 1 file, +49/-36.
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

- [x] `nftdetail` resolves owner + contract via id-IN; no whole-`accounts`/`soroban_contracts` read
- [x] Output byte-identical — verified via prod before/after diff (6/6 sampled NFTs, `verify_0355_nftdetail.sh`)
- [x] `n.current_owner_id` nullability typed correctly (`Option<i64>`; no RowBinary mismatch — confirmed in review)
- [x] Query-only, no schema change

## Implementation Notes

`fetch_by_composite` (`crates/api/src/nfts/queries_ch.rs`), 1 file, +49/-36:

- Dropped `LEFT JOIN soroban_contracts sc` and `LEFT JOIN accounts own` (the ~25M
  / ~23M whole-dimension hash reads). `NftChRow` now leads with the raw
  `current_owner_id: Option<i64>` surrogate; the enrichment `ne` collapse and the
  `nfts n FINAL` PK seek are unchanged.
- Contract StrKey: **echoed from the request input** (see Emerged #2), not resolved.
- Owner StrKey: resolved in Rust via the shared `resolve_accounts` (a `WHERE id IN`
  bloom seek on the single owner id), with `.filter(|s| !s.is_empty())` reproducing
  the old `nullIf(own.account_id, '')` (miss -> None, empty -> None, present -> Some).
- `fetch_list` / `fetch_transfers` already used bloom-pruned CTEs — left untouched.

Result: read_rows 24.18M -> ~103k (seek 62k + resolver 41k), ~235x; p95 ~5 s -> ms.

## Design Decisions

### From Plan

1. **Owner via `resolve_accounts`** — the id-IN bloom resolver the task specced,
   matching 0344/0345/0354 and the six other endpoint modules.

### Emerged

2. **Echo the contract StrKey from the input instead of `resolve_contracts`** — the
   plan called for resolving both surrogates, but the contract StrKey is the request
   _input_ and the `nfts` seek filters by it (`WHERE n.contract_id IN (cid)`), so the
   old `sc.contract_id` output is provably equal to the input. Echoing it drops one
   resolver round-trip — output-identical, verified. `resolve_contracts` is unused here.
