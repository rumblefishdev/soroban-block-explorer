---
id: '0442'
title: 'BUG: tx flow tree reads six `details` fields no backend module emits — nested contract calls never render'
type: BUG
status: backlog
related_adr: []
related_tasks: ['0380', '0359']
tags:
  [
    frontend,
    transaction-detail,
    dead-code,
    contract-invocations,
    priority-medium,
    effort-medium,
  ]
links: []
history:
  - date: '2026-07-27'
    status: backlog
    who: karolkow
    note: >
      Found while investigating external feedback that contract invocations
      render as "Invoked contract C…". Task 0380 already documents the
      `function_name` / `functionName` key mismatch; this is the wider finding
      behind it — the flow tree was written against an API contract that was
      never implemented, so its entire nested-call branch is unreachable.
  - date: '2026-07-28'
    status: backlog
    who: karolkow
    note: >
      Corrected: **six dead fields, not five.** `humanizeOp.ts:72-79` reads a
      `details.summary` no crate emits, and it is consulted before everything
      else — the early return that never fires is the only reason the
      `INVOKE_HOST_FUNCTION` branch under it is reachable. Missed on the first
      pass because the audit grepped `toFlowNodes.tsx` only. File renamed from
      `…five-fields…` to match. Nothing else about the task changes; the
      implement-vs-delete decision still comes first.
---

# BUG: flow tree renders against fields the backend never emits

## Summary

The default transaction view reads six keys out of the heavy `details` payload
that **no crate produces** — five in `toFlowNodes.tsx`, one in `humanizeOp.ts`. The nested-call branch is
therefore dead: a contract calling a contract calling a contract renders as a
single flat node, and the "Result" summary lines never appear.

## The six fields

Repo-wide grep (excluding `node_modules`, `target`, `dist`) finds these only in
the frontend and in one archived audit note — never in `crates/`:

| Field                 | Read at                      | Emitted by |
| --------------------- | ---------------------------- | ---------- |
| `contract_label`      | `toFlowNodes.tsx:67`, `:154` | nobody     |
| `summary_line_1`      | `toFlowNodes.tsx:100`        | nobody     |
| `summary_line_2`      | `toFlowNodes.tsx:101`        | nobody     |
| `invocations`         | `toFlowNodes.tsx:84`, `:157` | nobody     |
| `destination_summary` | `toFlowNodes.tsx:69`         | nobody     |
| `summary`             | `humanizeOp.ts:72-79`        | nobody     |

The last one was missed when this task was first written, which said "five".
`summaryFromHeavy` reads `details.summary` and is consulted **first** in
`humanizeOp`, returning early when it hits. Because nothing ever emits the key
it always returns `null` — which is the only reason the `INVOKE_HOST_FUNCTION`
branch below it is reachable at all. Harmless today, load-bearing by accident:
anything that starts emitting `summary` silently overrides every humanised
string in the default view.

`function_name` (`toFlowNodes.tsx:66`, `:153`) is a different failure again —
the backend _does_ emit it, as `functionName`. That one is task 0380.

What the parser actually emits for `INVOKE_HOST_FUNCTION`
(`crates/xdr-parser/src/operation.rs:498-510`): `hostFunctionType`,
`contractId`, `functionName`, `functionArgs`, `returnValue`. Note that
`functionArgs` carries fully decoded ScVals and already reaches the client —
the advanced view renders them today
(`web/src/pages/transaction-detail/advanced/OperationJsonDetail.tsx:132-133`).

## Consequences

- Nested contract calls are invisible in the default view even though the sub-
  invocation data would have to come from the backend to populate them.
- `buildResultSummary` always falls through to `humanizeOp`, so its two-line
  branch is unreachable.
- `contractTitle(null)` always takes the unlabelled path.

The archived note
`lore/1-tasks/archive/0359_FEATURE_asset-participation-index-remodel/notes/S-tx-render-audit.md:60`
observed the `summary_line_*` fallthrough in passing but framed it as one
absent field, not as a whole unimplemented contract.

## Decision needed before implementing

Two coherent endings, and picking one is the first step:

1. **Implement the contract.** Emit a real invocation tree (the sub-invocation
   structure is available from the XDR auth entries / diagnostic events) plus a
   contract label, and let the flow tree do what it was written for. Larger, but
   it is the feature the reporter actually asked for.
2. **Delete the dead branches.** Keep the flat render, drop ~60 lines of
   unreachable code, and stop implying a capability that does not exist.

Do not leave it as-is: unreachable code that looks like a feature is how 0380's
key mismatch survived a full frontend audit.

## Acceptance criteria

- [ ] Decision recorded (implement vs delete), with reasoning
- [ ] No frontend file reads a `details` key that no crate emits
- [ ] If implementing: nested invocations render for a known multi-hop
      transaction, verified against decoded XDR
- [ ] If deleting: `NestedCallShape`, `buildNestedChildren`,
      `buildResultSummary` and `summaryFromHeavy` reduced to what is reachable
- [ ] Coordinate with 0380 — same files, overlapping edits
- [ ] **Docs updated** — only if option 1 changes the endpoint payload
      (`docs/architecture/**` frontend data contracts) per ADR 0032
- [ ] **API types regenerated** — required for option 1, N/A for option 2
