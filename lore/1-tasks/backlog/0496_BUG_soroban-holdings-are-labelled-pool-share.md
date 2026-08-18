---
id: '0496'
title: 'BUG: every Soroban holding is labelled `pool_share` — two asset-type enums collided'
type: BUG
status: backlog
related_adr: ['0051']
related_tasks: ['0463', '0331', '0339']
tags: [backend, api, accounts, data-correctness, priority-medium, effort-small]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found while auditing why LP positions live outside `balances` (0463
      planning). Two enums share the number 3 with different meanings, and the
      API renders the wrong one. Measured on production before filing.
---

# BUG: Soroban holdings render as `pool_share`

## Summary

The account detail response labels **every Soroban (type-3) holding** as
`pool_share`. Measured on production: **42,975 rows across 38,324 holders**
carry `asset_type = 3` with a non-zero amount, and every one of them is
mislabelled.

## Root cause — two enums, one number

```
TokenAssetType  (stored in `assets.asset_type`)   Native=0, ClassicCredit=1, Soroban=3
asset_type_name (rendered by the API)             0 native, 1 credit_alphanum4,
                                                  2 credit_alphanum12, 3 pool_share
```

`crates/domain/src/enums/token_asset_type.rs:24-30` is the project enum;
`crates/api/src/accounts/queries.rs:102-110` is the renderer, which uses the
**XDR** meaning of 3 (`AssetType::PoolShare`, confirmed in `stellar-xdr`
26.0.1). A Soroban holding therefore reads as a liquidity-pool share.

The collision was created, not inherited: ADR 0051 / task 0339 retired type 2
(SAC) and Soroban took 3 — the number XDR already uses for `PoolShare`. The
renderer was never revisited.

Nothing caught it because our real pool shares are not in `balances` at all —
they live in `lp_positions` — so the `pool_share` label had no legitimate
occurrence to be compared against.

## Second defect in the same function

The doc comment calls these "Horizon-style" labels, but Horizon does not emit
`pool_share`. Verified against the live API and the Go SDK
(`protocols/horizon/main.go`): the value is **`liquidity_pool_shares`**. So the
string is wrong even for the case it was written for.

## Fix

- Render from the **project** enum, not the XDR one: type 3 must produce a
  Soroban label, not `pool_share`.
- Decide the exact string deliberately — it is a wire-visible contract
  (`libs/api-types/src/generated/types.gen.ts:20`). Horizon has no Soroban
  equivalent, so this is our own vocabulary; pick it once and use it
  everywhere.
- If a pool-share label is ever needed, it is `liquidity_pool_shares`.
- Check the other renderers for the same collision — `asset_type_name` is
  duplicated in the search path (`search/queries.rs`) and possibly elsewhere.
- Regression test pinning type 3 → the Soroban label.

## Acceptance criteria

- [ ] A Soroban holding no longer reads as `pool_share` on the account page
- [ ] Every copy of the type → label mapping agrees, with one shared source
- [ ] The chosen string is recorded, with the reason it is not Horizon's
- [ ] **Docs updated** — frontend data contract, since the value is on the wire
- [ ] **API types regenerated** if the field's documented values change
      (`npx nx run @rumblefish/api-types:generate`)
