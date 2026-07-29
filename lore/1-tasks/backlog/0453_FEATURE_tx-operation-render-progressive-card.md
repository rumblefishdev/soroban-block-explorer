---
id: '0453'
title: 'FEATURE: transaction operation render — one progressive card with a TRUE headline, replacing the normal/advanced split'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359', '0352', '0363', '0380', '0442', '0444']
tags:
  [
    frontend,
    transaction-detail,
    ux,
    priority-high,
    effort-large,
    layer-frontend-pages,
  ]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/370'
history:
  - date: '2026-07-29'
    status: backlog
    who: karolkow
    note: >
      Spawned from the render audit that has been sitting unowned in 0359's
      archive since 2026-07-08 (`notes/S-tx-render-audit.md`). That note ends
      with "spawn a separate FE/UX task on develop" and nothing ever did — so
      five of its eight findings had no owner, including the only Critical one
      and the structural one. Issue #370 reports the same thing from outside,
      independently. Created as the umbrella the loose bug tasks (0442, 0444)
      were being patched underneath.
---

# FEATURE: one progressive operation card

## Summary

The transaction-detail page renders each operation twice and badly: a "normal"
one-liner that is **factually wrong** for anything but a plain payment, and an
"advanced" dump of internal field names and raw stroops. Replace the binary with
**one card per operation** — a true human headline, the asset movement as key
facts, then progressive disclosure down to raw XDR.

The design work is already done. It is written up in
[`0359/notes/S-tx-render-audit.md`](../archive/0359_FEATURE_asset-participation-index-remodel/notes/S-tx-render-audit.md),
including a per-op-type headline spec for 16+ operation types and the field
mapping for each. **Read that note first — this task is its implementation, not
a re-design.**

## Why now

Two independent arrivals at the same conclusion.

