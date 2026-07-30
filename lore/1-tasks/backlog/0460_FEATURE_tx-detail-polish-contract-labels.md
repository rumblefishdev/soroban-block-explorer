---
id: '0460'
title: 'FEATURE: transaction-detail polish — contract labels + small UX debts from 0453'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0453', '0297']
tags:
  [frontend, backend, transaction-detail, ux, priority-medium, effort-medium]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0453: the deliberate deferrals and small debts that did
      not merit their own tasks, grouped so they do not rot as prose.
---

# FEATURE: transaction-detail polish

## Scope (each independently shippable)

1. **Contract display names** — stellar.expert shows `[Kale] CDL7…`; the
   name lives ON-LEDGER in instance storage `Symbol("METADATA")` (the
   off-ledger verdict in 0156/0283/0297 was chain-refuted — see project
   memory). Surface it in the invoke headline + Authorized calls tree.
2. **Picker mini-headlines** — rows currently repeat the type label; use
   the humanizeOp sentence (truncated) as the secondary line.
3. **Fee-bump fee breakdown** — SE shows non-refundable/refundable split;
   needs result-meta fee fields (may ride on 0457's enrichment).
4. **Pre-Protocol-23 note** — V3-meta txs have tx-level-only events; one
   quiet line in the events section instead of unexplained absence.
5. **Adaptive index** — the deferred layout option: hide the picker when a
   transaction has exactly one operation (87% of mainnet); revisit with
   Karol.
6. **Toggle-as-preference decision** — the wave-5 question parked for
   evidence: an Etherscan-style "always expand raw" preference; add the
   cheap click-counter telemetry first (product call).
7. **XdrRow / DisclosureRow unification** — when a third disclosure-style
   consumer appears, not before.

## Acceptance criteria

- [ ] Each item shipped or explicitly withdrawn with a recorded reason
