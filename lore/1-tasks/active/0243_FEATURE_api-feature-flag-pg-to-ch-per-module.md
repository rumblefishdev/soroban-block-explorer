---
id: '0243'
title: 'FEATURE: API feature flag per module — gradual PG↔CH migration for all 9 handler modules'
type: FEATURE
status: active
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
  - date: '2026-06-09'
    status: active
    who: stkrolikiewicz
    note: >
      Corrected status backlog → active to match the active/ directory and
      ongoing work (commits reference lore-0243; #251 flipped
      API_DATASOURCE_LIQUIDITY_POOLS to ch in IaC). Blockers 0241/0239/0240 are
      resolved (archived); `blocked_by` retained as a historical record.
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
