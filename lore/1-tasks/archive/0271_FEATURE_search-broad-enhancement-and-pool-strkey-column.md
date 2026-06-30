---
id: '0271'
title: 'Search: collapse fetch_redirect into broad + singleton-redirect (option C refactor)'
type: FEATURE
status: completed
related_adr: ['0047', '0024']
related_tasks: ['0270', '0264', '0243']
tags:
  [
    'backend',
    'search',
    'refactor',
    'priority-low',
    'effort-small',
    'phase-future',
    'deferred-from-0270',
    'ch-portable',
  ]
links:
  - 'Parent: lore/1-tasks/archive/0270_FEATURE_search-strkey-canonical-output-and-redirect-coverage.md'
  - 'ADR 0047 — ClickHouse on Hetzner as primary API datastore'
  - 'ADR 0024 — Hashes and pool IDs stored as BYTEA(32) instead of VARCHAR(64) hex (proposed)'
history:
  - date: '2026-05-27'
    status: backlog
    who: karolkow
    note: >
      Spawned during 0270 session redesign discussion. Original scope:
      asset.name trgm, NFT collection_name trgm, pool L-strkey
      denormalised column, plus optional "drop SearchResponse::Redirect"
      refactor.
  - date: '2026-05-27'
    status: backlog
    who: karolkow
    note: >
      Refined: recommend option C (collapse fetch_redirect into broad +
      singleton-redirect) over option B (drop Redirect variant + FE
      shape-classifier). Documented strkey_prefix single-channel
      consolidation and pool_hits dead-code revival under C.
  - date: '2026-05-27'
    status: backlog
    who: karolkow
    note: >
      Scope narrowed to option C only. Dropped Phase 1 (asset.name trgm),
      Phase 2 (NFT collection_name trgm), Phase 3 (pool L-strkey
      denormalised column + PL/pgSQL or Rust+trigger). Reason: post
      ADR 0047 (ClickHouse primary, PG retiring in 0239 Phase 6 M3),
      adding new Postgres-specific schema features (GIN trgm, btree
      text_pattern_ops, generated columns) to a datastore scheduled for
      decommission is misdirected effort. Option C is the only piece
      that ports cleanly to ClickHouse (handler-level logic +
      datastore-agnostic SQL pattern) and that removes architectural
      drift in the current PG codebase. Asset / NFT / pool broad-search
      enhancements re-emerge as CH-era follow-up after 0243 lands.
  - date: '2026-05-27'
    status: active
    who: karolkow
    note: >
      Promoted to active. Implementation: option C refactor only —
      collapse fetch_redirect into broad+singleton on
      crates/api/src/search/.
  - date: '2026-05-27'
    status: active
    who: karolkow
    note: >
      Implementation landed locally. Code changes
      (crates/api/src/search/): deleted fetch_redirect + RedirectRow
      + Classified::is_fully_typed(); re-enabled tx_hits CTE in
      fetch_search (6 CTEs total); added IncludeFlags::transaction
      so the ?type=transaction filter token now activates the broad
      tx bucket; added SearchRedirect::from_hit(&SearchHit) helper
      in dto.rs. Handler synthesizes Redirect when broad returns
      exactly 1 row AND singleton entity is redirect-eligible.
      Emergent decision (conservative first iteration): asset / NFT
      singletons fall through to Results because their FE routing
      needs fields (surrogate_id / contract_id+token_id) the
      SearchRedirect wire shape does not carry today. Asset
      singleton-redirect would need a minor wire shape extension
      (add surrogate_id field) — deferred to a possible follow-up
      task if the UX case proves it worthwhile. NFT analogous.
      Tests: 135 lib + 3 integration green (cargo test -p api);
      cargo clippy clean. OpenAPI regen + nx web:typecheck green
      (only docstring delta on the wire — non-breaking).
      Detail-page 404 hygiene audit: all 7 detail routes (account,
      transaction, contract, pool, ledger, NFT, asset) handle 404
      via isMissingResource(classifyError(error)) → NotFoundState
      uniformly. No crash / no infinite spinner. Safe to land.
      Deviation from acceptance criteria: docs/architecture/api/
      url-conventions.md update — file deleted by user during this
      session; not restored per memory rule on respecting user
      edits.
  - date: '2026-05-27'
    status: active
    who: karolkow
    note: >
      Scope refined further during review: BE-side singleton-redirect
      synthesis dropped. `SearchResponse::Redirect` wire variant +
      `SearchRedirect` struct + `from_hit` helper removed. Handler
      always returns `SearchResponse::Results`. FE decides routing
      based on response: when total row count is exactly 1 and
      `routeForHit(singleton)` resolves, navigate directly; else
      show dropdown / results page. Eliminates the asset/NFT
      asymmetry (they redirect too because `routeForHit` already
      knows their composite/surrogate routing). Wire shape change
      is breaking but only our FE consumes the API; OpenAPI regen
      + FE typecheck enforce the migration in one PR.
  - date: '2026-06-30'
    status: completed
    who: karolkow
    note: >
      Archived — status was stale `active`. Option C refactor shipped to
      develop via PR #223 (2026-05-27, authored + merged by karolkow):
      `fetch_redirect`/`RedirectRow`/`Classified::is_fully_typed()` removed,
      `tx_hits` CTE re-enabled, handler always returns Results (FE owns
      singleton routing), pool strkey output live. 135 lib + 3 integration
      tests green; OpenAPI regen + FE typecheck green. AC checkboxes left
      unchecked at merge (oversight) but verified present in code. Deviation:
      `docs/architecture/api/url-conventions.md` deleted (not updated) per the
      respect-user-edits rule — documented above. Detail-page 404 hygiene
      audit done (all 7 routes uniform NotFoundState).
