---
id: '0380'
title: 'FE: humanize-op render + value-decode fidelity (u256/i256 decoded, not raw hex)'
type: FEATURE
status: superseded
related_adr: []
related_tasks: ['0359', '0453']
tags: [priority-medium, effort-medium, layer-frontend-pages]
links:
  - 'https://github.com/rumblefishdev/soroban-block-explorer/issues/363'
history:
  - date: '2026-07-30'
    status: superseded
    who: karolkow
    by: ['0456']
    note: >
      Scope resolved in two places: the details.function_name/functionName
      key mismatch was fixed by 0453 (wave 0) and Stage-D humanisation is
      superseded by 0453's per-type sentence layer; the remaining
      u256/i256 hex-to-decimal decode moved into 0456 (wire-truth
      umbrella) so one task owns the parser<->card contract.
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker (Stage D + §11 render nits). Frontend rendering of decoded ops.'
  - date: '2026-07-22'
    status: backlog
    who: karolkow
    note: >
      **Concrete defect found during the 0431 library sweep — one key name.**
      `web/src/pages/transaction-detail/normal/humanizeOp.ts:66` reads
      `details.function_name` (snake_case). The parser emits **camelCase**:
      `crates/xdr-parser/src/operation.rs:507` writes `"functionName"`.
      Consequence: the `INVOKE_HOST_FUNCTION` branch never fires, so every
      contract invocation on the transaction detail page degrades to
      "Invoked contract <id>" instead of "Called transfer() on CABC…".
      This exact mismatch was already found and fixed once in a sibling file —
      `OperationJsonDetail.tsx:128-131` documents it ("never matched → the
      decoded call was silently dropped from the UI"). `humanizeOp` was missed
      in that pass. Worth checking whether any third reader has the same bug.
      Also relevant to this task's u256/i256 scope: `stellar-xdr` already ships
      `num256::{u256,i256}_str_from_pieces` and `num128` equivalents. See 0431 —
      do not hand-roll the conversion.
---

# FE: humanize-op render + value-decode fidelity

## Summary

Render decoded operations human-readably in the frontend (Stage D — "humanizeOp"),
and fix value-decode fidelity so large integers show as numbers, not raw hex.

## Context

Spawned from 0359 (Stage D + §11 architecture-audit render nits). The backend now
emits complete per-op assets/participants; the FE needs to render the decoded ops
legibly. Separate rendering nits surfaced in the audit.

## Implementation

- **Stage D** — humanize-op render: present each operation's type + decoded
  fields (assets, accounts, amounts) legibly in the tx/operation views.
- **§11 MINOR** — `u256` / `i256` currently render as raw hex; decode to numeric
  (respect US number grouping — see memory: keep-us-number-grouping).

## Acceptance Criteria

- [ ] operations render human-readably (humanizeOp) — Stage D
- [ ] u256/i256 shown as decoded numbers, not raw hex
