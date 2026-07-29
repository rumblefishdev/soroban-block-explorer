---
id: '0453'
title: 'FEATURE: transaction operation render — one progressive card with a TRUE headline, replacing the normal/advanced split'
type: FEATURE
status: active
related_adr: []
related_tasks:
  [
    '0359',
    '0070',
    '0071',
    '0305',
    '0352',
    '0363',
    '0380',
    '0411',
    '0442',
    '0444',
  ]
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
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/364'
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
  - date: '2026-07-29'
    status: backlog
    who: karolkow
    note: >
      Coverage re-checked on review rather than asserted, and it was not
      complete. Added: 0305 (pool_ids links) which owns part of finding #3 and
      was silently double-claimed; 0411 (net-settled on the same page); issue
      #364, an OPEN report of the 0444 bug; 0070/0071 as the superseded
      originals; the 0257 audit's never-implemented per-op icon spec; and the
      lineage — 0359's spawn plan had already scoped this as sibling #6 and
      parked it pending "a real traffic/support signal", which #370 now is.
  - date: '2026-07-29'
    status: backlog
    who: karolkow
    note: >
      Restored the `/ux-expert` provenance. The eight findings were copied in
      as if they were prose observations — unsourced, and stripped of the
      Principle column that justifies each severity, which is the whole reason
      the ordering holds. Also pointed at 0363's separate `/ux-expert` run
      (it already answers findings #4 and #8) and added a regression pass on
      the shipped card as an acceptance criterion.
  - date: '2026-07-29'
    status: active
    who: karolkow
    note: >
      Promoted to active after a full-context analysis session (code map,
      per-op-type details inventory, satellite boundaries). Key corrections to
      carry into the redesign: received amount is NOT generally derivable from
      claimedAtoms (LP-only filter, absent on failure; result-side
      SimplePaymentResult never extracted — partially covered by the net_settled
      work, incomplete, do not design against it yet); events are tx-scoped
      only (per-op index discarded at parse); heavy.operation_tree and
      heavy.result_code arrive unread — a third option for 0442 and a partial
      answer to #364. Redesign phase next: live benchmark + /ux-expert.
  - date: '2026-07-29'
    status: active
    who: karolkow
    note: >
      From-zero design pass finished: live benchmark (stellar.expert,
      stellarchain, Solscan), three interactive prototypes, UX literature +
      deep dive (SE MIT templates, Blockscout interpretation, Tenderly trace).
      Layout settled with Karol: one card, master-detail frame, index always
      visible; toggle survives until parity. Converted to directory; governing
      spec with 15 numbered decisions and 7 shippable waves in
      notes/G-implementation-plan.md — it supersedes the audit's render
      sketch. Starting wave 0 (dead code + functionName fix).
---

# FEATURE: one progressive operation card

## Summary

The transaction-detail page renders each operation twice and badly: a "normal"
one-liner that is **factually wrong** for anything but a plain payment, and an
"advanced" dump of internal field names and raw stroops. Replace the binary with
**one card per operation** — a true human headline, the asset movement as key
facts, then progressive disclosure down to raw XDR.

> **Governing spec: [`notes/G-implementation-plan.md`](notes/G-implementation-plan.md)**
> (2026-07-29, after the from-zero design pass). The audit below remains the
> finding record; where the two disagree on the solution, the G-note wins.

The original design write-up is in
[`0359/notes/S-tx-render-audit.md`](../../archive/0359_FEATURE_asset-participation-index-remodel/notes/S-tx-render-audit.md),
including a per-op-type headline spec for 16+ operation types and the field
mapping for each.

## Where this came from

It was **planned and never spawned**. 0359's spawn plan
([`notes/G-spawn-plan.md`](../../archive/0359_FEATURE_asset-participation-index-remodel/notes/G-spawn-plan.md),
sibling **#6 "FE — transaction render"**) already scoped it, already flagged the
`humanizeOp` mislabel as a standalone quick fix, and already established that it
is independent of 0359's data work except the claim-CB headline. It then closed
with the reason it stalled:

> Priority/effort here are **provisional** — confirm against a real
> traffic/support signal before locking the order.

Issue #370 is that signal. The provisional flag is discharged.

The two views themselves were built by **0070** (normal) and **0071**
(advanced), both archived. Neither is a current spec — this task replaces their
output, so do not read them as the target state.

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

These are a **`/ux-expert` audit**, not an opinion — run against the live render
on 2026-07-08 and recorded in the audit note. Each severity is anchored to a
principle, which is why the ordering is not negotiable by taste: #1 outranks
everything because a summary must be true before it can be useful.

| #   | Finding                                                          | Severity     | Principle behind it                                              | Owner                                       |
| --- | ---------------------------------------------------------------- | ------------ | ---------------------------------------------------------------- | ------------------------------------------- |
| 1   | Normal one-liner is factually misleading for path-payments/swaps | **Critical** | A summary must be TRUE first; a wrong summary is worse than none | **this task**                               |
| 2   | Organised around accounts, not around asset movement             | Major        | Group by the user's mental model, not by the data structure      | **this task**                               |
| 3   | Route / hops / pool crossings invisible                          | Major        | Show the thing the operation is actually about                   | **this task** (+ 0305 for the pool links)   |
| 4   | Advanced is a raw dump of internal names + raw stroops           | Major        | Even "raw" should format amounts and use human labels            | 0363                                        |
| 5   | The normal↔advanced binary yields two mediocre views             | Major        | Summary first, details on demand (Shneiderman)                   | **this task**                               |
| 6   | Received amount discarded though present in `claimedAtoms`       | Major        | Use the data you already have                                    | **this task** (0380 covers it only loosely) |
| 7   | Self-transfer not recognised → "to [same account]"               | Minor        | Recognise the special cases                                      | **this task**                               |
| 8   | Events table is raw ScVal JSON                                   | Minor        | Humanise; encode direction with sign and colour                  | 0363                                        |

**0363 carries a second, independent `/ux-expert` run** — its
`## UX Expert Analysis` section audits `EventsSection`, `OperationJsonDetail`,
`HighlightedJson` and `XdrRow` against the decoder and gives a per-option
verdict. Read it before designing the card's events section; it is the answer to
findings #4 and #8, already worked out.

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
- **0305** — rendering `OperationItem.pool_ids` as `L…` links. It is the
  shippable slice of finding #3: this task owns the **route chain** (which
  assets, in what order), 0305 owns making the **pools it crossed clickable**.
  The API already carries the field and the frontend ignores it entirely, so
  0305 lands without waiting for the card.
- **0411** — the per-asset "Net settled" breakdown on the same detail page.
  Transaction-level, above the operations. Named here only so two tasks do not
  independently redesign one page.

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
- **0444 / issue #364** — the "Result" node is hardcoded green and shows a
  description rather than the verdict, so a failed transaction reads as
  successful. That is the user-visible face of this task: #364 is open, is a
  reported correctness bug, and is closed by whatever this task decides. A fix
  was written and reverted deliberately — patching one node of a tree that may
  not survive is a plaster.
- **Per-op-type icons.** The 0257 frontend audit records the spec ("operation
  type → icon mapping consistent", task README 1.15) and confirms it was never
  implemented — `categoryChip` returns a coloured chip, not an icon
  (`0257/findings/AN-stellar-domain.md:59`). The card is where an icon would
  live. Adopt or drop it explicitly; do not leave it as a third orphan spec.
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

- [x] Every op type renders a headline that is **true** — no swap reported as a
      payment, no "processed" placeholder (all 27 types, wave 1)
- [x] `CHANGE_TRUST` names the asset and its issuer (the #370 case)
- [ ] Received amount and route shown for both path-payment directions —
      route: both; received: exact for strict-receive, **deliberately an
      honest empty slot for strict-send** (spec D9: not derivable from
      LP-only `claimedAtoms`; lights up when the net_settled read path lands)
- [x] Self-transfer (source == destination) recognised in the wording —
      including ops inheriting the tx source
- [x] Amounts formatted, never raw stroops; US number grouping preserved
      (raw numbers remain only inside the explicitly-raw details disclosure)
- [x] The normal/advanced toggle is gone, and nothing it used to show is lost
      (per-card details disclosure + always-rendered Events/Raw sections,
      collapsed; old `?mode=` links degrade gracefully)
- [x] A failed transaction never reads as successful (issue #364) — summary
      banner + result_code + dimmed "not applied" cards
- [x] 0442, 0444 and the per-op icon spec explicitly resolved — 0442 closed
      (dead branches deleted, real tree from `operation_tree`), 0444 closed
      (verdict moved to the banner), icons **implemented** (0257 spec adopted)
- [x] The 0305 boundary held: route assets here, pool links stay in 0305
- [x] **`/ux-expert` regression pass on the shipped card** —
      [notes/S-ux-regression-pass.md](notes/S-ux-regression-pass.md)
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md`
      §6.4 rewritten for the one-view contract
- [x] **API types regenerated** — with the `op_index` DTO change, same commit
      (freshness gate satisfied)

## Notes

The audit also retracted an earlier suspicion worth not re-opening: transaction
**status renders correctly** — our "Failed" matched Horizon's
`successful: false` and stellar.expert on tx `d8b4bab5`. There is no status bug.
