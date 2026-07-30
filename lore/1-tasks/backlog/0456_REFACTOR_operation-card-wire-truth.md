---
id: '0456'
title: 'REFACTOR: typed truth for the operation card wire — diagnostic call tree, details contract, big-int decode'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0453', '0380', '0352']
tags:
  [
    backend,
    frontend,
    xdr-parser,
    api,
    transaction-detail,
    priority-high,
    effort-large,
  ]
links: []
history:
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Spawned from 0453 follow-ups (five-agent review of PR #373). Groups the
      three wire-truth threads so one task owns the contract between the
      parser and the operation card.
  - date: '2026-07-30'
    status: backlog
    who: karolkow
    note: >
      Scope-1 update from the 0462 investigation: the DETAIL PAGE does not
      need the backend to expose the diagnostic tree — `heavy.
      diagnostic_events` already carries fn_call/fn_return and a client-side
      stack walk reconstructs execution exactly (verified on prod fixture,
      depth 5). 0462 ships that render FE-only. Scope 1 here remains
      valuable for OTHER surfaces (contract page, indexed queries) and for
      replacing the auth-tree fallback, but is no longer the blocker for an
      honest per-node view on the card.
---

# REFACTOR: typed truth for the operation card wire

## Summary

Three related gaps between what the parser knows and what the card can
honestly render. One task, because they all change the same seam.

## Scope

1. **Diagnostic execution tree.** `heavy.operation_tree` is built from AUTH
   entries and stamps the whole transaction's verdict on every node
   (`crates/xdr-parser/src/invocation.rs` — "derived from the parent
   transaction's success status"), which is why 0453 removed per-node ✗/✓
   glyphs and labeled the section "Authorized calls". The same file already
   computes the richer diagnostic-events tree for the unexposed flat
   `invocations` list — emit THAT (with real per-node `successful`) as
   `operation_tree` (or alongside), regenerate API types, and restore the
   "failed here" pinpoint in `op-card/CallTree.tsx` honestly.
2. **Typed `details` contract.** `XdrOperationDto.details` is `unknown` on
   the wire; the schema exists only as string keys duplicated in
   `operation.rs` and five FE decoders (humanizeOp, RouteStrip, CallTree,
   OperationJsonDetail, opFacts). Either utoipa tagged-union per-op schemas
   (preferred — codegen carries them, the freshness gate starts protecting
   the payload) or a single FE `decodeOpDetails(op_type, details)` module
   all consumers share. Kills the `function_name`-mismatch bug class for
   good.
3. **u256/i256 decode (absorbs the rest of 0380).** `scval.rs` renders
   `u256`/`i256` as 64-char hex; decode to decimal strings with
   `stellar-xdr`'s `num256::{u256,i256}_str_from_pieces` (do NOT hand-roll —
   see 0431).

## Acceptance criteria

- [ ] Failed invoke renders the failing nested call highlighted — from real
      per-node data, verified on a live failed Soroban tx
- [ ] One authoritative definition of every `details` key, compile-checked
      on at least one side of the wire
- [ ] `u256`/`i256` args render as decimal strings; test with a real value
- [ ] API types regenerated in the same commit as any DTO change
