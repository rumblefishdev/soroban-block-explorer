---
id: '0484'
title: 'BUG: search asset hits are indistinguishable — the issuer is on the wire but never shown'
type: BUG
status: backlog
related_adr: ['0051']
related_tasks: ['0485', '0472', '0318']
tags: [frontend, search, assets, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/368'
history:
  - date: '2026-08-13'
    status: backlog
    who: karolkow
    note: >
      Found while verifying 0472's work on a local API against production data.
      Split out of 0485 (search ranking): ranking picks the best FIRST hit,
      this task is about telling the others apart. Frontend-only — the issuer
      already ships inside `route_token`; we just drop it on the floor.
---

# BUG: two identical rows, two different assets

## What the user sees

Global search (`⌘K` bar / `/search?q=…`), **Token** tab, query `USDC`:

```
AEUSDC        classic_credit
BONDSUSDC     classic_credit
BUSDC         classic_credit
BUSDC         classic_credit     ← same text, different asset
ULTRAUSDC     classic_credit
USBUSDC       classic_credit
```

Two rows read `BUSDC`. What is actually behind them:

```
BUSDC-GA57KXF2UAGURRYK4LKTO4BO2JASL2PQCO2C53XUNK5G4CXX6QAL6QNR
bUSDC-GA57KXF2UAGURRYK4LKTO4BO2JASL2PQCO2C53XUNK5G4CXX6QAL6QNR
```

Same issuer, codes differing only in letter case — and on Stellar those are two
DIFFERENT assets. The list renders neither difference.

At the extreme: **441 assets carry the code `USDC`** (measured on prod), each
from a different issuer, because anyone may mint any code. One of them is
Circle's (613,691 holders); the rest range from experiments to look-alikes.

## Why it happens

A hit renders `identifier` (the asset code) and `label` (the type). The DTO doc
says outright that the code is "not unique / not routable". The row component
shows exactly those two fields.

## The part that makes this cheap

**The issuer is already in the response.** `route_token` carries the canonical
`CODE-ISSUER` pair, because the frontend needs it to build the link:

```json
{
  "identifier": "BUSDC",
  "label": "classic_credit",
  "route_token": "BUSDC-GA57KXF2UAGURRYK4LKTO4BO2JASL2PQCO2C53XUNK5G4CXX6QAL6QNR"
}
```

`SearchResultsView` uses it for the href and `SearchResultRow` renders only the
other two fields. So the fix is display, not plumbing — **no API change, no
api-types regen**.

## Options

| #     | Approach                                    | API change | What it buys                               |
| ----- | ------------------------------------------- | ---------- | ------------------------------------------ |
| **1** | Show the issuer parsed out of `route_token` | **no**     | tells the rows apart immediately           |
| 2     | Add the TOML home domain (`centre.io`)      | yes        | the clearest "this is the real one" signal |
| 3     | Add holder count                            | yes        | makes the ranking visible instead of magic |
| 4     | Rely on ranking alone (0482)                | —          | first hit right, the rest a guess          |

Option 1 is this task. Options 2–3 sit on top of it and belong with the search
backend work in [[0485]].

## Watch out

`route_token` is not always a pair — the three shapes are the same three
`assetDisplayCode` already handles:

| asset class    | `route_token`        | show               |
| -------------- | -------------------- | ------------------ |
| classic credit | `CODE-ISSUER`        | truncated issuer   |
| Soroban token  | `C…` contract StrKey | truncated contract |
| native         | `native`             | nothing            |

Do not split on `-` blindly: an asset code may itself contain characters that
make a naive split wrong, and the Soroban/native forms have no `-` at all.

## Acceptance criteria

- [ ] A classic asset hit shows its issuer; two same-code assets are visibly different
- [ ] Soroban hits show the contract, native shows neither — no empty separator
- [ ] No API change, no api-types regen
- [ ] vitest cases for all three shapes
- [ ] Docs: frontend-overview search section
