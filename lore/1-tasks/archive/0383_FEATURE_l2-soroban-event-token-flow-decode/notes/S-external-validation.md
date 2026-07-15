---
prefix: S
title: 'Decoder external validation — Horizon + stellar.expert + raw XDR (ground truth)'
status: mature
spawned_from: '0383'
date: 2026-07-14
who: karolkow
---

# S — External validation of the token-event decoder

Cross-checked the 0383 decode (`parse_token_event` / `derive_token_event`)
against independent sources on REAL prod transactions. Two passes.

## Pass 1 — native transfers (3 txs, 10 events)

Txs `258c3243…`, `0bda1a13…`, `2d90057a…` (multi-hop DEX swaps, native XLM).

| Source                                                          | Result                                                                                                       |
| --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| Horizon effects                                                 | 10/10 MATCH — every `from` debited, every `to` credited, native; account set = decoded from/to union exactly |
| stellar.expert (via `api.stellar.expert` XDR)                   | byte-exact MATCH on all native legs                                                                          |
| Raw XDR (py-stellar-sdk, `TransactionMeta` v4 from Soroban RPC) | byte-exact MATCH incl. event ordering                                                                        |

**Scare investigated + closed:** the sample was accidentally native-only
(`LIMIT 10` landed on native rows). Verified directly that `soroban_events` DOES
contain the non-native legs (USDC/BLND/EURC) for these txs, and the decoder
handles them (`EventAsset::Credit`, unit-tested). Not a gap — a sampling artefact.

## Pass 2 — diverse verbs + asset types (4 txs)

Deliberately spanning transfer/mint/burn and credit/bespoke:

| tx          | verb / asset                                 | our parser      | raw XDR (ground truth)                     | Horizon effects                           |
| ----------- | -------------------------------------------- | --------------- | ------------------------------------------ | ----------------------------------------- |
| `84dd8ffe…` | transfer, credit **XLD**                     | ✅              | MATCH (XLD legs exact)                     | MATCH                                     |
| `f86826d3…` | **mint** KALE → G-recipient                  | ✅              | MATCH (11 mints, `from=None`)              | MATCH (recipient credited KALE, 0 debits) |
| `75664ee7…` | **burn**, bespoke (2-topic, no asset string) | ✅ → `Contract` | MATCH (`[burn, GDEOU7MS]`, LP-share token) | N/A\*                                     |
| `2f32199b…` | **burn** KALE                                | ✅              | MATCH                                      | MATCH (holder debited KALE)               |

\* Horizon **structurally cannot express a non-SAC token** — which _confirms_ our
`Contract → asset_id = None` scoping: the bespoke LP-share token has no classic
asset, so no asset-presence row (Horizon showed the surrounding withdraw crediting
classic AQUA/XTAR instead). Account participant IS still registered.

Also ran `parse_token_event` on the 5 real topic strings directly (temp Rust
test, removed after): all decode correctly — native/credit/bespoke asset +
transfer/mint/burn verbs + `from`/`to` positions.

## Net-new value — measured, not assumed

Answering "do these Soroban token moves add anything the op path lacks?":

- **Mint recipients are 100% non-invoker** — `1,932,765 / 1,932,765` mint events
  in a 3k-ledger window have `recipient != tx source`. Verified example: KALE mint
  to `GAJRN7UX…`, tx initiated by relayer `GDBCQRNJ…`. Mint is the dominant
  net-new verb (622k/10k), so **mint recipients are the account-side win** — they
  receive DeFi tokens without initiating the tx, so nothing else registers them.
- **Transfer/swap accounts are ~1 per tx** (98%: 19,681/20k txs have exactly one
  G-account), typically the invoker → already registered via `op.source`; the
  event contribution there is mostly redundant (harmless, RMT dedups).
- **Asset side** (all verbs): the moved classic asset (XLD/KALE/USDC/BLND/EURC…)
  is invisible to the op path for an `InvokeHostFunction` (opaque op body), so
  event-derived asset presence is genuinely net-new. This is the primary value,
  gated on the 0359 `operation_asset_appearances` deploy.

## Conclusion

Decoder is **byte-exact correct across all verbs + asset types**, confirmed by
independent ground-truth XDR + Horizon + our own parser on real prod data. The
task's net-new value is measured, not assumed: mint recipients (accounts) +
Soroban asset movements (assets). No bugs found.
