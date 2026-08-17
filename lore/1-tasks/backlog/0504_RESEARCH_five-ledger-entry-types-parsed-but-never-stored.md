---
id: '0504'
title: 'RESEARCH: five ledger entry types are parsed, labelled, and thrown away — offers, claimable balances, data, TTL, config'
type: RESEARCH
status: backlog
related_adr: []
related_tasks: ['0463', '0501', '0502', '0503', '0331']
tags:
  [
    backend,
    xdr-parser,
    clickhouse,
    completeness,
    priority-medium,
    effort-medium,
  ]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found while hunting for capability gaps after the checkpoint-snapshot
      discovery. The parser walks all ten LedgerEntryType variants and names
      them, but only five ever reach a table. The measured activity behind the
      missing five is very large — the classic DEX alone is ~102M operations
      per two months — so this is a scoping question, not a cleanup.
---

# RESEARCH: what we parse and then discard

## The finding

`extract_single_change` handles all ten `LedgerEntryType` variants and gives
each a label. Only five become rows: `Account`, `Trustline`, `ContractData`,
`ContractCode`, `LiquidityPool`. The other five are decoded, named, and
dropped — there is **no table** for any of them (`grep` over
`schema/init.sql`: no `offers`, no `claimable_balances`, no `account_data`,
no `ttl`, no `config_settings`).

Measured activity, prod, ledgers > 63,000,000 (~2 months):

| Missing entry          | Driving operations                                                     | Ops in ~2 months   |
| ---------------------- | ---------------------------------------------------------------------- | ------------------ |
| **`Offer`**            | ManageSellOffer (3) + ManageBuyOffer (12) + CreatePassiveSellOffer (4) | **~102,477,000**   |
| **`ClaimableBalance`** | Create (14) + Claim (15) + Clawback (20)                               | **~1,715,000**     |
| `Data`                 | ManageData (10)                                                        | 66,479             |
| `Ttl`                  | Soroban archival                                                       | — (state, not ops) |
| `ConfigSetting`        | network config                                                         | —                  |

The classic DEX is the single largest activity on the network and we hold
**none** of its state.

## Why this is not just "a missing feature"

Three of these are **holdings-adjacent** — they change what an account
actually has, which is the exact subject of issue #377:

1. **Open offers lock funds.** An account with an open sell offer cannot spend
   the offered amount. `AccountEntry` and `TrustLineEntry` both carry
   `Liabilities { buying, selling }` for precisely this, and **we store
   neither** (`grep liabilities` over the parser and persist: nothing). So our
   balance figure does not distinguish spendable from committed — the same
   defect family as frozen trustlines (task 0501) and zero-vs-closed
   (ADR 0055).
2. **A claimable balance is value addressed to an account.** ~1M created and
   ~620k claimed in two months, and none of it appears on our account page.
   Task 0331 already flagged claimable balances as an uncounted supply venue.
3. **`Ttl` is Soroban archival state** — the same archived-but-restorable
   condition that may make type-3 holdings over-report (open question in the
   0463 map).

## What to answer

- **Scope, per entry type**: index it, deliberately skip it, or defer with a
  trigger. Record the reason either way — "we never got to it" is what
  produced this finding.
- **Liabilities first?** They are the cheapest of the three: fields already in
  entries we ALREADY parse, no new entry type, and they make an existing
  number honest rather than adding a new page.
- **Cost per entry type.** Offers are the big one: ~102M ops per two months
  implies very high row churn, so measure before promising a table.
- **Does the checkpoint snapshot (task 0502) cover them?** It is full ledger
  state, so it should carry offers, claimable balances and data entries — if
  so, backward completeness for whichever we choose rides the same artifact.
- **What does the account page owe the reader?** Deciding "we do not show
  offers" is legitimate; showing a balance that silently includes committed
  funds is not.

## Done means

A per-entry-type verdict with its measured cost, tasks filed for whatever is
in scope, and the deliberate skips written down so the next person does not
rediscover this by accident.