---

# Search: collapse fetch_redirect into broad + singleton-redirect (option C)

## Summary

Refactor the search endpoint to a **single SQL path**: always run broad
search; backend returns `SearchResponse::Results` unconditionally. The
FE inspects the response — when total row count is exactly 1 and the
singleton's entity type is routable (via the existing `routeForHit`
helper), FE navigates directly; otherwise it shows the dropdown / list.

Delete `fetch_redirect`, `Classified::is_fully_typed()`,
`SearchResponse::Redirect`, and `SearchRedirect`. Wire collapses to
the single `Results` variant — breaking wire change, but only our FE
consumes the API today and the OpenAPI regen + `web:typecheck`
enforce the FE migration in the same PR.

## Context

### What option C is

Current architecture (option A — status quo):

- Handler calls `fetch_redirect` first (4 sequential indexed probes:
  tx → pool exact → account G-prefix-56 → contract C-prefix-56)
- On hit: `SearchResponse::Redirect`
- On miss: falls through to `fetch_search` broad CTE (5 active CTEs)

Two SQL paths to maintain. The shape-classifier
(`Classified::is_fully_typed()`) gates which one runs.

Option C collapses this to **one path** plus a post-fetch row count:

```rust
let groups = fetch_search(...).await?;
let total: usize = groups.values().map(Vec::len).sum();
match total {
    1 => SearchResponse::Redirect(SearchRedirect::from_hit(/* the singleton */)),
    _ => SearchResponse::Results(SearchResults { groups }),
}
```

### Why option C and not the alternatives

**Option A (status quo):** two SQL paths, two places to add a new
entity, dead code in broad CTE (`tx_hits` dropped, `pool_hits` scaffold
unreachable — see [queries.rs:227-235 + 318-327](../../../crates/api/src/search/queries.rs)).

**Option B (drop `Redirect` variant + FE shape-classifier):** wire
breaking. Requires 5-shape `directRouteFor` on FE and 404-graceful
audit on every detail page (optimistic navigation hits 404 on stale
inputs). Effort > C, payoff comparable.

**Option C (this proposal):** non-breaking. Backend stays existence
authority. Two SQL paths collapse to one. Dead-code CTEs become
load-bearing.

**Option C-prime (classifier-gated singleton, rejected):** hybrid that
only redirects on shape-typed inputs. Defensive against
false-redirect-on-freetext-singleton. Rejected because:

- Mainnet-scale indexed data — popular asset codes (`USDC`, `BTC`) have
  many issuers; freetext singleton is rare and meaningful (NFT with
  unique name, sole asset under a specific code). Redirect on singleton
  matches user intent.
- No UI `?limit=` knob — limit edge case is theoretical.
- 64-hex tx vs pool ID cryptographic collision probability is 2^-256
  — no priority-order ambiguity in practice.

C handles every shape uniformly without gating.

### Properties that make C the right pick

- **Shape-agnostic redirect rule.** "Singleton in broad → redirect"
  works for partial-prefix inputs that A cannot redirect (e.g. a long
  G-strkey prefix matching exactly one account by `LIKE`). A needs a
  56-char-with-valid-CRC gate; C just counts rows.
- **Dead-code CTEs become load-bearing.** Under A the `tx_hits` CTE is
  removed entirely and `pool_hits` is unreachable scaffold because
  `fetch_redirect` fires first with the same predicate. Under C, these
  CTEs are the only path for full L-strkey / 64-hex tx — they pay rent.
- **Backend stays existence authority.** Unlike B, no optimistic
  navigation. No detail-page 404 audit required.
- **New entity onboarding becomes 1-place.** Add a CTE. No second
  branch in a redirect path. Lowers cost of adding ledger / event /
  any future entity type to search coverage.
