---
id: '0444'
title: 'BUG: operation flow-tree "Result" node is hardcoded green and shows a description, not the tx verdict'
type: BUG
status: active
related_adr: []
related_tasks: ['0352', '0380']
tags:
  [frontend, transaction-detail, ux, correctness, priority-medium, effort-small]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/363'
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/364'
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Spawned from external feedback on the live deployment. Reporter opened a
      failed transaction and read the green "Result" node as a success verdict,
      then asked why the transaction was nonetheless labelled failed. Confirmed
      in code: the node's colour never depends on `tx.successful`. Not caught by
      the 0257 or 0359 frontend audits — both reviewed the node's *text*, not
      its colour.
  - date: '2026-07-28'
    status: active
    who: karolkow
    note: >
      Implemented. `kind` and the title now both derive from `tx.successful`:
      `Result · Success` on green, `Result · Failed` on a red palette that
      mirrors the green one exactly. Colour is never the only signal — the
      verdict is in words, which is what #370 asked for literally.
      **The verdict is the transaction's, not the operation's**, because
      per-operation result codes are not exposed by the API (0352 Step 6). That
      is defensible rather than a fudge: Stellar applies a transaction
      atomically, so on failure no operation took effect — the failed node says
      "Transaction failed — this operation was not applied" above the existing
      description. It does mean we cannot yet say WHICH operation was at fault;
      that needs 0352's backend half.
      New test file `toFlowNodes.test.tsx` (none existed — the same coverage gap
      that let 0380's key mismatch survive a full frontend audit); verified it
      fails with the fix removed. web 123 green, ui 76 green.
      Serves both #364 and #370. Closes neither: #364 also wants the failure
      REASON in the normal view (0352), #370 also wants operation targets such
      as "for asset VELO issued by …" (a `humanizeOp` branch for classic ops,
      still unscoped).
---

# BUG: flow-tree "Result" node is always green

## Summary

The last node of the operation flow tree on the transaction detail page is
titled **"Result"**, is painted green unconditionally, and carries a
_description of what the operation did_ rather than whether it succeeded. On a
failed transaction it still renders green, directly contradicting the `Failed`
chip in the summary header a few hundred pixels above.

## Root cause

Two independent halves, both confirmed:

- **Colour.** `libs/ui/src/visualization/OperationFlowTree.tsx:73-77` —
  `case 'result'` returns `scales.green[950]` / `green[600]` with no input other
  than the node kind. `tx.successful` is not in scope in that file at all.
- **Content.** `web/src/pages/transaction-detail/normal/toFlowNodes.tsx:139-144`
  sets the node's summary from `buildResultSummary(...)` falling back to
  `humanizeOp(...)` — an operation description ("Invoked contract C…"), never a
  verdict.

The summary-header chip is correct and already carries text
(`libs/ui/src/components/StatusChip.tsx:14` renders `Success` / `Failed`), so
the page contradicts itself.

## Reproduction

Any failed transaction, e.g.
`de6aa93104f21a6e18f2d104c3418974edc3fecc925932feb254144d6bd5f5ce`
(`txFEE_BUMP_INNER_FAILED` → inner `txFAILED` → `INVOKE_HOST_FUNCTION` →
`TRAPPED`, confirmed by decoding the result XDR; Horizon agrees
`successful: false`). Summary chip reads `Failed`; the flow tree's "Result" node
is green.

## Fix

Drive the node's palette and label from the transaction verdict:

- Add a `resultStatus: 'success' | 'failed'` (or reuse the existing kind with a
  variant) so `nodeStyle` can return the error palette.
- Prefix the node with the verdict — `Failed · <description>` — so the text is
  self-sufficient without colour. Feedback explicitly asked for the word, not
  just the hue: colour alone also fails users who cannot distinguish it.
- Keep the description; it is useful, just not a verdict.

Related but separate: task 0352 surfaces _why_ a transaction failed (the
`ScError` behind it). This task only makes the existing verdict honest.

## Acceptance criteria

- [ ] "Result" node palette derives from `tx.successful`, not from node kind
- [ ] Node text names the verdict explicitly (not colour-only)
- [ ] Verified against a known-failed and a known-successful transaction
- [ ] Unit test covering the failed-transaction branch
- [ ] **Docs updated** — N/A, presentation-only, no change to system shape
- [ ] **API types regenerated** — N/A, frontend only
