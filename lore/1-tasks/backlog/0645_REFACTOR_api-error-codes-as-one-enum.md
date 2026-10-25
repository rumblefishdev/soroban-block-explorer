---
id: '0645'
title: 'REFACTOR: API error codes as one enum — compiler-checked, listed in OpenAPI'
type: REFACTOR
status: backlog
related_adr: []
related_tasks: ['0639']
tags: [priority-low, effort-small, layer-api]
links:
  - crates/api/src/common/errors.rs
  - crates/api/src/openapi/schemas.rs
history:
  - date: 2026-10-15
    status: backlog
    who: karolkow
    note: >
      Spawned from the 0639 review walkthrough of PR #661 (W61 A): the error
      codes are string constants, so a typo compiles and OpenAPI lists none.
---

# REFACTOR: API error codes as one enum

## Summary

Every API error answers `{"code": "...", "message": "..."}`. The codes are
21 `pub const &str` in `crates/api/src/common/errors.rs`, used at 89 call
sites (measured 2026-10-15, `grep 'errors::[A-Z_]'` outside tests), and
`ErrorEnvelope.code` is a `String` (`openapi/schemas.rs:20`). A mistyped code
compiles, the OpenAPI spec says only `code: string`, and the frontend cannot
know which codes exist. One `enum ErrorCode` fixes all three.

## Stan teraz

- Done: nothing; decided in the 0639 walkthrough (2026-10-15, W61 A).
- Next: the implementation below, one `[refactor]` PR after 0639 merges.
- In force: the JSON on the wire does not change by a single byte.

## Implementation

- `enum ErrorCode` in `common/errors.rs`, one variant per constant, each
  keeping its doc comment; `#[serde(rename_all = "snake_case")]` and
  `utoipa::ToSchema`, so OpenAPI lists every value.
- `ErrorEnvelope.code: ErrorCode`; the helpers (`errors::not_found`,
  `internal_error`, …) take the enum; every call site switches from the
  constant to the variant.
- Regenerate `libs/api-types`; the frontend gets a union type of the codes.
- Out of scope: mapping domain errors (`VerificationError`, `ArchiveError`)
  to codes by trait — kept as explicit `match` at the handler.

## Acceptance Criteria

- [ ] No `pub const …: &str` error code remains; a typo in a code fails to
      compile.
- [ ] A test pins each variant's serialised text to today's string (no wire
      change); existing handler tests pass unchanged.
- [ ] `openapi.json` lists the codes as an enum; API types regenerated.
- [ ] **Docs updated** — `docs/architecture/backend/backend-overview.md`
      error section, or `N/A — reason`.
