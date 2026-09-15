---
id: '0552'
title: 'FEATURE: transaction detail names the pool behind a liquidity movement — pool, both legs, position owner, price range'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0540', '0542', '0543', '0516', '0374']
tags: [priority-low, effort-medium, layer-frontend, layer-api, liquidity-pools]
links:
  - web/src/pages/transaction-detail/sections/EventsSection.tsx
  - web/src/pages/transaction-detail/op-card/OperationCard.tsx
history:
  - date: 2026-09-14
    status: backlog
    who: karolkow
    note: >
      Filed from the 0540 value-flow map (ticket T13, raised 2026-09-09), option
      2 of three. The account page's Balance change cell shows how much of what
      moved, never where it went; a two-sided liquidity deposit reads the same as
      money leaving for good. Naming the pool on the transaction detail needs no
      table and no backfill.
---

# Transaction detail names the pool behind a liquidity movement

## Summary

The `Balance change` cell (0540) answers "how much of what" and never "to whom".
When an account deposits two assets into an automated market maker in one
transaction, the cell correctly shows two outgoing legs — and nothing tells the
reader the legs went to the same place, that the place was a pool, or what the
account got for them. Add a section to the transaction detail that says so: the
pool, both legs, the position's owner and its price range.

## Context

Raised 2026-09-09 from a real account: a bridge minted a classic credit asset to
it, and thirteen thousand ledgers later the account sent that asset and a second
one to a concentrated-liquidity pool in one transaction.

What the data says:

- **The parties are in the payload, not the topics.** The pool's issuance event
  carries a single topic, but its data map holds `sender` (the depositing
  account), `owner` (the position-manager contract), both leg amounts — equal to
  the two transfers to the unit — the liquidity minted, and the tick pair. Across
  that pool's history all 45 swaps and all 6 issuances name their parties in the
  data and none in the topics.
- **Nothing came back to the account, and that is true.** `owner` is the
  position manager, so the account's balances did lose both assets; what it holds
  afterwards is a claim on another contract, which the chain never expresses as a
  transfer. The section must not show the account as receiving a position.
- **These events are correct rejects for the balance index.** 0542's reject
  classification (2026-09-13) measured 3 698 such position events from 129
  emitters and found none moved a balance. Teaching the decoder to store them
  (the map's option 3) would add nothing to the account's row — so the gain is
  here, at read time.
- **The pool may be unknown to us.** The example emits `swap`, `mint` and `init`
  but is not in the pool registry, so until 0516 / 0374 cover it the section
  names it by its `C…` address.

Options considered on the map: (1) leave the column alone — correct and
illegible; (2) this task; (3) decode the payload into rows — belongs to 0542 and
changes nothing on the account page. The task owner leaned to (2) on 2026-09-09
and chose it 2026-09-14.

## Implementation

- Read side only: the transaction detail already decodes events at read time.
  Recognise a liquidity issuance / withdrawal event by payload shape (a map with
  both leg amounts and an `owner`), not by contract name.
- Render: pool (registry name when known, else `C…`), both legs matched to the
  transfer rows they equal, position owner, price range from the tick pair.
- A leg that does not match a transfer in the same transaction is shown as the
  event states it, and the mismatch is visible — never silently reconciled.
- Before starting, check that 0516's pool model does not already plan this
  surface; if it does, fold this task into it.

## Acceptance Criteria

- [ ] The 2026-09-09 deposit transaction shows the pool, both legs, the position
      owner and the price range
- [ ] The depositing account is never shown as receiving the position
- [ ] An unregistered pool renders by its `C…` address, linked to its contract page
- [ ] Recognition is by payload shape, verified on at least three pool families
- [ ] No new table, column or backfill
- [ ] **Docs updated** — `docs/architecture/frontend/**` (transaction detail
      sections); `database-schema/**` N/A — no schema change