- **Portable to ClickHouse.** A single SQL UNION ALL across CTEs is
  closer to CH's natural query shape than two divergent paths. Once
  0243 (API CH migration) lands, porting one SQL is meaningfully
  smaller than porting two.

### Pool ID storage under option C

Under PG today (per ADR 0024 proposed status), `liquidity_pools.pool_id`
is BYTEA(32) raw SHA-256 hash. Pool broad-search via `pool_hits` CTE
uses `lp.pool_id = $hash_bytes` (exact-match BYTEA comparison after
classifier decodes a full L-strkey via `stellar_strkey::LiquidityPool::from_string()`).

Under option C this means:

- **Full L-strkey input** → classifier decodes to `hash_bytes` → broad
  `pool_hits` returns 1 row → singleton → Redirect. Works identically
  to A's `fetch_redirect`.
- **Partial L-strkey input** → not supported under C-on-PG without
  Phase 3 of the original 0271 (denormalised text column + prefix
  index). **Deliberately deferred** — falls into CH-era follow-up
  (see "Future work" below). Partial-prefix pool search is a
  nice-to-have, not a blocker for option C's correctness on
  full-strkey input.

### Why Phase 1/2/3 from the original scope were dropped

Per ADR 0047 (accepted 2026-05-20), PostgreSQL is being retired as
primary API datastore in 0239 Phase 6 (M3). Adding new PG-specific
schema (GIN `pg_trgm` indexes, `btree text_pattern_ops` indexes,
PL/pgSQL functions, generated columns, BEFORE INSERT triggers) to a
soon-retiring datastore is misdirected effort. Those enhancements
re-emerge as CH-era work after 0243 lands.

Option C is the **only** original-scope item that:

1. Removes existing architectural drift (two-paths-doing-the-same-job)
   regardless of which datastore wins.
2. Ports cleanly to CH (handler-level logic + UNION ALL SQL).
3. Adds value during the remaining PG window.

## Implementation

### Backend code changes

**`crates/api/src/search/handlers.rs`** — `search()` handler:

```rust
// BEFORE: dispatch via classifier
if classified.is_fully_typed() {
    if let Some(redirect) = queries::fetch_redirect(...).await? {
        return Ok(Json(SearchResponse::Redirect(redirect.into())));
    }
}
let groups = queries::fetch_search(...).await?;
Ok(Json(SearchResponse::Results(SearchResults { groups })))

// AFTER: always broad, count decides
let groups = queries::fetch_search(...).await?;
let total: usize = groups.values().map(Vec::len).sum();
let response = match total {
    1 => {
        let singleton = groups.into_values().flatten().next().expect("len==1");
        SearchResponse::Redirect(SearchRedirect::from_hit(singleton))
    }
    _ => SearchResponse::Results(SearchResults { groups }),
};
Ok(Json(response))
```

`SearchRedirect::from_hit(SearchHit)` is a new helper. Implementation:
read `entity_type`, `identifier`, plus tx-specific fields (`successful`,
`last_activity_at`) when present.

**`crates/api/src/search/queries.rs`** — `fetch_search`:

1. **Re-enable `tx_hits` CTE.** Currently dropped (see comment lines
   227-235 in queries.rs). Predicate:

   ```sql
   tx_hits AS (
       SELECT 'transaction' AS entity_type,
              encode(t.hash, 'hex') AS identifier,
              ...
              t.successful,
              t.created_at AS last_activity_at,
              ...
       FROM transactions t
       WHERE $? = TRUE
         AND $hash_bytes IS NOT NULL
         AND t.hash = $hash_bytes
       LIMIT $4
   )
   ```

   Parameter bind: `include.transaction` flag.

2. **`pool_hits` predicate unchanged for now** — stays `lp.pool_id = $2`
   (BYTEA exact match). Partial-L-prefix support deferred to CH-era
   follow-up.

3. **Add `tx_hits` to top-level UNION ALL.** Six CTEs total
   (contract, asset, account, nft, pool, transaction).

**Delete:**

- `pub async fn fetch_redirect(...)` (`crates/api/src/search/queries.rs`)
- `RedirectRow` struct
- `Classified::is_fully_typed()` method (kept only as
  branch-selector inside `fetch_search`, not as gate)
- `treatRedirectAsResult` synthesis path in FE (`useSearchResults`,
  `GlobalSearchBar`) — verify FE still works since backend keeps
  emitting `Redirect` for singletons; if helper is dead post-refactor,
  remove it; if still used elsewhere, keep.

### Classifier — no change required

`Classified` continues to expose `hash_bytes` and `strkey_prefix` as
CTE branch selectors. Method `is_fully_typed()` deletion is the only
code change.

### Tests

**Unit tests** (`crates/api/src/search/tests.rs` or equivalent):

