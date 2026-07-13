---
title: 'Role cross-reference — ParticipationRole ↔ XDR field ↔ Horizon effect (official grounding)'
type: generation
status: mature
spawned_from: notes/G-schema-and-roles.md
spawns: []
tags: ['roles', 'cross-validation', 'horizon', 'xdr', 'ops-validation']
links:
  - https://github.com/stellar/stellar-xdr/blob/curr/Stellar-transaction.xdr
  - https://developers.stellar.org/docs/data/apis/horizon/api-reference/resources/effects/types
history:
  - date: 2026-07-08
    status: mature
    who: karolkow
    note: >
      Requested certainty grounding for the role dictionary. Every role mapped
      to its OFFICIAL protocol source (XDR operation/result field) and the
      closest OFFICIAL vocabulary (Horizon effect type). Doubles as the ops
      validation contract (which Horizon surface validates which role).
---

# Role cross-reference — official grounding

The role NAME set is ours (no official per-asset participation taxonomy
exists — that's the product gap 0359 fills). But every role is anchored 1:1
in two OFFICIAL sources:

1. **XDR protocol definitions** (stellar-core `Stellar-transaction.xdr`) — the
   named operation/result fields the role is read from. This is what the
   emitter (`xdr-parser/src/participations.rs`) literally matches on.
2. **Horizon effect types** (developers.stellar.org, Horizon API reference →
   Effects) — the closest official vocabulary for "what happened to whom";
   used as the VALIDATION surface in the ops phase.

| ParticipationRole (i16)   | XDR source field (official)                                                | Closest Horizon effect (official)                    | Validation surface        |
| ------------------------- | -------------------------------------------------------------------------- | ---------------------------------------------------- | ------------------------- |
| `payment` = 0             | `PaymentOp.asset`; `CreateAccountOp.startingBalance`; `AccountMergeResult` | `account_credited` + `account_debited`               | Horizon `/operations`     |
| `sent` = 1                | `PathPaymentStrictSend/ReceiveOp.sendAsset`                                | `account_debited`                                    | Horizon op `source_asset` |
| `received` = 2            | `PathPaymentStrictSend/ReceiveOp.destAsset`                                | `account_credited`                                   | Horizon op `asset`        |
| `sold` = 3                | `ManageSellOffer/ManageBuyOffer/CreatePassiveSellOfferOp.selling`          | (offer has no effect until fill)                     | Horizon op `selling_*`    |
| `bought` = 4              | `…Op.buying`                                                               | (as above)                                           | Horizon op `buying_*`     |
| `traded` = 5              | result `ClaimAtom.assetSold` + `.assetBought` (order-book AND LP)          | `trade` (one per crossed offer, both sides)          | **Horizon `/trades`**     |
| `trustline` = 6           | `ChangeTrustOp.line` (non-PoolShare)                                       | `trustline_created/updated/removed`                  | Horizon `/effects`        |
| `escrowed` = 7            | `CreateClaimableBalanceOp.asset`                                           | `claimable_balance_created`                          | Horizon `/effects`        |
| `released` = 8            | meta: removed `ClaimableBalanceEntry.asset` (op body has only `balanceId`) | `claimable_balance_claimed`                          | Horizon `/effects`        |
| `clawed_back` = 9         | `ClawbackOp.asset`; meta CB entry for `ClawbackClaimableBalanceOp`         | `account_debited` (clawback) / CB clawback effect    | Horizon `/effects`        |
| `authorize` = 10          | `SetTrustLineFlagsOp.asset` (target = `trustor`, a third party)            | `trustline_flags_updated` (also `authorized/deauth`) | Horizon `/effects`        |
| `lp_a` = 11 / `lp_b` = 12 | meta: `LiquidityPoolEntry.params.assetA/assetB` (body has only pool id)    | `liquidity_pool_deposited` / `_withdrew`             | Horizon `/effects`        |

Recorded N/A (official grounding for the SKIPS): `set_options`, `manage_data`,
`bump_sequence`, sponsorship 16–18 (reserve mechanics, no asset field in XDR),
`inflation` (dead on mainnet), `allow_trust` (deprecated in favour of
`set_trustline_flags`; deferred — asset is code-only + issuer = op source),
`invoke_host_function` (token flow lives in Soroban EVENTS — step 6 / SEP-41
`transfer/mint/burn` events, the industry-standard decode path).

**Ops validation contract (from this table):** endpoint roles (0–4, 6–12)
validate 1:1 against Horizon operation objects + effects; `traded` (5)
validates against Horizon `/trades`. No single external source validates the
UNION (stellar.expert has no public per-asset API) — validate per-arm.
