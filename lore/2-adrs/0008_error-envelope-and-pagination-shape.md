---
id: '0008'
title: 'Error envelope and pagination shape for the HTTP API'
status: accepted
deciders: [stkrolikiewicz, karolkow]
related_tasks:
  ['0042', '0043', '0046', '0047', '0050', '0051', '0053', '0057', '0254']
related_adrs: ['0005']
tags: [layer-backend, scope-api-contract]
links: []
history:
  - date: 2026-04-08
    status: accepted
    who: stkrolikiewicz
    note: 'ADR created alongside task 0042 (OpenAPI infrastructure). Shapes defined in crates/api/src/openapi/schemas.rs.'
  - date: 2026-05-24
    status: accepted
    who: karolkow
    note: >
      Amended (task 0254) with the direction-aware cursor extension:
      `PageInfo` gains a `prev_cursor: Option<String>` field, renames
      `cursor` → `next_cursor` for symmetry, and drops the redundant
      `has_more` boolean (subsumed by `next_cursor.is_some()`);
      cursor payloads are wrapped in a `{dir, p}` envelope so backend
      can branch SQL between forward (DESC `<`) and backward (ASC `>`,
      then reverse) walks. Clean break — legacy cursors (pre-envelope)
      are rejected with `invalid_cursor`.
---

# ADR 0008: Error envelope and pagination shape for the HTTP API

**Related:**

- [Task 0042: OpenAPI/Swagger infrastructure setup](../1-tasks/archive/0042_FEATURE_openapi-swagger-infrastructure.md)
- [Task 0254: Backend `prev_cursor` + pagination test suite](../1-tasks/active/0254_FEATURE_backend-prev-cursor-and-pagination-tests.md) — direction-aware extension
- [ADR 0005: Rust-only backend API](./0005_rust-only-backend-api.md)

---

## Context

Task 0042 introduces the M1 OpenAPI infrastructure. Before any M2 endpoint modules (tasks 0043–0053) are wired in, two cross-cutting shapes need to be fixed once and then honoured uniformly:

1. **Error envelope** — the JSON body that every failing request returns
2. **Pagination envelope** — the wrapper that every list endpoint uses to expose its items

Both shapes are load-bearing. Changing them after M2 endpoints ship is a breaking change for every frontend consumer, every external SDK client, and every integration test. Picking them in a vacuum inside a single task's implementation notes is not enough — the decision deserves an explicit ADR so future contributors can trace the reasoning, and so changes go through the ADR lifecycle (superseded-by) rather than silent drift.

---

## Decision

Adopt the following Rust shapes, defined in `crates/api/src/openapi/schemas.rs`. `ErrorEnvelope` and `PageInfo` are registered eagerly as OpenAPI schema components via `ApiDoc::components(schemas(...))`. `Paginated<T>` is a generic wrapper that appears in the OpenAPI output only when concretely instantiated by endpoint handler return types — no concrete instantiation exists yet in M1, so M2 endpoint modules will register `Paginated<Transaction>`, `Paginated<Ledger>`, etc. automatically via their handler signatures.

```rust
pub struct ErrorEnvelope {
    pub code: String,                          // stable machine-readable key
    pub message: String,                       // human-readable description
    pub details: Option<serde_json::Value>,    // optional structured context
}

pub struct PageInfo {
    pub next_cursor: Option<String>,   // opaque forward cursor, None on last page (renamed from `cursor` in task 0254 for symmetry with prev_cursor)
    pub prev_cursor: Option<String>,   // opaque backward cursor, None on first page (task 0254)
    pub limit: u32,                    // page size that produced `data`
}

pub struct Paginated<T: ToSchema> {
    pub data: Vec<T>,
    pub page: PageInfo,
}
```

Every failure response body across all M2 endpoints must serialise to `ErrorEnvelope`. Every list endpoint must return `Paginated<T>` for some concrete `T`. Mixing shapes (e.g. one endpoint returning a bare JSON array) is forbidden without a follow-up ADR superseding this one.

**Status codes** are not part of this ADR — the envelope is the response _body_, status codes follow normal HTTP semantics.

---

## Rationale

### Error envelope

- **Stable `code`** is the contract surface clients key off. Changing `message` wording is not a breaking change; changing `code` is. This split mirrors the pattern used by Stripe, GitHub, and Slack APIs, where message text is localised/edited freely while codes are versioned.
- **Optional `details`** as untyped JSON keeps the shape open for field-level validation errors, rate-limit metadata, and future structured context without requiring a new envelope version.
- The shape is **intentionally minimal** — three fields covers every known use case in the explorer API and leaves no room for bikeshedding. See "Alternatives" for why we did not adopt RFC 7807.

### Pagination

