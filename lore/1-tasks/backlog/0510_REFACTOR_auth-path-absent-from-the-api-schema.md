---
id: '0510'
title: 'REFACTOR: the auth path is absent from the API schema, so the frontend hand-mirrors its type'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0455']
tags: ['api', 'frontend', 'openapi', 'effort-small', 'priority-medium']
links: []
history:
  - date: 2026-08-19
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0455 review sweep (findings 48, 49). Bundled because they
      are one cause and two symptoms: the hand-written type exists precisely
      because the endpoint is missing from the schema, and the module-level
      mutable state is the same missing contract on the client side.
---

# REFACTOR: the auth path is absent from the API schema

## Summary

The session endpoint is not described in the generated OpenAPI document, so the
frontend carries a hand-written copy of its response type. That copy is not
checked against the server by anything — the CI gate that keeps API types fresh
cannot see a path the schema never mentions.

## Context — verified 2026-08-19

- `libs/api-types/src/openapi.json` contains **zero** occurrences of `/auth`.
- `web/src/api/session.ts` declares the response shape by hand.
- The same file keeps **four** mutable bindings at module level (lines 23, 24,
  29, 116): the token, its expiry, an in-flight promise, and a script-loading
  promise.

The two are the same gap seen from both ends. With no schema entry there is no
generated client, so the module grows its own ad-hoc client — and an ad-hoc
client keeps its state where it lands, which here is module scope. Module-level
mutable state is invisible to tests unless the module is re-imported, and it is
shared by every consumer whether they want that or not.

The failure mode is quiet: the server changes the session response, the CI
freshness gate stays green because the path is not in the schema, and the
frontend keeps reading a field that stopped existing.

## Implementation

1. Add the auth/session path to the API's OpenAPI schema and regenerate types.
2. Replace the hand-written type with the generated one; delete the copy.
3. With a generated client in place, move the four module-level bindings into an
   explicit holder the callers pass or a context the app provides — whichever
   fits the existing frontend conventions. The point is that the state has an
   owner and a test can construct a fresh one.

Step 1 is the one that matters; 2 and 3 follow from it and are small.

## Acceptance Criteria

- [ ] The session endpoint appears in `libs/api-types/src/openapi.json`
- [ ] The frontend uses the generated type; no hand-written mirror remains
- [ ] Changing the server response shape without regenerating fails CI
- [ ] No mutable bindings at module scope in the session module; a test can
      create an isolated instance
- [ ] **Docs updated** — the frontend data-contract section names the auth path
- [ ] **API types regenerated** — required; this task's whole point