**From outside (issue #370, 2026-07-28).** A user on the live deployment, on a
`CHANGE_TRUST` transaction:

> even in normal view (though I would call it more like Basic vs Detailed view)
> I'd still like to see what was the target for the change trust op. In this
> case, it was for an asset, VELO.

They also propose the wording we should show — "Change Trust processed for asset
VELO issued by …" — which is almost exactly the line the audit's spec
prescribes. And they name the framing problem in passing: `normal` / `advanced`
are the wrong two words for what should be one view with depth.

**From inside (the audit, 2026-07-08).** Live inspection of a
`PATH_PAYMENT_STRICT_SEND` self-swap — 1 XLM → TF → bubba across two pools —
against Horizon and stellar.expert.

| View           | What it shows                                                                                             |
| -------------- | --------------------------------------------------------------------------------------------------------- |
| ours, normal   | `Result: Sent 1 XLM to GAFB…36GD`                                                                         |
| ours, advanced | `claimedAtoms` / `destMin` / `poolIds` / `sendAmount: 10000000` — internal names, raw stroops, ScVal JSON |
| stellar.expert | `swapped 1 XLM → TF → X bubba` + a Trades section                                                         |

The headline is not merely thin, it is **untrue**: it reports only the send leg,
so a swap reads as a payment. Source equals destination here, so "to
GAFB…36GD" is meaningless on top of that. The received amount is not missing
from our data — it is in the last `claimedAtoms` entry, and the render does not
read it.

## The eight findings and who owns them

| #   | Finding                                                          | Severity     | Owner                                       |
| --- | ---------------------------------------------------------------- | ------------ | ------------------------------------------- |
| 1   | Normal one-liner is factually misleading for path-payments/swaps | **Critical** | **this task**                               |
| 2   | Organised around accounts, not around asset movement             | Major        | **this task**                               |
| 3   | Route / hops / pool crossings invisible                          | Major        | **this task**                               |
| 4   | Advanced is a raw dump of internal names + raw stroops           | Major        | 0363                                        |
| 5   | The normal↔advanced binary yields two mediocre views             | Major        | **this task**                               |
| 6   | Received amount discarded though present in `claimedAtoms`       | Major        | **this task** (0380 covers it only loosely) |
| 7   | Self-transfer not recognised → "to [same account]"               | Minor        | **this task**                               |
| 8   | Events table is raw ScVal JSON                                   | Minor        | 0363                                        |

## Scope

**This task owns** the operation card: headline, key facts, progressive
sections, and the removal of the mode toggle.

**It does not own** — keep these separate, they are independently shippable:

- **0363** — decoding ScVals to typed chips. This task decides _where_ the
  decoded events live (a section on the card); 0363 decides _how_ they render.
- **0380** — `u256`/`i256` decode fidelity, plus the `details.function_name`
  vs `functionName` key mismatch that stops `INVOKE_HOST_FUNCTION` from ever
  humanising.
- **0352** — the transaction-level fail-reason banner. Per-transaction, not
  per-operation.

## Ship first, on its own

Two slices are independently correct and do not need the redesign:

1. **Stop lying about swaps.** `normal/humanizeOp.ts:52-61` maps
   `PATH_PAYMENT_STRICT_*` to `sendAmount`/`sendAsset`, so line 111 emits
   "Sent {sendAmount} {sendAsset} to {dest}". At minimum: "Swapped {send} →
   {recv}", reading `recv` from the final `claimedAtoms` entry.
2. **Cover the operations that render nothing.** `humanizeOp` handles four op
   types — payment, path-payment, invoke, create-account. Everything else falls
   through to `"{opLabel} processed"`: offers, liquidity-pool deposit/withdraw,
   claimable balances, clawback, change-trust (the #370 case), account merge.
   The per-op sentences are already specified in the audit note.

## Shape

```
┌─ Operation 1 · Path Payment (Strict Send) · Classic ───────────┐
│  GAFB…36GD swapped  (self)                                     │
│     1 XLM   →   via TF   →   3,383,190.10 bubba                │
│                                                                │
│  Sent      1 XLM                                               │
│  Received  3,383,190.10 bubba                                  │
│  Route     XLM → TF → bubba      · 2 pools                     │
│  Min recv  3,206,685.74 bubba    (slippage bound)              │
│                                                                │
│  ▸ Trades (2)     ▸ Token events (6)     ▸ Raw XDR             │
└────────────────────────────────────────────────────────────────┘
```

Headline = a true sentence per op type. Key facts = the asset movement first.
Progressive sections absorb everything "advanced" shows today, without a mode.

## Open decisions this task must settle

- **0442** — the flow tree reads six `details` keys no backend module emits, so
  nested contract calls never render. Implement the missing contract, or delete
  the dead branches? The answer depends on whether the flow tree survives this
  redesign at all. **Decide here, not in 0442.**
- **0444** — the "Result" node is hardcoded green and shows a description rather
  than the verdict. A fix was written and reverted deliberately: patching one
  node of a tree that may not survive is a plaster.
- **Naming.** #370 suggests "Basic / Detailed". If the toggle disappears the
  question is moot — but if any split remains, take the reporter's words.

## What is NOT blocked on the backend

The audit checked this explicitly: advanced already holds the full parsed
`details` for most op types. This is **humanisation of data we already fetch**,
not new parsing.

One real exception: `claim_claimable_balance` / `clawback_claimable_balance`
carry only a `balanceId` in the operation body — the asset needs same-op
`LedgerEntryChanges`. stellar.expert has the same gap and also shows only the
id. Render "claimed balance {id}" until that lands; do not block on it.

## Acceptance criteria

- [ ] Every op type renders a headline that is **true** — no swap reported as a
      payment, no "processed" placeholder
- [ ] `CHANGE_TRUST` names the asset and its issuer (the #370 case)
- [ ] Received amount and route shown for both path-payment directions
- [ ] Self-transfer (source == destination) recognised in the wording
- [ ] Amounts formatted, never raw stroops; US number grouping preserved
- [ ] The normal/advanced toggle is gone, and nothing it used to show is lost
- [ ] 0442 and 0444 explicitly resolved — implemented, folded in, or withdrawn
      with the reason recorded
- [ ] **Docs updated** — frontend data contracts under `docs/architecture/**`
      per ADR 0032, if the render's field consumption changes
- [ ] **API types regenerated** — `N/A` unless the backend contract changes;
      state which when closing

## Notes

The audit also retracted an earlier suspicion worth not re-opening: transaction
**status renders correctly** — our "Failed" matched Horizon's
`successful: false` and stellar.expert on tx `d8b4bab5`. There is no status bug.