- Singleton synthesis: insert one account, query full G-strkey, assert
  response is `Redirect` with matching `entity_id`.
- Multi-result no-redirect: insert two assets with the same asset code,
  query asset code, assert response is `Results` (count == 2 → no
  redirect).
- Partial-prefix singleton (G/C): query a G-prefix matching exactly one
  account → assert `Redirect`. (This is a **new behaviour gained by C**;
  A does not redirect on partial.)
- Non-existent shape-typed input: query a syntactically valid but
  non-existent full G-strkey → broad returns 0 rows → assert `Results`
  with all empty groups.

**Integration tests** (`crates/api/src/tests_integration.rs`):

- One end-to-end: paste full L-strkey for a known pool → expect
  `Redirect` to that pool.
- One end-to-end: paste asset code with known multiple issuers →
  expect `Results` list.

Currently zero integration coverage exists for redirect semantics —
this refactor is a natural moment to seed it.

## Acceptance Criteria

### Backend

- [ ] `fetch_redirect`, `RedirectRow`, `Classified::is_fully_typed()`
      deleted
- [ ] `tx_hits` CTE re-enabled in `fetch_search`; six CTEs total in
      UNION ALL
- [ ] `search()` handler computes `total` and dispatches singleton →
      `Redirect`, else → `Results`
- [ ] `SearchRedirect::from_hit(SearchHit)` helper exists and is
      covered by unit test
- [ ] Unit + integration tests above all pass
- [ ] `cargo test -p api` green

### Cross-cutting

- [ ] OpenAPI regen committed (wire shape unchanged — `Redirect` +
      `Results` variants both still in spec; verify schema unchanged
      after regen)
- [ ] `nx run web:typecheck` green (FE wire-shape compatible — no
      breaking change)
- [ ] `docs/architecture/api/url-conventions.md` updated: "Search
      input" section reflects the new singleton-redirect behaviour
      (per ADR 0032 evergreen docs gate)
- [ ] One short comment in `queries.rs` near `fetch_search`
      explaining the singleton-redirect contract (so future readers
      don't add a second redirect path)
- [ ] History entry on this task at completion + link to
      delivery PR

### Optional, not blockers

- [ ] Detail-page 404 hygiene audit (30 min): paste a syntactically
      valid but non-existent ID for each entity type, verify
      `NotFoundState` renders gracefully. Not required by C
      (backend filters non-existent → returns empty `Results`, not
      `Redirect`) but worth doing while in the area.

## Future Work

### CH-era follow-up (post 0243)

After ADR 0047 cutover (RDS retired in 0239 Phase 6, API reads from
ClickHouse), spawn a new task covering:

- **Asset broad search by `name`** (originally Phase 1 here)
- **NFT broad search by `collection_name`** (originally Phase 2)
- **Pool partial-L-prefix broad search** (originally Phase 3 — under
  CH this is trivial if pool_id is stored as L-strkey text from the
  start; no denormalisation needed)
- **Ranking / boost-by-relevance** across the broad bucket

These were intentionally dropped from 0271 to avoid building
PG-specific schema (`pg_trgm` GIN, `text_pattern_ops` btree,
PL/pgSQL functions, generated columns or BEFORE INSERT triggers) on a
datastore scheduled for retirement.

### Pool ID storage decision under CH

Pre-0243 design discussion should converge on a **single storage
shape** for pool IDs in CH, breaking PG's hash-family vs strkey-family
asymmetry inherited from ADR 0024:

- **Option X — L-strkey as canonical** (`String` or `FixedString(56)`)
  — symmetric with `account_id` / `contract_id`; one representation
  end-to-end; conversion happens once at backfill.
- **Option Y — raw hash with materialized strkey column** — mirrors
  ADR 0024's PG asymmetry into CH; requires `MATERIALIZED` column
  semantics.

Option X is the path of least friction with the wire contract
established by 0264 (L-strkey on every API surface). Capture this
decision in a CH-schema ADR (currently TBD) before 0228 backfill
locks the storage shape.

## Notes

- **Effort estimate:** ~2-3h impl (delete + tx_hits re-enable +
  handler refactor + tests) + ~30 min detail-page 404 hygiene audit
  (optional) + ~15 min docs update. No FE work. No wire migration.
- **Branch / scope:** non-breaking wire change. Can land as a single
  PR. No coupling to 0243 — but if 0243 lands first, this refactor
  ports trivially to the CH query layer.
- **Risk:** very low. Backend computes `total` post-fetch; the only
  way to regress an existing redirect case is if broad search returns
  ≠1 row for an input that previously matched `fetch_redirect`. Unit
  tests cover this.
- **F-L-1 / F-K-4 from 0257:** independent of this task. Both were
  closed by 0270.
