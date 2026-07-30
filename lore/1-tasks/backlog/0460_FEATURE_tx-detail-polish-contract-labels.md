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
8. **Failed-transaction banner redesign** (Karol, post-ship review): the red
   box is the only custom alert on the page and the theme has no MuiAlert
   style to match — replace with a thin status strip under the Summary title
   or add a house Alert to libs/ui; keep the "not applied" chip (already a
   themed Chip).
9. **Story-chip placement**: next to the Success chip it reads as a second
   status — move next to the page title or the Operations header.
10. **Picker width**: the 50/50 grid gives the index half the page; try
    5/7 or 4/8 (pairs with item 5, adaptive index).
11. **Clickable values in headlines/facts**: sentences are plain strings, so
    assets/accounts in them are dead text — needs ReactNode sentences;
    coordinate with 0456 (typed details → componentised templates).
12. **Route-strip edge labels**: hop amounts are the pools' actual payouts
    while the headline shows the min/max bound — label edges (e.g. "actual")
    so the two numbers read as different facts, not a mismatch.

## Acceptance criteria

- [ ] Each item shipped or explicitly withdrawn with a recorded reason