- **Cursor-based, not offset-based.** The explorer indexes a live ledger stream — new records arrive continuously. Offset pagination over a growing collection produces skipped and duplicated items as pages shift between requests. Cursor pagination gives stable listings regardless of stream advances.
- **Opaque cursor string.** Encoded as whatever the endpoint needs (base64-encoded `(id, ts)`, sequence number, etc.) — opacity means the backend can change the encoding later without a client migration.
- **`next_cursor.is_some()` is the canonical "more pages exist" signal.** The original ADR carried an explicit `has_more: bool` field for the same purpose; task 0254 dropped it as redundant once `Option<String>` already conveys end-of-stream cleanly on both directions (`next_cursor: None` = last forward page, `prev_cursor: None` = first page). The server still answers definitively — the answer just lives inside the `Option`.
- **Single generic `Paginated<T>`**. One shape for every list endpoint means one TypeScript type on the frontend, one SDK helper, one error-handling branch. DRY wins here because the shape is genuinely identical everywhere.
- Aligns with **Stellar Horizon API** conventions (our upstream data source), reducing cognitive load for users coming from Horizon.

---

## Alternatives Considered

### Alternative 1: RFC 7807 `application/problem+json`

**Description:** Use the IETF standard problem-details format with `type`, `title`, `status`, `detail`, `instance` fields.

**Pros:**

- IETF standard, widely recognised
- `type` as a URI gives a canonical link to error documentation
- Plays well with JSON schema validators that understand problem+json

**Cons:**

- Five required fields where three suffice
- `type` URI discipline is real overhead — either we maintain a docs site with one page per error type, or we fake it with `about:blank` and lose the value
- `status` field duplicates the HTTP status code, inviting drift between headers and body
- `instance` is almost never useful — we don't generate per-request identifiers anyway
- Most public API consumers do not actually check `Content-Type: application/problem+json` and treat it as regular JSON, so the formal benefit is lost

**Decision:** REJECTED — overkill for a block explorer API. The simpler `{code, message, details}` shape is easier to produce, easier to consume, and avoids the `type` URI maintenance burden.

### Alternative 2: Offset-based pagination (`?offset=100&limit=50`)

**Description:** Classic offset pagination as used by most CRUD APIs.

**Pros:**

- Trivial to implement on top of SQL `LIMIT/OFFSET`
- Clients can jump to arbitrary pages (`?page=5`)
- Familiar to every backend developer

**Cons:**

- **Unstable under concurrent writes.** The ledger stream is append-only but constantly advancing — offset 100 means something different on every request
- Skipped/duplicated items when new records land between consecutive page fetches
- Deep offsets are expensive in PostgreSQL (`OFFSET 10000` scans and discards 10k rows)
- Does not match Stellar Horizon's API idiom

**Decision:** REJECTED — the instability is a correctness issue for an explorer, not a theoretical concern. Cursor pagination is the right pattern for append-only streams.

### Alternative 3: Per-endpoint bespoke envelopes

**Description:** Let each endpoint define its own list response shape — e.g. `TransactionListResponse { transactions: Vec<Transaction>, next_cursor: Option<String> }` rather than `Paginated<Transaction>`.

**Pros:**

- Each endpoint is self-contained
- Frontend type names are domain-specific (`TransactionList` vs `Paginated<Transaction>`)

**Cons:**

- 20+ endpoint types to maintain
- Every pagination bugfix needs to land in 20+ places
- Frontend cannot write a single paginated-list hook; needs one per endpoint
- Drift over time is inevitable without enforcement

**Decision:** REJECTED — the uniformity gain is worth far more than the minor type-name verbosity cost. utoipa handles generic schemas correctly in utoipa 5.

---

## Consequences

### Positive

- One shape, one TypeScript type, one error-handling branch on the frontend
- Pagination bugs fix in one place
- Documented decision: future contributors (and future sessions) can trace why these shapes look the way they do
- OpenAPI spec validates handler return types against these shapes at compile time — drift is caught at build

### Negative

- Changing either shape later is a breaking change and requires a new ADR superseding this one
- `details: Option<serde_json::Value>` loses some OpenAPI schema fidelity — `details` is documented as "arbitrary JSON". Consumers who want structured validation of a specific error's `details` need to inspect the `code` first
- Cursor pagination means no arbitrary jumping — clients can only advance through pages. Considered acceptable for an explorer where most users consume newest-first streams anyway

---

---

## Cursor direction encoding (task 0254 amendment)

The original ADR specified a one-way cursor (`PageInfo.cursor: Option<String>`, renamed to `next_cursor` in this amendment)
— forward walks only, no `prev_cursor`. Task 0238 worked around the lack of a
backward cursor with an in-memory prev-stack in `useCursorPagination` that
reset on refresh / remount, breaking the deep-link-then-Prev flow. Task 0254
introduces a backend `prev_cursor` and removes the FE stack hack. This
section documents the wire-shape extension and the SQL algebra.

### Decision

`PageInfo` gains a second opaque cursor and drops `has_more`:

```rust
pub struct PageInfo {
    pub next_cursor: Option<String>,   // forward / next — renamed from `cursor` for symmetry
    pub prev_cursor: Option<String>,   // backward / prev — NEW
    pub limit: u32,
}
```

