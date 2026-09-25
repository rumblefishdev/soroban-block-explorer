---
id: '0510'
title: 'REFACTOR: the auth path is absent from the API schema, so the frontend hand-mirrors its type'
type: REFACTOR
status: active
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
  - date: '2026-09-25'
    status: active
    who: karolkow
    note: >
      Activated. Re-verified on develop c0e85893 before starting: the path is
      still absent from openapi.json, the handler and the hand-written type are
      unchanged since filing. Two corrections to the plan: the frontend imports
      the generated TYPE only (calling the generated SDK from session.ts would
      deadlock on its own interceptor), and the handler is mounted only when
      the JWT secret is set, so the spec registration must not mount it.
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

- [x] The session endpoint appears in `libs/api-types/src/openapi.json` —
      `POST /auth/session`, `security: [{}]`, request `SessionRequest`, 200
      `SessionResponse`, 403/500/503 as `text/plain` (the handler answers plain
      text, not `ErrorEnvelope`)
- [x] The frontend uses the generated type; no hand-written mirror remains —
      `session.ts` imports `type SessionResponse` from `@rumblefish/api-types`
- [x] Changing the server response shape without regenerating fails CI —
      proved: renaming `expires_in` → `expires_at` in Rust changes the
      extracted spec (`diff` exit 1 on the `SessionResponse` schema), which is
      what `check-generated` diffs; after regeneration `session.ts` would stop
      typechecking on `body.expires_in`
- [x] No mutable bindings at module scope in the session module; a test can
      create an isolated instance — `createSession(...)` holds the token, expiry
      and in-flight promise; `createTurnstileSolver()` holds the script promise.
      `web/src/api/__tests__/session.test.ts`, 7 tests, including two instances
      that do not share state
- [x] **Docs updated** — `docs/architecture/frontend/frontend-overview.md` §4.5
      (the session type comes from the package; plain `fetch` and why) and
      `docs/architecture/backend/backend-overview.md` (`/auth/session` opts out
      of the global security requirement; why it sits in `paths(...)`)
- [x] **API types regenerated** — `pnpm nx run @rumblefish/api-types:generate`;
      new path, `SessionRequest` / `SessionResponse` schemas, `session` SDK
      function and mutation hook (the frontend does not call either)

## Implementation Notes

- `crates/api/src/auth/mod.rs` — `SessionRequest` / `SessionResponse` derive
  `ToSchema`; `SessionResponse` is now `pub` with documented fields;
  `#[utoipa::path]` on `session`.
- `crates/api/src/openapi/mod.rs` — `paths(crate::auth::session)` in `ApiDoc`.
- `crates/api/src/lib.rs` — declares `mod auth` so the lib target (which
  `extract_openapi` uses) can see the handler; it was declared only in `main.rs`.
- `web/src/api/session.ts` — `createSession({ siteKey, apiBaseUrl, solve? })`
  returns `{ ensureToken, invalidate }`; the Turnstile code is unchanged apart
  from taking its script loader as an argument.
- `web/src/api/client.ts` — builds the one app-wide session and calls it from
  the two interceptors.

Verification (2026-09-25):

- `cargo clippy -p api --all-targets -- -D warnings` clean; `cargo fmt -p api
--check` clean; `cargo test -p api --lib auth` 14 passed.
- `nx run web:test` 47 files / 392 tests passed; `web:typecheck` passed;
  `web:lint` 0 errors (4 warnings, all in files this task does not touch).
- Mutation check: removing the single-flight guard fails "shares one
  round-trip between concurrent callers".

## Design Decisions

### From Plan

1. **Schema first, then the type, then the state** — in the task's order.

### Emerged

2. **Spec-only registration through `ApiDoc` `paths(...)`**, not
   `register_routes`. `register_routes` mounts what it lists, and the route
   must exist only when the auth layer is armed (`main::app` mounts it by
   hand). Listing the path in the derive puts it in the spec without mounting
   it. Considered and rejected: moving `AuthConfig` into `AppState` so the
   route could be mounted always and answer 503 when dark — a runtime change
   for a spec problem.
3. **Generated type, not generated client.** The task's step 3 assumed the
   generated client. `session.ts` keeps plain `fetch`: the SDK client's
   request interceptor awaits `ensureToken`, so the session call through that
   client would wait on its own in-flight promise.
4. **A factory with closure state**, not a class or React context. The module
   has one consumer (`client.ts`, outside React), so a context would add a
   provider for nothing; the closure gives each instance its own state, which
   is what the test needs.
5. **The Turnstile script promise moved into a solver factory** as well, so
   the module has no mutable binding at all; `solve` is injectable, so tests
   need no widget.
6. **The internal note about registration is a `//` comment, not `///`** —
   utoipa copies doc comments into the public spec description.

## Issues Encountered

- **`crate::auth` not found in the lib target.** The crate compiles its module
  tree twice (lib for `extract_openapi`, bin for the Lambda); `auth` was only
  in the bin. Fixed by declaring it in `lib.rs` (the lib already allows
  `dead_code` for exactly this).
- **Five page tests timed out** when `test`, `lint` and `typecheck` ran in
  parallel alongside two other builds; each passes alone and the full `test`
  target passes on its own run. Load, not a regression.

## Future Work

- `crates/api/src/auth/mod.rs` still carries its tests inline. Extracting them
  is a move, so it belongs in its own PR, not this one.
