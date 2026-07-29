---
title: 'UX regression pass — the eight audit findings re-checked on the shipped card'
type: synthesis
status: mature
spawned_from: ../README.md
spawns: []
tags: [frontend, transaction-detail, ux, ux-expert, regression]
links: []
history:
  - date: '2026-07-29'
    status: mature
    who: karolkow
    note: >
      Umbrella acceptance criterion: the same audit that produced the eight
      findings (0359/notes/S-tx-render-audit.md), re-run against the shipped
      result after waves 0–5. Verified live on a dev server against mainnet
      transactions (VELO arbitrage, KALE invoke, LOW_RESERVE failure).
---

# UX regression pass (wave 5)

Method: each original finding re-checked against the shipped render; then the
orphan sweep (the 0348/0351 lesson — a fix that leaves a control pointing at
something that no longer exists).

## The eight findings

| #   | Original finding                                | Verdict now                                                                                                                                                                                                                                                               |
| --- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Normal one-liner factually misleading for swaps | **Closed.** All 27 op types render a truthful sentence; a swap can never read as a payment (strict-send shows exact sent + bound, strict-receive exact delivered).                                                                                                        |
| 2   | Organised around accounts, not asset movement   | **Closed.** Headline leads with the asset movement; route strip sits above the facts; accounts are inline links, not the skeleton.                                                                                                                                        |
| 3   | Route / hops / pools invisible                  | **Closed** for the route chain (strip with per-hop amounts from `claimedAtoms`, honest `partial` flag for order-book segments). Pool _links_ stay 0305's, boundary held.                                                                                                  |
| 4   | Advanced = raw dump                             | **Contained, owner unchanged.** The dump is now a per-card "Operation details" disclosure, collapsed by default, labeled raw. Typed ScVal rendering remains 0363 — deliberately not absorbed here.                                                                        |
| 5   | normal↔advanced binary                          | **Closed.** Toggle removed; one page, per-section disclosures; old `?mode=` URLs are ignored gracefully.                                                                                                                                                                  |
| 6   | Received amount discarded                       | **Resolved by design (D9).** The exact strict-send delivered amount is not honestly derivable from LP-only atoms; the card ships a deliberately empty "Received —" slot that lights up when the net_settled read path lands. Strict-receive shows the exact amount today. |
| 7   | Self-transfer unrecognised                      | **Closed**, including the inherited-source case caught live (op-level source null → falls back to tx source).                                                                                                                                                             |
| 8   | Events table raw ScVal JSON                     | **Contained, owner unchanged.** Per-op events show as name chips in the card (`transfer`, `mint` via `op_index`); the full table is collapsed by default; humanised rendering is 0363.                                                                                    |

## Orphan sweep

- `?mode=advanced` in old links: ignored, page renders the one view — no dead
  control, no crash.
- Deep links `#op-N`: work; invalid index falls back to the first operation.
- Removed components (`ModeToggle`, `useDetailMode`, `OperationFlowTree`,
  both mode panels, `toFlowNodes`): no remaining importers; `libs/ui` exports
  cleaned; files in `.trash/0453-wave3/`.
- Picker: dead type filter removed in wave 0; rows now carry per-type icons.
  Residual cosmetic: the row sub-label duplicates the type label (it used to
  host the never-populated `subtype`) — harmless, candidate for a mini-headline
  in a future slice.
- 0305 boundary: no pool links were added anywhere in the card — intact.

## Residual risks / follow-ups (all owned)

- ScVal walls now live only behind disclosures; the real fix is 0363.
- Full failure REASON (decoded per-op codes + Soroban errors) is 0352; today
  the banner shows atomicity wording + raw `result_code`.
- `op_index` / claim-CB asset reach production responses only after the next
  backend deploy; the frontend is absence-safe (tested).
- Balance-changes section parked behind the net_settled read path (D13).
