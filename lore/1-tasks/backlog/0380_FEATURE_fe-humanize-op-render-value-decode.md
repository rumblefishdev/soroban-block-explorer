---
id: '0380'
title: 'FE: humanize-op render + value-decode fidelity (u256/i256 decoded, not raw hex)'
type: FEATURE
status: backlog
related_adr: []
related_tasks: ['0359']
tags: [priority-medium, effort-medium, layer-frontend-pages]
links: []
history:
  - date: 2026-07-13
    status: backlog
    who: karolkow
    note: 'Spawned from 0359 tracker (Stage D + §11 render nits). Frontend rendering of decoded ops.'
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
