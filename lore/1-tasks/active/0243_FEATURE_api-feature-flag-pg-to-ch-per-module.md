---
id: '0243'
title: 'FEATURE: API feature flag per module — gradual PG↔CH migration for all 9 handler modules'
type: FEATURE
status: backlog
related_adr: ['0044', '0047']
related_tasks: ['0207', '0228', '0239', '0240', '0241', '0244']
blocked_by: ['0241', '0239', '0240']
tags:
  [
    priority-high,
    effort-large,
    layer-api,
    layer-backend,
    clickhouse,
    feature-flag,
    gradual-migration,
  ]
milestone: 2
links:
  - crates/api/Cargo.toml
  - docs/architecture/database-schema/endpoint-queries/
  - lore/1-tasks/archive/0207_FEATURE_clickhouse-endpoint-queries-reference-set.md
history:
  - date: '2026-05-20'
    status: backlog
    who: stkrolikiewicz
    note: >
      Spawned from M1-M3 sequencing plan (2026-05-20). Closes the D2 AC #1 gap
      ("all REST API endpoints schema-valid against mainnet data") after the
      PG→CH pivot. Currently `crates/api/` depends exclusively on sqlx (PG).
      Team decision (2026-05-20): feature flag per module (9 flags, gradual
      rollout, safe rollback per handler).
  - date: '2026-06-05'
    status: active
    who: karolkow
    note: >
      Branch feat/0243-accounts-contracts-assets-ch-read-path: accounts list +
      contracts list → CH (the 0274/0275 list endpoints bypassed the flag);
      assets whole module → CH (list/detail/transactions) with composite `:id`
      routing token (Option A — surrogate dropped PR #175) and reserved `native`
      token; search `surrogate_id` → `route_token` (asset hits ship the
      canonical token, fixes the search→asset 404 + CH-has-no-surrogate dead
      path). 290 lib/unit tests pass (+ routing/canonical/cursor/route_token);
      FE web+ui typecheck clean, 17 FE tests; api-types + ADR-0032 docs updated.
      NOT done: 3 of 9 modules left (LP/NFT/search), per-module staging smoke +
      prod flips, 0231 enrichment dependency, review (see Review Plan). Code not
      committed (awaiting explicit signal).
---

# API feature flag per module — gradual PG↔CH migration

## Summary

`crates/api/` depends exclusively on `sqlx` (Postgres). After deploying 0241
(indexer hard swap CH-only), PG receives no new ledgers → the API serves
stale data. This task rewrites the API from PG to CH per module behind a
feature flag (9 flags = 9 modules), enabling a gradual rollout and safe
rollback per handler.

## Context

- 9 handler modules in `crates/api/src/`: accounts, assets, contracts,
  ledgers, liquidity_pools, nfts, network, search, transactions (23 endpoints
  total).
- Each has a per-module `queries.rs` using `sqlx::query!`.
- 17 reference CH SQL queries already mapped in
  `docs/architecture/database-schema/endpoint-queries/01..17_*.sql` (task 0207).
- 87 existing integration tests in `tests_integration.rs`.

D1 = write-path correctness (M1 task 0241). D2 = read-path correctness
(this task).

## Implementation Plan

### Step 1: Cargo deps + connection config

`crates/api/Cargo.toml`:

```toml
db-clickhouse = { path = "../db-clickhouse" }
clickhouse = { workspace = true }
# Keep sqlx for transition; task 0244 removes it post-stable
```

Connection pool initialized at Lambda cold-start
(`clickhouse::Client::default().with_url(...)`). mTLS config: cert + key + ca
from Secrets Manager (per 0239 Phase 2, user `api_reader` per 0240).

### Step 2: DataSource enum + dispatch

```rust
// crates/api/src/common/datasource.rs
pub enum DataSource {
    Pg,
    Ch,
}

impl DataSource {
    pub fn for_module(module: &str) -> Self {
        match std::env::var(format!("API_DATASOURCE_{}", module.to_uppercase())).as_deref() {
            Ok("ch") => Self::Ch,
            _ => Self::Pg, // default during transition
        }
    }
}
```

### Step 3: Per-module migration (9 modules)

For each of the 9 modules:

1. Add `queries_ch.rs` with a CH equivalent for every `queries.rs::*`
   function. Reuse SQL from
   `docs/architecture/database-schema/endpoint-queries/`.
2. In the handler: dispatch
   `match DataSource::for_module("...") { Pg => queries::*, Ch => queries_ch::* }`.
3. Schema parity: response shape stays identical — keep the typed structs,
   map `clickhouse::Row` → existing `domain::*` types.
4. Integration tests: extend with a per-flag variant, or add a separate test
   pass for `API_DATASOURCE_*=ch`.

Suggested order (simplest to most complex):

1. `network` (1 endpoint, `01_get_network_stats.sql`)
2. `ledgers` (2 endpoints, `04`, `05`)
3. `accounts` (2 endpoints, `06`, `07`)
4. `transactions` (2 endpoints, `02`, `03`)
5. `assets` (3 endpoints, `08`-`10`)
6. `nfts` (3 endpoints, `15`-`17`)
7. `contracts` (4 endpoints, `11`-`14`)
8. `liquidity_pools` (5 endpoints — the largest module, reference SQL not
   yet in 0207)
9. `search` (1 endpoint, complex — trigram + multi-table)

### Step 4: Per-module rollout protocol

For each module:

1. PR with `queries_ch.rs` + handler dispatch + tests
2. Deploy with flag `API_DATASOURCE_<MODULE>=pg` default (no-op deploy,
   tests exercise the CH path locally)
3. Manual flip `pg → ch` in the staging env config
4. 24 h smoke (CloudWatch monitoring, error rate, latency)
5. If clean: flip the prod env config, monitor for 7 days
6. If a problem surfaces: rollback the flag, debug, retry

### Step 5: Cleanup → 0244

Once all 9 modules are on `ch` default + 7 days stable → spawn 0244
(remove sqlx, drop `queries.rs`, simplify the `DataSource` enum).

## Acceptance Criteria (incremental per module)

For each of the 9 modules:

- [ ] `queries_ch.rs` authored with CH equivalents
- [ ] Handler dispatch via the `DataSource` enum
- [ ] Integration tests pass in `Pg` mode (no regression)
- [ ] Integration tests pass in `Ch` mode (parity verified)
- [ ] Staging smoke with flag=ch: 24 h without errors
- [ ] Flip default flag to `ch` in the prod env config
- [ ] 7 days of prod monitoring: error rate <0.1%, latency p95 within budget

Cross-cutting:

- [ ] Connection pool initialized at cold-start (no per-request connect)
- [ ] mTLS config working (verified via Caddy access logs
      `X-Client-Subject: CN=lambda-api-...`)
- [ ] OpenAPI spec unchanged (response schema parity) —
      `nx run @rumblefish/api-types:check-generated` passes
- [ ] **Docs updated** — `docs/architecture/api/api-overview.md` (if it
      exists) reflects the CH-default datastore; per-handler comments point
      at the CH queries
- [ ] **API types regenerated** — required only if the response schema
      changes (it should not); run `npx nx run @rumblefish/api-types:generate`
      on every module PR as a sanity check

## Depends on

- **0241** (CH carries live data from `L_last_closed + 1` — without it, CH
  returns stale history only)
- **0239 Phase 2** (mTLS connection layer)
- **0240** (RBAC user `api_reader` with appropriate permissions)
- **0207** ✅ (reference CH SQL queries already authored — reused verbatim)

## Open questions

- **Feature flag granularity**: 9 (per-module) vs 23 (per-endpoint).
  Per-module is the preferred starting point (simpler); escalate to
  per-endpoint if any module proves problematic.
- **liquidity_pools complex queries**: 5 endpoints but no entries in the
  0207 reference set yet. May require additional SQL mapping (spawn a
  follow-up if it turns out to be large).
- **search trigram**: the PG implementation uses the `pg_trgm` extension. CH
  has its own approaches — may require a redesign or a fallback to PG for
  search in the first iteration.

## Notes

- Feature flag defaults to `pg` for the entire transition; per-module flips
  are explicit operator actions. Operator-driven rollout.
- After the final flip across all 9 modules → spawn 0244 (cleanup).
- Sibling task: 0231 (CH enrichment port) — independent, but both rely on
  the CH connection setup landed by this task as a precedent.

---

## Module status

| Module                         | CH read path                                                              | Landed by                       |
| ------------------------------ | ------------------------------------------------------------------------- | ------------------------------- |
| network, ledgers, transactions | ✅ detail+list                                                            | stkrolikiewicz (#221/#226/#235) |
| accounts                       | ✅ detail+tx (prior) **+ list (this branch)**                             | stkrolikiewicz + karolkow       |
| contracts                      | ✅ detail+interface+invocations (prior) **+ list + events (this branch)** | stkrolikiewicz + karolkow       |
| **assets**                     | ✅ **list + detail + transactions (this branch)**                         | karolkow                        |
| liquidity_pools, nfts, search  | ❌ PG-only (next)                                                         | —                               |

## Implementation Notes — branch `feat/0243-accounts-contracts-assets-ch-read-path` (karolkow)

- **accounts/contracts list → CH** — the FE-audit list endpoints (0274/0275)
  shipped hard-wired to PG, bypassing the module flag (a latent stale-read once
  PG stops being written). Added `queries_ch::fetch_list` /
  `fetch_contract_list` + `DataSource::for_module` dispatch, mirroring the
  detail handlers next to them.
- **assets whole module → CH** — new `assets/queries_ch.rs` (list +
  by-contract + by-code-issuer + native + transactions), two-step driver-seek
  for tx (same shape as accounts/contracts), `assets a FINAL` + plain lookup
  joins, `join_use_nulls=1`, positional `clickhouse::Row` decode.
- **Composite `:id` routing (Option A)** — the numeric surrogate was dropped on
  CH (PR #175), so `/assets/:id` is a single canonical token: contract StrKey /
  `CODE-ISSUER` / reserved `native`. `AssetItem.id` `i32 → String` (the token);
  list cursor `AssetIdCursor{id}` → composite `AssetKeyCursor` over the CH
  `assets` ORDER BY 4-tuple. PG `queries.rs` adapted to the same contract so the
  shape is datasource-agnostic.
- **search route_token fix** — search asset hits emitted the dropped numeric
  `surrogate_id` (which `/assets/:id` now 400s, and which doesn't exist on CH at
  all — the CH search doc even manufactured a `cityHash64` of it). Replaced
  `surrogate_id` (`i64`) with `route_token` (`String`) across the search wire;
  asset hits carry the canonical token, others `null` (route on `identifier`);
  FE `routeForHit` routes `route_token ?? identifier`. CH `22_get_search.sql`
  `cityHash64` footgun removed.
- **contracts events → CH (fixes the live split-brain)** — `list_events`
  dispatches PG/CH. CH reads `soroban_events` directly (full-content per-event,
  inline JSON payload already ScVal-decoded + diagnostic-filtered at ingest), so
  NO Archive S3 overlay / read-time decode. New 3-component `EventCursor`
  (`ledger_sequence, transaction_id, event_index`), datasource-tagged + cross-
  source guard. CH pages per event (`data.len() <= limit`) vs PG per folded
  appearance. `EventItem` wire shape unchanged. CH reference SQL 14 banner
  flipped to MIGRATED. (Discovered: with the 0241 indexer cutover, PG events had
  gone stale under `CONTRACTS=ch` — this closes it.)
- **fetch_limit() normalised** across this branch's lists + assets-tx (handler
  owns the `+1` peek; queries bind raw) — matches accounts-list.
- **Tests:** 294 unit/lib pass (+ routing/canonical/cursor/route_token + 4 event
  decode tests). FE: web+ui typecheck clean, 17 FE tests pass. api-types
  regenerated (EventItem unchanged; only the `limit` param doc + asset `id`).
- **Docs (ADR 0032):** `backend/backend-overview.md`,
  `frontend/frontend-overview.md`, CH `09`/`22` SQL docs updated.

## Design Decisions

### Emerged

1. **Assets `:id` = composite token, Option A (not a re-added surrogate).**
   Validated by two independent fresh-eye reviews: the strkey-vs-code-issuer
   "mixing" is the honest shape of Stellar asset identity (Horizon-aligned) and
   `/assets/:id`'s sub-resource (`/transactions`) kills every multi-segment
   alternative on routing-arity grounds. A uniform-contract-id scheme (derive
   the SAC `C…` for every asset — feasible via `xdr-parser::derive_sac_contract_id`)
   was rejected: it needs a read-path reverse-lookup and diverges from Horizon.
2. **Reserved `native` token.** The classic XLM singleton (`asset_type=0`) has
   no composite identity (`ck_assets_identity`), so it is unaddressable under
   pure Option A — added `/assets/native`. (FE XLM detail reads `asset_type=0`.)
3. **Separator stays `-`** (`CODE-ISSUER`). Briefly swapped to `:` (Horizon
   canonical) then reverted on request — `:` URL-encodes to `%3A` and
   stellar.expert uses `-`.
4. **`route_token` replaces `surrogate_id` for the whole search wire**, not just
   assets — the surrogate preference in `routeForHit` was broken for
   account/contract too (their detail routes reject numerics); routing on
   `route_token ?? identifier` fixes all three at once.
5. **assets-tx CH driver uses `LIMIT 1 BY … LIMIT limit+1`** (dedup-then-cut)
   rather than the PG/canonical `limit*4` over-fetch — cheaper and correct
   (reviewed + kept: the `LIMIT 1 BY` runs before the cut, yielding `limit+1`
   distinct txs).
6. **`EventItem.amount = 1` on the CH events path.** PG `amount` is the
   appearance fold-count (replicated across the expanded events); CH is per-event
   (unfolded), so each row's fold is 1. The field is a documented vestige (not a
   token amount) and is NOT surfaced by the FE, so the cheap `1` was chosen over
   a window/aggregate to reconstruct the trio count. Documented divergence.
7. **`fetch_limit()` convention chosen** (handler owns the `+1`) for this
   branch's lists + assets-tx, per the daily call — repo-wide a mix still exists
   (transactions uses query-side `+1`); normalising the whole repo is out of
   scope.

## Issues Encountered

- **Positional `clickhouse::Row` decode** — a linter reordered `AssetChRow`
  fields, silently mismatching the SELECT column order (`deployed_at_ledger` ↔
  `icon_url`) — would decode a string into `i64` at runtime (offline tests don't
  catch it). Realigned the SELECT to the struct. **Review every CH row's column
  order positionally.**
- **Worktree module resolution** — the worktree has no own `node_modules`, so
  the FE typecheck resolved `@rumblefish/api-types` from the **main repo**
  (different branch, stale, no `route_token`) → a "phantom" tsc error that
  contradicted the on-disk file. Worked around with `node_modules/@rumblefish/*`
  symlinks → worktree libs. (Worktree-hygiene gotcha for future FE work here.)
- **Search→asset regression** the numeric-drop surfaced (search emitted a key
  the detail route rejects) — fixed via `route_token` (above).

## Review Plan (hand-off)

Full plan + consistency-vs-stkrolikiewicz-conventions table archived in the
branch session. Reviewer must decide three things and run two checks:

- [ ] **assets-tx `limit+1` vs canonical `limit*4`** over-fetch
      (`assets/queries_ch.rs` driver) — accept the cheaper dedup-then-cut, or
      restore `*4` for rare-asset multi-op fan-out safety.
- [ ] **assets-list cursor binds (not inlines) the 4-tuple** — safe only because
      the keyset clause is omitted on page 1; accept the convention drift vs his
      inline-i64 cursors, or inline for uniformity.
- [ ] **Two `limit+1` conventions** in one branch (accounts-list adds it in the
      handler; contracts/assets-list in the query) — normalise or document.
- [ ] **`canonical_id` precedence == search `route_token` COALESCE** byte-for-byte
      (`assets/handlers.rs` ↔ `search/queries.rs`).
- [ ] **Positional decode** — `AssetChRow` ↔ `ASSET_CH_SELECT` column-for-column.
- [x] **Contracts events split-brain** (was highest rollout risk) — RESOLVED:
      `list_events` now dispatches CH (`queries_ch::fetch_events`), so with
      `CONTRACTS=ch` live all four read endpoints serve fresh CH (was: events
      stale on frozen PG since the 0241 indexer cutover). Needs deploy to take
      effect on prod.
- [ ] **Operator read-rows smoke** before any prod flip — accounts-list
      (highest: non-PK `last_seen` sort + FINAL + join), assets-tx (non-PK
      identity seek), contracts-list (non-PK `id` sort).

## Future Work (spawn as backlog tasks **on develop**, per project convention)

- **CH-mode parity tests** — no integration test exercises `API_DATASOURCE_*=ch`
  today (all PG-gated); the task AC "Integration tests pass in Ch mode" is met
  only by staging smoke. A CH-mode shape-equality smoke would close it.
- ~~Contracts events split-brain reconcile~~ — DONE this branch (events → CH).
- **Remaining modules:** liquidity_pools, nfts, search → CH (stkrolikiewicz's
  recommended order: LP → NFTs → Search).
- **0231** must land (or run alongside) before `ASSETS=ch` prod flip — else CH
  `assets.{icon_url,name}` are NULL (regression vs PG enrichment).
