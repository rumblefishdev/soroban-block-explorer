---
id: '0452'
title: 'FEATURE: derive the SAC address for every classic asset, so its presence stops meaning "this asset has moved"'
type: FEATURE
status: backlog
related_adr: ['0051']
related_tasks: ['0323', '0337', '0339', '0450']
tags: [backend, api, frontend, assets, sac, priority-low, effort-small]
links: []
history:
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Deferred out of 0450. The asset detail page shows a "SAC contract" row
      only for assets we happen to hold a SAC handle for, and since CAP-67 that
      means "assets that have moved" rather than "assets with a contract" — so
      two otherwise identical classic assets disagree on screen for no visible
      reason. 0450 briefly hid the row for un-deployed SACs; that was reverted
      on review as the lossy fix. Keeping an honest "reserved, not deployed"
      row visible surfaces the oddity; the real repair is to make it
      unconditional.
---

# FEATURE: derive the SAC address for every classic asset

## Summary

Every classic asset **has** a Stellar Asset Contract address whether or not
anyone deployed to it — it is `sha256`-derived from `(asset_code, issuer,
network)` and needs no on-chain act to exist. Show it for every classic asset,
with its deployed/reserved status, instead of only for the ones we happen to
know about.

## The problem it fixes

We learn of a SAC address in one of two ways: a real deploy, or a CAP-67 unified
asset event emitted under the derived address. Since CAP-67, ordinary classic
transfers emit those (`transfer`/`mint`/`burn`/`clawback`/`set_authorized`,
`crates/xdr-parser/src/sac.rs:189-190`), so `detect_undeployed_sac_overrides`
records a handle with `sac_deployed = false` for any classic asset that has
moved (task 0323).

Consequence, seen live: `zyx` (supply 0, never moved) shows no SAC row, while
`zxc` (minted, one holder) shows an un-deployed address. Same asset class, same
absence of any contract, different UI — and nothing on screen explains why. The
row's presence encodes activity, not SAC-ness.

## Why it is cheap

`xdr_parser::derive_sac_strkey(asset_code, issuer, network_id)`
(`crates/xdr-parser/src/sac.rs:105`) is pure computation — no query, no lookup.
`map_item` already calls it, just gated behind `sac_contract_surrogate != 0`
(`crates/api/src/assets/handlers.rs:72`). Dropping the gate for classic assets
is the whole backend change.

## Scope

1. `map_item`: derive `sac_contract_id` for every classic-credit and native
   asset, not only those with an observed handle. `sac_deployed` stays as-is —
   it becomes the status of a row that is always present, rather than a gate on
   whether the row exists.
2. Frontend: the detail row is unconditional for classic assets; the status
   line distinguishes deployed (linked) from not-yet-deployed (plain text).
   Reword "Reserved address — not deployed" — nobody reserved anything; it is
   simply where this asset's SAC would live. Something closer to
   "Not deployed — this is where a Stellar Asset Contract for this asset would
   live".
3. Decide whether the assets **list** wants it too. Probably not: the list is
   for identifying and drilling in, and 0450 deliberately gave that column back
   to the issuer.

## Watch out

- **Native XLM** has its own SAC and `derive_sac_strkey` handles the empty
  code/issuer pair — check it renders sensibly rather than as a classic asset.
- **Soroban-native assets have no SAC** (the contract _is_ the asset). The
  derivation must not fire for `asset_type = 3`.
- Do **not** change what `sac_deployed` means, and do not let this leak into the
  `Has SAC` filter or the `SAC` chip — both correctly mean "deployed" today, and
  0450 established that all surfaces should agree on that meaning.

## Acceptance criteria

- [ ] Every classic-credit and native asset exposes a `sac_contract_id`
- [ ] Soroban-native assets still expose none
- [ ] Detail row present for every classic asset, with a status that does not
      imply somebody reserved the address
- [ ] Two classic assets that differ only in whether they have ever moved render
      identically
- [ ] `Has SAC` filter and the `SAC` chip still mean "deployed" — unchanged
- [ ] **Docs updated** — `docs/architecture/**` asset/SAC description if it
      states how `sac_contract_id` is populated, per ADR 0032
- [ ] **API types regenerated** — touches `crates/api/**`; run
      `npx nx run @rumblefish/api-types:generate`
