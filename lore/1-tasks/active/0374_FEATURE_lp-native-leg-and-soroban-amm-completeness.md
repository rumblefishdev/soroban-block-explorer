---
id: '0374'
title: 'LP completeness: native XLM leg match + Soroban-AMM union + share% recompute'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-api, liquidity-pools]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/405'
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker. Bundles F-B/K2-2, K3-5, K4-6.'
  - date: 2026-08-14
    status: backlog
    who: karolkow
    note: >
      Linked issue 405 (add Soroban AMM protocols). Rewrote K3-5: the union is
      the last step, not the work — no Soroban pool state is indexed today.
---

# LP completeness

## Summary

Make liquidity-pool activity complete: match the native XLM leg (currently
unmatchable → 21.7% of pools invisible), union Soroban-AMM pools into
`/liquidity-pools`, and recompute stale `share_percentage`.

## Context

Spawned from 0359. Mostly read/query-side: the native leg fails to match because
of the two-conventions native representation (see memory: native two
conventions); Soroban AMMs live outside the classic pool table.

## Implementation

- **F-B / K2-2** — match the native XLM leg in LP snapshots (16 552 pools /
  21.7% currently invisible).
- **K3-5** — surface Soroban-AMM pools. The union into `/liquidity-pools` is the
  final step; the actual work is extracting pool state we do not index at all
  today (no reserves, no swap volume, no pool row — only the LP token contract).
  One adapter per protocol:
  - **Soroswap first** — its LP tokens already carry on-chain `METADATA` we
    read (248 `…Soroswap…` names in `soroban_contract_metadata`), so pool
    discovery is a lookup.
  - **Aquarius second** — only 19 metadata hits, so its pool contracts must be
    discovered via factory/registry and decoded from swap events. The harder
    half, despite being the more-requested one in issue 405.
  - then union with the classic pools + a Classic/Soroban filter on the list
    (cheap once both live in one list).
  - `ContractType` has no `Dex` variant — 131 740 contracts sit in `Other`.
    Splitting it is anticipated in `crates/domain/src/enums/contract_type.rs`.
- **K4-6** — recompute stale LP `share_percentage` (unconfirmed; verify first).

## Acceptance Criteria

- [ ] native XLM leg matches → pools visible — F-B / K2-2
- [ ] Soroswap pools indexed (reserves + volume) and unioned — K3-5
- [ ] Aquarius pools indexed and unioned — K3-5
- [ ] Classic / Soroban filter on the pool list — K3-5
- [ ] share_percentage correct (or confirmed already correct) — K4-6