- `prev_cursor: None` ⇒ first page (the client has not paged forward yet).
- `next_cursor: None` ⇒ last page (no further forward continuation).

Both cursor strings are emitted independently — middle pages carry both,
first pages omit `prev_cursor`, last pages omit `next_cursor`. The
redundant `has_more: bool` field was removed: `next_cursor.is_some()`
carries the same signal with zero ambiguity ("which is the truth,
`next_cursor` or `has_more`?").

### Cursor wire envelope

Each opaque cursor string is `base64url(JSON(envelope))` where:

```rust
struct CursorEnvelope<P> {
    dir: Direction,   // "next" | "prev", REQUIRED
    p: P,             // resource-specific payload (TsIdCursor, PoolListCursor, …)
}
```

- The `dir` discriminant is required (no `#[serde(default)]`) — a missing
  `dir` decodes as `InvalidPayload`. This forecloses the silent-promotion
  failure mode where a malformed cursor would be treated as a forward walk.
- The envelope is nested rather than `#[serde(flatten)]`'d. Flatten buffers
  through `serde_json::Value`, breaks under `#[serde(deny_unknown_fields)]`
  on the payload, and silently collides if a future payload declares a
  `dir` field of its own. Nesting trades ~5 bytes per cursor for clean
  composition.
- `#[serde(deny_unknown_fields)]` on `CursorEnvelope` ensures cursors
  minted by a future deployment with extra envelope fields fail decode on
  current code instead of being misinterpreted.

### SQL direction branch

Each list endpoint's `fetch_*` accepts a `direction: Direction` argument
and uses `format!()` to interpolate the comparison operator and ordering
clause:

| Direction | WHERE               | ORDER BY | Post-fetch                          |
| --------- | ------------------- | -------- | ----------------------------------- |
| `Next`    | `(ts, id) < cursor` | DESC     | drop excess row from tail           |
| `Prev`    | `(ts, id) > cursor` | ASC      | drop excess from tail, then reverse |

The Prev branch fetches in ASC so a row-constructor `>` comparison can
walk backward through the same indexes. The post-fetch reverse normalises
to DESC presentation order so handler-side row → DTO mapping is
direction-agnostic. Multi-key cursor payloads (`SharesCursor`,
`NftTransferCursor`, `PoolListCursor`) compose identically — Postgres
row-constructor comparison is lexicographic over the column tuple.

### Finalize-page matrix

`common::pagination::finalize_page` produces the two-cursor `PageInfo`
from a `limit+1` row slice and a direction:

| `direction` | input cursor        | excess | `next_cursor`          | `prev_cursor`          |
| ----------- | ------------------- | ------ | ---------------------- | ---------------------- |
| `Next`      | absent (page 1)     | yes    | last displayed (Next)  | None                   |
| `Next`      | absent (page 1)     | no     | None                   | None                   |
| `Next`      | present (mid)       | yes    | last displayed (Next)  | first displayed (Prev) |
| `Next`      | present (last)      | no     | None                   | first displayed (Prev) |
| `Prev`      | present             | yes    | last displayed (Next)¹ | first displayed (Prev) |
| `Prev`      | present (hit start) | no     | last displayed (Next)¹ | None                   |

¹ Backward walks anchor `next_cursor` at the oldest
displayed row unconditionally. The explorer's data is immutable
(append-only ledger stream), so the cursor anchor was sourced from a
prior forward fetch and always references a row that still exists in
the table. No EXISTS check is needed.

### Clean break — no legacy cursor decode

Pre-amendment cursors (bare-payload JSON, no `dir`, no envelope) are
**rejected** on the new code path with `CursorError::InvalidPayload`,
surfacing as HTTP 400 `invalid_cursor`. There is no fallback to
`Direction::Next`. Rationale: silent promotion of a malformed cursor to
a forward walk is a sharper failure mode than a 400 — a legacy cursor
that decodes "successfully" but lacks intent is precisely the class of
bug the envelope is designed to catch. The trade-off is paid once at
rollout: any in-flight cursor (browser tab open across the deploy) gets
a 400; the FE shows an error state and the user navigates to first
page. Acceptable for the explorer's traffic profile.

---

## References

- [Stripe API errors](https://stripe.com/docs/api/errors) — pattern reference for `code` + `message`
- [GitHub REST API errors](https://docs.github.com/en/rest/overview/resources-in-the-rest-api#client-errors) — similar minimal envelope
- [Stellar Horizon API](https://developers.stellar.org/api/introduction/pagination/) — cursor pagination we mirror
- [RFC 7807 Problem Details](https://datatracker.ietf.org/doc/html/rfc7807) — rejected alternative
- `crates/api/src/openapi/schemas.rs` — canonical source of the `PageInfo` shape (forward + `prev_cursor`)
- `crates/api/src/common/cursor.rs` — `Direction` enum + `CursorEnvelope` wrapper
- `crates/api/src/common/pagination.rs` — direction-aware `finalize_page` (matrix above)
- `libs/ui/src/table/useCursorPagination.ts` — frontend symmetric `goNext` / `goPrev`
