---
id: '0444'
title: 'BUG: operation flow-tree "Result" node is hardcoded green and shows a description, not the tx verdict'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0352', '0380', '0453']
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
  - date: '2026-07-29'
    status: backlog
    who: karolkow
    note: >
      Implemented, then **reverted before it reached production** — deliberately,
      not because it was broken. Driving the node's colour and title off
      `tx.successful` does stop the node lying, and the tests passed, but it
      treats the symptom: the node claims to be a per-OPERATION result while
      the only verdict we can source is per-TRANSACTION, because the API
      exposes no per-operation result codes (0352 Step 6). The version that was
      reverted papered over that with wording ("Transaction failed — this
      operation was not applied"), which is true under Stellar's atomicity but
      still answers a question the reader did not ask.
      Reverted to work the real shape out first rather than ship the patch and
      lose the appetite for it. Whoever picks this up: decide whether the node
      is per-operation (needs the backend half of 0352) or is honestly relabelled
      as a transaction-level verdict, and note that this component sits inside
      a view built against an API contract that never landed (0442) — see the
      redesign spec in
      `lore/1-tasks/archive/0359_.../notes/S-tx-render-audit.md`, whose finding
      #5 is precisely "the normal/advanced binary gives two mediocre views".
      The reverted work is recoverable from commit `d5444023` if any of it is
      worth keeping.
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
