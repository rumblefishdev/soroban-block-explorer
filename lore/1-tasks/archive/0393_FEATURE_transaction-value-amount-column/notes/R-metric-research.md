---
prefix: R
title: How value-per-transaction is handled professionally
status: mature
---

# R — Value-per-transaction: protocol facts + industry practice

## Protocol: no per-transaction amount

- Horizon `Transaction` object: 25 attributes; the only money fields are
  `fee_charged` and `max_fee`. No amount/value/total/amount_sent.
- A transaction is a container of N operations. Amount is an **operation**
  attribute. Per the operations reference, 13 operation types carry a money
  field: create account (`starting_balance`), payment (`amount`), path payment
  strict send/receive (`send_amount`/`dest_min`, `send_max`/`dest_amount`),
  manage buy/sell offer, create passive sell offer, create claimable balance,
  clawback, LP deposit (`max_amount_a/b`), LP withdraw.
- Horizon endpoints reflect this: `/transactions` has no amount; `/operations`
  and `/payments` are operation-grained and carry amounts; `/effects` exposes
  `account_credited`/`account_debited`.

## How explorers show it (checked against live mainnet explorers)

| Explorer                                       | Amount in the transaction list?                                                                                                                                                                 |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reference / SDF-style explorer                 | No — operation count                                                                                                                                                                            |
| Analytics explorer (account/ledger scoped)     | No global tx list; amounts on operations                                                                                                                                                        |
| A scan-style explorer                          | No — fee only                                                                                                                                                                                   |
| A wallet/DEX explorer                          | No in the tx stream; a **separate payments stream** carries amount                                                                                                                              |
| One explorer that does show an "amount" column | It is an **operation feed** (the tx hash repeats once per operation); its per-transaction aggregate sums raw amounts across different assets and labels it "Mixed" — arithmetically meaningless |

The reference explorer (and the analytics platforms) deliberately show a
**list of transfers** on the transaction detail, not a single consolidated
number. A swap is two rows (asset out, asset in); a simple send is one row.

## Other chains

| Chain                                                                  | Per-tx amount? | Why                                                             |
| ---------------------------------------------------------------------- | -------------- | --------------------------------------------------------------- |
| Account/EVM chains                                                     | Yes            | The transaction has a single native `value` field               |
| UTXO chains                                                            | Yes            | Sum of outputs is well-defined                                  |
| Single-purpose-tx chains                                               | Yes            | One operation per transaction                                   |
| Multi-instruction chains (structurally like Stellar's multi-operation) | Yes            | They pick a convention: **net native-coin movement** for the tx |

The multi-instruction case is the relevant precedent: it shows a per-tx figure
by taking the **net native-coin** movement, not a cross-asset sum.

## Gross vs net (the professional distinction)

- **Gross transfer volume** = sum of every transfer. Double-counts routing.
  DeFi analytics platforms explicitly separate aggregator volume from
  underlying-DEX volume precisely to avoid counting one routed trade twice.
- **Net settled value** = value that actually changed ownership, offsetting
  pass-through/mutual flows. Traditional netting compresses gross to net
  materially (e.g. a netting settlement system compresses daily gross by ~78%).
- Address-level analytics compute **net flow** but explicitly aggregate it over
  an **address × time window**, not per single transaction ("a single large
  transaction can swing net flow") — i.e. netting is normally an address
  metric, not a per-transaction one.

## Conclusion feeding the task

No major explorer computes a **net-settled amount per transaction** as a list
column; the standard is to list transfers or show native value. Since the
product requires one aggregated value, **net settled value** (netting
pass-throughs) is the least-wrong single number — it matches how net flow is
defined, and avoids the gross double-count the industry warns against. The full
resolution and edge cases are in the S note.
