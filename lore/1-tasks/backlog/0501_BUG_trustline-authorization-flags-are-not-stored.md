---
id: '0501'
title: 'BUG: a frozen trustline renders as a normal balance — authorization flags are never stored'
type: BUG
status: backlog
related_adr: ['0055']
related_tasks: ['0463', '0331', '0492']
tags:
  [
    backend,
    clickhouse,
    xdr-parser,
    api,
    data-correctness,
    priority-medium,
    effort-small,
  ]
links: ['https://github.com/rumblefishdev/soroban-block-explorer/issues/377']
history:
  - date: '2026-08-17'
    status: backlog
    who: karolkow
    note: >
      Found while auditing account/holding state completeness for 0463. The
      account page shows a frozen holding exactly like a spendable one,
      because `balances` has no flags column at all. Same "display the whole
      truth" requirement that drives 0463.
---

# BUG: frozen holdings look spendable

## What is wrong

An asset issuer can revoke authorization on a holder's trustline. The holder
still _has_ the balance, but **cannot send, trade or redeem it** until the
issuer re-authorizes. It is the difference between money in your pocket and
money in a frozen bank account.

Our `balances` table has **no flags column**, so we cannot tell the two apart
and render both identically. Horizon exposes `is_authorized` /
`is_authorized_to_maintain_liabilities` on every balance; we have nothing to
expose.

Same defect class as the zero-vs-closed ambiguity ADR 0055 fixes: a plausible
value that is silently wrong is worse than a visible gap.

## Where the data is dropped

`TrustLineEntry.flags` is parsed and reaches the JSON, but the balance write
path never carries it into a column
(`crates/db-clickhouse/src/persist/stage.rs`, the trustline balance branch).
Extraction gap, not a data gap — the same shape as the signers gap in 0463.

Related and to decide together: task 0331 left the Soroban SAC
`BalanceValue.authorized` / `clawback` flags unpropagated for the same reason
("the frozen-balance policy is open"). One decision should cover both, so
classic and Soroban holdings do not disagree about what "frozen" means.

## Scope

- `balances` gains flag storage; follow ADR 0055's precedent —
  `ALTER … ADD COLUMN … DEFAULT` first, writer second, read last.
- Parser/persist carry `TrustLineEntry.flags` and the SAC `authorized` /
  `clawback` equivalents.
- Backward completeness: the checkpoint snapshot seed (0463) carries
  `TrustLineEntry` **including flags**, so this rides the same artifact if
  sequenced with it — otherwise it needs its own pass. Prefer riding along.
- API + UI: a frozen holding must be visibly marked. Never a silent normal row.
- Decide whether a frozen balance still counts toward `holder_count` /
  `total_supply` — and record the reasoning, because it changes published
  numbers.

## Acceptance criteria

- [ ] A holding on a revoked trustline is visibly distinguishable on the
      account page from a spendable one
- [ ] Classic and Soroban (SAC) holdings use one consistent notion of frozen
- [ ] Backward-complete via the 0463 snapshot seed, or its own measured pass
- [ ] The aggregate decision (`holder_count` / `total_supply`) is recorded,
      and any change to a published number is called out explicitly
- [ ] **Docs updated** — schema + read path + frontend contract
- [ ] **API types regenerated** — yes, the balance DTO gains fields
